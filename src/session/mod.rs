use std::sync::{Arc, OnceLock};
use std::time::Duration;

use moka::future::Cache;
use native_db::*;
use native_model::{Model, native_model};
use serde::{Deserialize, Serialize};
use tokio::sync::Mutex as AsyncMutex;

use crate::content::inventory::Inventory;
use crate::content::modifier::{HookArg, ModifierBag, ModifierRegistry, warn_once};
use crate::utils::CappedBuffer;

const LOCK_STRIPES: usize = 256;

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[native_model(id = 1, version = 1)]
#[native_db]
pub struct UserSession {
    #[primary_key]
    pub user_id: u64,

    pub rolls: CappedBuffer<i64>,
    pub between_rolls: CappedBuffer<i64>,
    pub modifiers: ModifierBag,

    #[serde(default)]
    pub inventory: Inventory,
}

impl UserSession {
    pub fn new(user_id: u64) -> Self {
        Self {
            user_id,
            inventory: Inventory::default()
                .with_category("equip", 1)
                .with_category("inventory", 3),
            rolls: CappedBuffer::default(),
            between_rolls: CappedBuffer::default(),
            modifiers: ModifierBag::default(),
        }
    }

    pub fn fire_trigger(&mut self, name: &str) {
        let registry = ModifierRegistry::global();
        if !registry.known_triggers.contains(name) {
            warn_once("trigger", name);
        }

        for def in registry.all() {
            if def.triggers.contains(&name) {
                let json_str = self
                    .modifiers
                    .entries
                    .remove(def.id)
                    .unwrap_or_else(|| "null".to_string());
                let mut json_val: serde_json::Value =
                    serde_json::from_str(&json_str).unwrap_or(serde_json::Value::Null);

                (def.dispatch_trigger)(name, &mut json_val, self);

                self.modifiers
                    .entries
                    .insert(def.id.to_string(), json_val.to_string());
            }
        }
    }

    #[allow(dead_code)]
    pub fn fire_hook<T>(&self, name: &str, value: T) -> T {
        let registry = ModifierRegistry::global();
        if !registry.known_hooks.contains(name) {
            warn_once("hook", name);
        }

        let mut hook_arg = HookArg::new(name, value);

        for def in registry.all() {
            if def.hooks.contains(&name) {
                let default_str = "null".to_string();
                let json_str = self.modifiers.entries.get(def.id).unwrap_or(&default_str);
                let json_val: serde_json::Value =
                    serde_json::from_str(json_str).unwrap_or(serde_json::Value::Null);

                hook_arg = (def.dispatch_hook)(&json_val, self, hook_arg);
            }
        }

        hook_arg.into_value::<T>()
    }
}

// Global static model definitions registry required by native_db 0.8
static MODELS: OnceLock<Models> = OnceLock::new();

fn get_models() -> Result<&'static Models, db_type::Error> {
    if let Some(models) = MODELS.get() {
        return Ok(models);
    }
    let mut models = Models::new();
    models.define::<UserSession>()?;
    let _ = MODELS.set(models);
    Ok(MODELS.get().unwrap())
}

#[derive(Clone)]
pub struct SessionManager {
    db: Arc<Database<'static>>,
    cache: Cache<u64, Arc<UserSession>>,
    striped_locks: Arc<[AsyncMutex<()>; LOCK_STRIPES]>,
}

impl SessionManager {
    pub fn new(database_path: &str) -> anyhow::Result<Self> {
        let models = get_models().map_err(|e| anyhow::anyhow!("{e:?}"))?;
        let builder = Builder::new();

        let db = if database_path == ":memory:" {
            builder.create_in_memory(models)?
        } else {
            let path = std::path::Path::new(database_path);
            if let Some(parent) = path.parent() {
                if !parent.as_os_str().is_empty() {
                    std::fs::create_dir_all(parent)?;
                }
            }

            if path.exists() {
                builder.open(models, path)?
            } else {
                builder.create(models, path)?
            }
        };

        let cache = Cache::builder()
            .max_capacity(10_000)
            .time_to_idle(Duration::from_secs(15 * 60))
            .build();

        let striped_locks = Arc::new(std::array::from_fn(|_| AsyncMutex::new(())));

        Ok(Self {
            db: Arc::new(db),
            cache,
            striped_locks,
        })
    }

    #[inline]
    fn get_user_lock(&self, user_id: u64) -> &AsyncMutex<()> {
        &self.striped_locks[(user_id as usize) % LOCK_STRIPES]
    }

    pub async fn get_session(&self, user_id: u64) -> anyhow::Result<UserSession> {
        if let Some(session) = self.cache.get(&user_id).await {
            return Ok((*session).clone());
        }

        let r = self.db.r_transaction()?;
        let session = match r.get().primary(user_id) {
            Ok(Some(s)) => s,
            Ok(None) => UserSession::new(user_id),
            Err(e) => {
                log::warn!(
                    "Corrupted or incompatible session for user {user_id}: {e:?}. Resetting to default."
                );
                UserSession::new(user_id)
            }
        };

        self.cache.insert(user_id, Arc::new(session.clone())).await;
        Ok(session)
    }

    pub async fn save_session(&self, session: &UserSession) -> anyhow::Result<()> {
        self.cache
            .insert(session.user_id, Arc::new(session.clone()))
            .await;

        let rw = self.db.rw_transaction()?;
        rw.upsert(session.clone())?;
        rw.commit()?;

        Ok(())
    }

    pub async fn mutate_session<F, R>(&self, user_id: u64, f: F) -> anyhow::Result<R>
    where
        F: FnOnce(&mut UserSession) -> R,
    {
        let lock = self.get_user_lock(user_id);
        let _guard = lock.lock().await;

        let mut session = self.get_session(user_id).await?;
        let result = f(&mut session);
        self.save_session(&session).await?;
        Ok(result)
    }

    #[allow(dead_code)]
    pub async fn fire_trigger(&self, user_id: u64, name: &str) -> anyhow::Result<()> {
        self.mutate_session(user_id, |session| {
            session.fire_trigger(name);
        })
        .await
    }

    #[allow(dead_code)]
    pub async fn fire_hook<T: Default>(&self, user_id: u64, name: &str) -> anyhow::Result<T> {
        let session = self.get_session(user_id).await?;
        Ok(session.fire_hook(name, T::default()))
    }

    #[allow(dead_code)]
    pub async fn fire_hook_with<T>(&self, user_id: u64, name: &str, value: T) -> anyhow::Result<T> {
        let session = self.get_session(user_id).await?;
        Ok(session.fire_hook(name, value))
    }
}
