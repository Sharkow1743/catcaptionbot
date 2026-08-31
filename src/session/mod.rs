use serde::{Deserialize, Serialize};
use sqlx::SqlitePool;
use sqlx::sqlite::SqliteConnectOptions;
use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use tokio::sync::Mutex as AsyncMutex;

use crate::content::modifier::{HookArg, ModifierBag, ModifierRegistry, warn_once};
use crate::utils::CappedBuffer;

#[derive(Clone, Serialize, Deserialize, Default)]
pub struct UserSession {
    pub user_id: u64,
    pub rolls: CappedBuffer<i64>,
    pub between_rolls: CappedBuffer<i64>,
    pub modifiers: ModifierBag,
}

impl UserSession {
    pub fn fire_trigger(&mut self, name: &str) {
        let registry = ModifierRegistry::global();
        if !registry.known_triggers.contains(name) {
            warn_once("trigger", name);
        }

        for def in registry.all() {
            if def.triggers.contains(&name) {
                let mut json_val = self
                    .modifiers
                    .entries
                    .remove(def.id)
                    .unwrap_or(serde_json::Value::Null);

                (def.dispatch_trigger)(name, &mut json_val, self);

                self.modifiers.entries.insert(def.id.to_string(), json_val);
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
                let default_json = serde_json::Value::Null;
                let json_val = self.modifiers.entries.get(def.id).unwrap_or(&default_json);
                hook_arg = (def.dispatch_hook)(json_val, self, hook_arg);
            }
        }

        hook_arg.into_value::<T>()
    }
}

#[derive(Clone)]
pub struct SessionManager {
    pool: SqlitePool,
    locks: Arc<Mutex<HashMap<u64, Arc<AsyncMutex<()>>>>>,
}

impl SessionManager {
    pub async fn new(database_path: &str) -> Result<Self, sqlx::Error> {
        let pool = if database_path == ":memory:" {
            let options = SqliteConnectOptions::new()
                .filename(":memory:")
                .create_if_missing(true);
            sqlx::sqlite::SqlitePoolOptions::new()
                .max_connections(1)
                .connect_with(options)
                .await?
        } else {
            let options = SqliteConnectOptions::new()
                .filename(database_path)
                .create_if_missing(true)
                .journal_mode(sqlx::sqlite::SqliteJournalMode::Wal)
                .synchronous(sqlx::sqlite::SqliteSynchronous::Normal);
            sqlx::sqlite::SqlitePoolOptions::new()
                .max_connections(10)
                .connect_with(options)
                .await?
        };

        sqlx::query(
            "CREATE TABLE IF NOT EXISTS sessions (
                user_id INTEGER PRIMARY KEY,
                data TEXT NOT NULL
            );",
        )
        .execute(&pool)
        .await?;

        Ok(Self {
            pool,
            locks: Arc::new(Mutex::new(HashMap::new())),
        })
    }

    fn user_lock(&self, user_id: u64) -> Arc<AsyncMutex<()>> {
        self.locks
            .lock()
            .unwrap()
            .entry(user_id)
            .or_insert_with(|| Arc::new(AsyncMutex::new(())))
            .clone()
    }

    pub async fn get_session(&self, user_id: u64) -> Result<UserSession, sqlx::Error> {
        let row: Option<(String,)> = sqlx::query_as("SELECT data FROM sessions WHERE user_id = ?")
            .bind(user_id as i64)
            .fetch_optional(&self.pool)
            .await?;

        match row {
            Some((json_str,)) => {
                let session = serde_json::from_str(&json_str).unwrap_or_default();
                Ok(session)
            }
            None => Ok(UserSession {
                user_id,
                ..Default::default()
            }),
        }
    }

    pub async fn save_session(&self, session: &UserSession) -> Result<(), sqlx::Error> {
        let data = serde_json::to_string(session).unwrap();

        sqlx::query(
            "INSERT INTO sessions (user_id, data) VALUES (?, ?)
             ON CONFLICT(user_id) DO UPDATE SET data = excluded.data;",
        )
        .bind(session.user_id as i64)
        .bind(data)
        .execute(&self.pool)
        .await?;

        Ok(())
    }

    pub async fn mutate_session<F, R>(&self, user_id: u64, f: F) -> Result<R, sqlx::Error>
    where
        F: FnOnce(&mut UserSession) -> R,
    {
        let lock = self.user_lock(user_id);
        let _guard = lock.lock().await;

        let mut session = self.get_session(user_id).await?;
        let result = f(&mut session);
        self.save_session(&session).await?;
        Ok(result)
    }

    #[allow(dead_code)]
    pub async fn fire_trigger(&self, user_id: u64, name: &str) -> Result<(), sqlx::Error> {
        self.mutate_session(user_id, |session| {
            session.fire_trigger(name);
        })
        .await
    }

    #[allow(dead_code)]
    pub async fn fire_hook<T: Default>(&self, user_id: u64, name: &str) -> Result<T, sqlx::Error> {
        let session = self.get_session(user_id).await?;
        let value = session.fire_hook(name, T::default());
        Ok(value)
    }

    #[allow(dead_code)]
    pub async fn fire_hook_with<T>(
        &self,
        user_id: u64,
        name: &str,
        value: T,
    ) -> Result<T, sqlx::Error> {
        let session = self.get_session(user_id).await?;
        let value = session.fire_hook(name, value);
        Ok(value)
    }
}
