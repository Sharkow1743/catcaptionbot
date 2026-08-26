use serde::{Deserialize, Serialize};
use sqlx::SqlitePool;
use sqlx::sqlite::SqliteConnectOptions;

mod utils;
pub use utils::unix_timestamp;

use crate::content::modifier::{HookArg, ModifierBag, ModifierRegistry, warn_once};

#[derive(Clone, Serialize, Deserialize, Default)]
pub struct UserSession {
    pub user_id: u64,
    pub last_roll: i64,
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
                    .get(def.id)
                    .cloned()
                    .unwrap_or(serde_json::Value::Null);

                (def.dispatch_trigger)(name, &mut json_val, self);

                self.modifiers.entries.insert(def.id.to_string(), json_val);
            }
        }
    }

    pub fn fire_hook<T: 'static>(&self, name: &str, value: &mut T) {
        let registry = ModifierRegistry::global();
        if !registry.known_hooks.contains(name) {
            warn_once("hook", name);
        }

        let mut hook_arg = HookArg { name, value };

        for def in registry.all() {
            if def.hooks.contains(&name) {
                let default_json = serde_json::Value::Null;
                let json_val = self.modifiers.entries.get(def.id).unwrap_or(&default_json);
                (def.dispatch_hook)(json_val, self, &mut hook_arg);
            }
        }
    }
}

#[derive(Clone)]
pub struct SessionManager {
    pool: SqlitePool,
}

impl SessionManager {
    pub async fn new(database_path: &str) -> Result<Self, sqlx::Error> {
        let options = SqliteConnectOptions::new()
            .filename(database_path)
            .create_if_missing(true);

        let pool = SqlitePool::connect_with(options).await?;

        sqlx::query(
            "CREATE TABLE IF NOT EXISTS sessions (
                user_id INTEGER PRIMARY KEY,
                data TEXT NOT NULL
            );",
        )
        .execute(&pool)
        .await?;

        Ok(Self { pool })
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
        let mut session = self.get_session(user_id).await?;
        let result = f(&mut session);
        self.save_session(&session).await?;
        Ok(result)
    }

    pub async fn fire_trigger(&self, user_id: u64, name: &str) -> Result<(), sqlx::Error> {
        let mut session = self.get_session(user_id).await?;
        session.fire_trigger(name);
        self.save_session(&session).await?;
        Ok(())
    }

    pub async fn fire_hook<T: Default + 'static>(
        &self,
        user_id: u64,
        name: &str,
    ) -> Result<T, sqlx::Error> {
        let session = self.get_session(user_id).await?;
        let mut value = T::default();
        session.fire_hook(name, &mut value);
        Ok(value)
    }
}
