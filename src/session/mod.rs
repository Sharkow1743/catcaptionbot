use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;

use bonsaidb::core::schema::{Collection, SerializedCollection};
use bonsaidb::local::AsyncDatabase;
use bonsaidb::local::config::{Builder, StorageConfiguration};
use moka::future::Cache;
use serde::{Deserialize, Serialize};
use teloxide::types::MessageId;
use tokio::sync::{Mutex as AsyncMutex, Notify};

use crate::content::Content;
use crate::content::inventory::Inventory;
use crate::content::modifier::{HookArg, ModifierBag, ModifierRegistry, warn_once};
use crate::utils::CappedBuffer;

const LOCK_STRIPES: usize = 256;

// =========================================================================
// 1. Persistent Profile (Saved to Database)
// =========================================================================

#[derive(Debug, Clone, Serialize, Deserialize, Collection, Default)]
#[collection(name = "user-profiles", primary_key = u64)]
pub struct UserProfile {
    #[natural_id]
    pub user_id: u64,
    pub inventory: Inventory,
}

// =========================================================================
// 2. Ephemeral In-Memory Stats (Never touches Disk)
// =========================================================================

#[derive(Debug, Clone, Default)]
pub struct EphemeralStats {
    pub rolls: CappedBuffer<i64>,
    pub between_rolls: CappedBuffer<i64>,
    pub last_roll_msg_id: Option<MessageId>,
    pub last_content: Option<Content>,
    pub modifiers: ModifierBag,
}

// =========================================================================
// 3. Combined Active Session Representation
// =========================================================================

#[derive(Debug, Clone, Default)]
pub struct UserSession {
    pub user_id: u64,

    // Ephemeral stats (in-memory only)
    pub rolls: CappedBuffer<i64>,
    pub between_rolls: CappedBuffer<i64>,
    pub last_roll_msg_id: Option<MessageId>,
    pub last_content: Option<Content>,
    pub modifiers: ModifierBag,

    // Persistent attributes
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
            last_roll_msg_id: None,
            last_content: None,
        }
    }

    pub fn to_profile(&self) -> UserProfile {
        UserProfile {
            user_id: self.user_id,
            inventory: self.inventory.clone(),
        }
    }

    pub fn from_profile(profile: UserProfile) -> Self {
        let mut session = Self::new(profile.user_id);
        session.inventory = profile.inventory;
        session
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

// =========================================================================
// 4. Session Manager with Write-Behind Queue
// =========================================================================

#[derive(Clone)]
pub struct SessionManager {
    db: AsyncDatabase,
    cache: Cache<u64, Arc<UserSession>>,
    striped_locks: Arc<[AsyncMutex<()>; LOCK_STRIPES]>,
    pending_writes: Arc<AsyncMutex<HashMap<u64, UserProfile>>>,
    flush_notify: Arc<Notify>,
}

impl SessionManager {
    pub fn new(database_path: &str) -> anyhow::Result<Self> {
        let storage_config = if database_path == ":memory:" {
            StorageConfiguration::new("memory.bonsaidb").memory_only()
        } else {
            let path = std::path::Path::new(database_path);
            if let Some(parent) = path.parent() {
                if !parent.as_os_str().is_empty() {
                    std::fs::create_dir_all(parent)?;
                }
            }
            StorageConfiguration::new(path)
        };

        let db = bonsaidb::local::Database::open::<UserProfile>(storage_config)?.into_async();

        let cache = Cache::builder()
            .max_capacity(10_000)
            .time_to_idle(Duration::from_secs(15 * 60))
            .build();

        let striped_locks = Arc::new(std::array::from_fn(|_| AsyncMutex::new(())));
        let pending_writes = Arc::new(AsyncMutex::new(HashMap::new()));
        let flush_notify = Arc::new(Notify::new());

        // Spawn background worker for debounced disk persistence
        {
            let db = db.clone();
            let pending_writes = pending_writes.clone();
            let flush_notify = flush_notify.clone();

            tokio::spawn(async move {
                loop {
                    tokio::select! {
                        _ = flush_notify.notified() => {
                            // Small debounce window to batch rapid successive clicks
                            tokio::time::sleep(Duration::from_millis(500)).await;
                        }
                        _ = tokio::time::sleep(Duration::from_secs(3)) => {}
                    }

                    Self::flush_pending(&db, &pending_writes).await;
                }
            });
        }

        Ok(Self {
            db,
            cache,
            striped_locks,
            pending_writes,
            flush_notify,
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

        // Cache miss: load persistent profile from database
        let session = match UserProfile::get_async(&user_id, &self.db).await {
            Ok(Some(doc)) => {
                let mut profile = doc.contents;
                profile.user_id = user_id;
                UserSession::from_profile(profile)
            }
            Ok(None) => UserSession::new(user_id),
            Err(e) => {
                log::warn!(
                    "Failed to retrieve profile for user {user_id}: {e:?}. Resetting to default."
                );
                UserSession::new(user_id)
            }
        };

        self.cache.insert(user_id, Arc::new(session.clone())).await;
        Ok(session)
    }

    /// Updates memory cache instantly and queues profile for write-behind persistence.
    pub async fn save_session(&self, session: &UserSession) -> anyhow::Result<()> {
        self.cache
            .insert(session.user_id, Arc::new(session.clone()))
            .await;

        {
            let mut pending = self.pending_writes.lock().await;
            pending.insert(session.user_id, session.to_profile());
        }
        self.flush_notify.notify_one();

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

    async fn flush_pending(
        db: &AsyncDatabase,
        pending_writes: &AsyncMutex<HashMap<u64, UserProfile>>,
    ) {
        let batch: Vec<UserProfile> = {
            let mut pending = pending_writes.lock().await;
            if pending.is_empty() {
                return;
            }
            pending.drain().map(|(_, p)| p).collect()
        };

        for profile in batch {
            if let Err(e) =
                UserProfile::overwrite_async(&profile.user_id, profile.clone(), db).await
            {
                log::error!(
                    "Failed to persist profile for user {}: {e:?}",
                    profile.user_id
                );
            }
        }
    }

    /// Flushes all queued writes to disk immediately.
    pub async fn flush(&self) -> anyhow::Result<()> {
        Self::flush_pending(&self.db, &self.pending_writes).await;
        Ok(())
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
