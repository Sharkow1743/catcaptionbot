pub mod migration;
pub mod stats;

use std::any::Any;
use std::collections::{HashMap, HashSet};
use std::ops::{Deref, DerefMut};
use std::sync::Arc;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use bonsaidb::core::schema::{Collection, Schema, SerializedCollection};
use bonsaidb::core::transmog_pot::pot;
use bonsaidb::local::AsyncDatabase;
use bonsaidb::local::config::{Builder, StorageConfiguration};
use migration::{CURRENT_PROFILE_SCHEMA_VERSION, MigrationManager};
use moka::future::Cache;
use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};
use stats::UserStats;
use teloxide::types::MessageId;
use tokio::sync::{Mutex as AsyncMutex, Notify, RwLock as AsyncRwLock};

use crate::content::Content;
use crate::content::inventory::Inventory;
use crate::content::modifier::{HookArg, ModifierBag, ModifierRegistry, warn_once};
use crate::utils::CappedBuffer;
use crate::utils::short_id::ShortId;

const LOCK_STRIPES: usize = 256;

fn current_time_secs() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

#[derive(Debug, Schema)]
#[schema(
    name = "session-database",
    collections = [UserProfile, PersistentSharedRecord]
)]
pub struct SessionSchema;

#[derive(Debug, Clone, Serialize, Deserialize, Collection)]
#[collection(name = "user-profiles", primary_key = u64)]
#[serde(default)]
pub struct UserProfile {
    #[natural_id]
    pub user_id: u64,
    pub schema_version: u32,
    pub inventory: Inventory,
    pub stats: UserStats,
    pub shards: HashMap<String, u32>,
}

impl UserProfile {
    pub fn new(user_id: u64) -> Self {
        Self {
            user_id,
            schema_version: CURRENT_PROFILE_SCHEMA_VERSION,
            inventory: Inventory::default()
                .with_category("equip", 1)
                .with_category("inventory", 3),
            stats: UserStats::default(),
            shards: HashMap::new(),
        }
    }
}

impl Default for UserProfile {
    fn default() -> Self {
        Self {
            user_id: 0,
            schema_version: 0, // Defaults missing database records to schema version 0
            inventory: Inventory::default()
                .with_category("equip", 1)
                .with_category("inventory", 3),
            stats: UserStats::default(),
            shards: HashMap::new(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, Collection)]
#[collection(name = "shared-records", primary_key = u32)]
pub struct PersistentSharedRecord {
    #[natural_id]
    pub id: u32,
    pub data: Vec<u8>,
    pub related_users: HashSet<u64>,
    pub updated_at_secs: u64,
}

#[derive(Debug, Clone, Default)]
pub struct UserSession {
    pub profile: UserProfile,
    pub rolls: CappedBuffer<i64>,
    pub between_rolls: CappedBuffer<i64>,
    pub last_roll_msg_id: Option<MessageId>,
    pub last_content: Option<Content>,
    pub modifiers: ModifierBag,
}

impl Deref for UserSession {
    type Target = UserProfile;

    #[inline]
    fn deref(&self) -> &Self::Target {
        &self.profile
    }
}

impl DerefMut for UserSession {
    #[inline]
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.profile
    }
}

impl UserSession {
    pub fn new(user_id: u64) -> Self {
        Self {
            profile: UserProfile::new(user_id),
            ..Default::default()
        }
    }

    #[inline]
    pub fn to_profile(&self) -> UserProfile {
        self.profile.clone()
    }

    #[inline]
    pub fn from_profile(profile: UserProfile) -> Self {
        Self {
            profile,
            ..Default::default()
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

struct EphemeralEntry {
    data: Arc<dyn Any + Send + Sync>,
    related_users: HashSet<u64>,
    #[allow(dead_code)]
    updated_at_secs: u64,
}

#[derive(Default)]
pub struct EphemeralStore {
    entries: AsyncRwLock<HashMap<ShortId, EphemeralEntry>>,
    user_index: AsyncRwLock<HashMap<u64, HashSet<ShortId>>>,
}

impl EphemeralStore {
    pub async fn insert<T: Send + Sync + 'static>(
        &self,
        value: T,
        users: impl IntoIterator<Item = u64>,
    ) -> ShortId {
        let id = ShortId::now_v7();
        self.set(id, value, users).await;
        id
    }

    pub async fn set<T: Send + Sync + 'static>(
        &self,
        id: ShortId,
        value: T,
        users: impl IntoIterator<Item = u64>,
    ) {
        let user_set: HashSet<u64> = users.into_iter().collect();

        let mut entries = self.entries.write().await;
        let mut user_index = self.user_index.write().await;

        if let Some(old) = entries.get(&id) {
            for uid in &old.related_users {
                if !user_set.contains(uid) {
                    if let Some(set) = user_index.get_mut(uid) {
                        set.remove(&id);
                        if set.is_empty() {
                            user_index.remove(uid);
                        }
                    }
                }
            }
        }

        for uid in &user_set {
            user_index.entry(*uid).or_default().insert(id);
        }

        entries.insert(
            id,
            EphemeralEntry {
                data: Arc::new(value),
                related_users: user_set,
                updated_at_secs: current_time_secs(),
            },
        );
    }

    pub async fn get<T: Clone + Send + Sync + 'static>(&self, id: ShortId) -> Option<T> {
        let entries = self.entries.read().await;
        entries.get(&id)?.data.downcast_ref::<T>().cloned()
    }

    pub async fn mutate<T: Clone + Send + Sync + 'static, R>(
        &self,
        id: ShortId,
        f: impl FnOnce(&mut T) -> R,
    ) -> Option<R> {
        let mut entries = self.entries.write().await;
        let entry = entries.get_mut(&id)?;

        let mut value = entry.data.downcast_ref::<T>()?.clone();
        let res = f(&mut value);
        entry.data = Arc::new(value);
        entry.updated_at_secs = current_time_secs();
        Some(res)
    }

    pub async fn remove(&self, id: ShortId) -> bool {
        let mut entries = self.entries.write().await;
        let removed = match entries.remove(&id) {
            Some(entry) => entry,
            None => return false,
        };

        let mut user_index = self.user_index.write().await;
        for uid in &removed.related_users {
            if let Some(set) = user_index.get_mut(uid) {
                set.remove(&id);
                if set.is_empty() {
                    user_index.remove(uid);
                }
            }
        }

        true
    }

    pub async fn find_by_user(&self, user_id: u64) -> Vec<ShortId> {
        let user_index = self.user_index.read().await;
        user_index
            .get(&user_id)
            .map(|s| s.iter().copied().collect())
            .unwrap_or_default()
    }

    pub async fn find_by_user_typed<T: Clone + Send + Sync + 'static>(
        &self,
        user_id: u64,
    ) -> Vec<(ShortId, T)> {
        let user_index = self.user_index.read().await;
        let entries = self.entries.read().await;

        if let Some(ids) = user_index.get(&user_id) {
            ids.iter()
                .filter_map(|id| {
                    let entry = entries.get(id)?;
                    let val = entry.data.downcast_ref::<T>()?.clone();
                    Some((*id, val))
                })
                .collect()
        } else {
            Vec::new()
        }
    }

    pub async fn link_user(&self, id: ShortId, user_id: u64) -> bool {
        let mut entries = self.entries.write().await;
        if let Some(entry) = entries.get_mut(&id) {
            if entry.related_users.insert(user_id) {
                let mut user_index = self.user_index.write().await;
                user_index.entry(user_id).or_default().insert(id);
                return true;
            }
        }
        false
    }

    pub async fn unlink_user(&self, id: ShortId, user_id: u64) -> bool {
        let mut entries = self.entries.write().await;
        if let Some(entry) = entries.get_mut(&id) {
            if entry.related_users.remove(&user_id) {
                let mut user_index = self.user_index.write().await;
                if let Some(set) = user_index.get_mut(&user_id) {
                    set.remove(&id);
                    if set.is_empty() {
                        user_index.remove(&user_id);
                    }
                }
                return true;
            }
        }
        false
    }

    pub async fn all_typed<T: Clone + Send + Sync + 'static>(&self) -> Vec<(ShortId, T)> {
        let entries = self.entries.read().await;
        entries
            .iter()
            .filter_map(|(id, entry)| {
                let val = entry.data.downcast_ref::<T>()?.clone();
                Some((*id, val))
            })
            .collect()
    }
}

#[derive(Clone)]
pub struct SessionManager {
    db: AsyncDatabase,
    cache: Cache<u64, Arc<UserSession>>,
    striped_locks: Arc<[AsyncMutex<()>; LOCK_STRIPES]>,
    pending_profile_writes: Arc<AsyncMutex<HashMap<u64, UserProfile>>>,
    flush_notify: Arc<Notify>,
    pub ephemeral_store: Arc<EphemeralStore>,
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

        let db = bonsaidb::local::Database::open::<SessionSchema>(storage_config)?.into_async();

        let cache = Cache::builder()
            .max_capacity(10_000)
            .time_to_idle(Duration::from_secs(15 * 60))
            .build();

        let striped_locks = Arc::new(std::array::from_fn(|_| AsyncMutex::new(())));
        let pending_profile_writes = Arc::new(AsyncMutex::new(HashMap::new()));
        let flush_notify = Arc::new(Notify::new());
        let ephemeral_store = Arc::new(EphemeralStore::default());

        {
            let db = db.clone();
            let pending_profile_writes = pending_profile_writes.clone();
            let flush_notify = flush_notify.clone();

            tokio::spawn(async move {
                loop {
                    tokio::select! {
                        _ = flush_notify.notified() => {
                            tokio::time::sleep(Duration::from_millis(500)).await;
                        }
                        _ = tokio::time::sleep(Duration::from_secs(3)) => {}
                    }

                    Self::flush_pending_profiles(&db, &pending_profile_writes).await;
                }
            });
        }

        Ok(Self {
            db,
            cache,
            striped_locks,
            pending_profile_writes,
            flush_notify,
            ephemeral_store,
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

        let session = match UserProfile::get_async(&user_id, &self.db).await {
            Ok(Some(doc)) => {
                let mut profile = doc.contents;
                profile.user_id = user_id;

                let migration_mgr = MigrationManager::new();
                let was_migrated = migration_mgr.migrate(&mut profile);

                let session = UserSession::from_profile(profile);
                if was_migrated {
                    let _ = self.save_session(&session).await;
                }

                session
            }
            Ok(None) => UserSession::new(user_id),
            Err(e) => {
                log::warn!(
                    "Failed to retrieve profile for user {user_id}: {e:?}. Resetting to default"
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

        {
            let mut pending = self.pending_profile_writes.lock().await;
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

    async fn flush_pending_profiles(
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

    pub async fn flush(&self) -> anyhow::Result<()> {
        Self::flush_pending_profiles(&self.db, &self.pending_profile_writes).await;
        Ok(())
    }

    #[inline]
    pub async fn insert_ephemeral<T: Send + Sync + 'static>(
        &self,
        value: T,
        users: impl IntoIterator<Item = u64>,
    ) -> ShortId {
        self.ephemeral_store.insert(value, users).await
    }

    #[inline]
    pub async fn set_ephemeral<T: Send + Sync + 'static>(
        &self,
        id: ShortId,
        value: T,
        users: impl IntoIterator<Item = u64>,
    ) {
        self.ephemeral_store.set(id, value, users).await;
    }

    #[inline]
    pub async fn get_ephemeral<T: Clone + Send + Sync + 'static>(&self, id: ShortId) -> Option<T> {
        self.ephemeral_store.get(id).await
    }

    #[inline]
    pub async fn mutate_ephemeral<T: Clone + Send + Sync + 'static, R>(
        &self,
        id: ShortId,
        f: impl FnOnce(&mut T) -> R,
    ) -> Option<R> {
        self.ephemeral_store.mutate(id, f).await
    }

    #[inline]
    pub async fn remove_ephemeral(&self, id: ShortId) -> bool {
        self.ephemeral_store.remove(id).await
    }

    #[inline]
    pub async fn find_ephemeral_by_user(&self, user_id: u64) -> Vec<ShortId> {
        self.ephemeral_store.find_by_user(user_id).await
    }

    #[inline]
    pub async fn find_ephemeral_by_user_typed<T: Clone + Send + Sync + 'static>(
        &self,
        user_id: u64,
    ) -> Vec<(ShortId, T)> {
        self.ephemeral_store.find_by_user_typed(user_id).await
    }

    #[inline]
    pub async fn link_user_ephemeral(&self, id: ShortId, user_id: u64) -> bool {
        self.ephemeral_store.link_user(id, user_id).await
    }

    #[inline]
    pub async fn unlink_user_ephemeral(&self, id: ShortId, user_id: u64) -> bool {
        self.ephemeral_store.unlink_user(id, user_id).await
    }

    #[inline]
    pub async fn all_ephemeral_typed<T: Clone + Send + Sync + 'static>(&self) -> Vec<(ShortId, T)> {
        self.ephemeral_store.all_typed().await
    }

    pub async fn insert_persistent<T: Serialize>(
        &self,
        value: &T,
        users: impl IntoIterator<Item = u64>,
    ) -> anyhow::Result<ShortId> {
        let id = ShortId::now_v7();
        self.set_persistent(id, value, users).await?;
        Ok(id)
    }

    pub async fn set_persistent<T: Serialize>(
        &self,
        id: ShortId,
        value: &T,
        users: impl IntoIterator<Item = u64>,
    ) -> anyhow::Result<()> {
        let payload = pot::to_vec(value)?;
        let record = PersistentSharedRecord {
            id: id.as_u32(),
            data: payload,
            related_users: users.into_iter().collect(),
            updated_at_secs: current_time_secs(),
        };

        PersistentSharedRecord::overwrite_async(&id.as_u32(), record, &self.db).await?;
        Ok(())
    }

    pub async fn get_persistent<T: DeserializeOwned>(
        &self,
        id: ShortId,
    ) -> anyhow::Result<Option<T>> {
        let doc = PersistentSharedRecord::get_async(&id.as_u32(), &self.db).await?;
        match doc {
            Some(doc) => {
                let val: T = pot::from_slice(&doc.contents.data)?;
                Ok(Some(val))
            }
            None => Ok(None),
        }
    }

    pub async fn mutate_persistent<T, R, F>(&self, id: ShortId, f: F) -> anyhow::Result<Option<R>>
    where
        T: Serialize + DeserializeOwned,
        F: FnOnce(&mut T) -> R,
    {
        let u32_id = id.as_u32();
        if let Some(doc) = PersistentSharedRecord::get_async(&u32_id, &self.db).await? {
            let mut record = doc.contents;
            let mut val: T = pot::from_slice(&record.data)?;
            let result = f(&mut val);
            record.data = pot::to_vec(&val)?;
            record.updated_at_secs = current_time_secs();
            PersistentSharedRecord::overwrite_async(&u32_id, record, &self.db).await?;
            Ok(Some(result))
        } else {
            Ok(None)
        }
    }

    pub async fn remove_persistent(&self, id: ShortId) -> anyhow::Result<bool> {
        let u32_id = id.as_u32();
        if let Some(doc) = PersistentSharedRecord::get_async(&u32_id, &self.db).await? {
            doc.delete_async(&self.db).await?;
            Ok(true)
        } else {
            Ok(false)
        }
    }

    pub async fn find_persistent_by_user(
        &self,
        user_id: u64,
    ) -> anyhow::Result<Vec<(ShortId, PersistentSharedRecord)>> {
        let docs = PersistentSharedRecord::all_async(&self.db).await?;
        let matching = docs
            .into_iter()
            .filter_map(|doc| {
                if doc.contents.related_users.contains(&user_id) {
                    Some((ShortId::from(doc.contents.id), doc.contents))
                } else {
                    None
                }
            })
            .collect();
        Ok(matching)
    }

    pub async fn find_persistent_by_user_typed<T: DeserializeOwned>(
        &self,
        user_id: u64,
    ) -> anyhow::Result<Vec<(ShortId, T)>> {
        let docs = PersistentSharedRecord::all_async(&self.db).await?;
        let mut results = Vec::new();
        for doc in docs {
            if doc.contents.related_users.contains(&user_id) {
                if let Ok(val) = pot::from_slice::<T>(&doc.contents.data) {
                    results.push((ShortId::from(doc.contents.id), val));
                }
            }
        }
        Ok(results)
    }

    pub async fn link_user_persistent(&self, id: ShortId, user_id: u64) -> anyhow::Result<bool> {
        let u32_id = id.as_u32();
        if let Some(doc) = PersistentSharedRecord::get_async(&u32_id, &self.db).await? {
            let mut record = doc.contents;
            if record.related_users.insert(user_id) {
                record.updated_at_secs = current_time_secs();
                PersistentSharedRecord::overwrite_async(&u32_id, record, &self.db).await?;
                return Ok(true);
            }
        }
        Ok(false)
    }

    pub async fn unlink_user_persistent(&self, id: ShortId, user_id: u64) -> anyhow::Result<bool> {
        let u32_id = id.as_u32();
        if let Some(doc) = PersistentSharedRecord::get_async(&u32_id, &self.db).await? {
            let mut record = doc.contents;
            if record.related_users.remove(&user_id) {
                record.updated_at_secs = current_time_secs();
                PersistentSharedRecord::overwrite_async(&u32_id, record, &self.db).await?;
                return Ok(true);
            }
        }
        Ok(false)
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
