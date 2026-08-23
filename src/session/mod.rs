use serde::{Deserialize, Serialize};
use sqlx::sqlite::SqliteConnectOptions;
use sqlx::{Pool, Sqlite, SqlitePool};

mod utils;

pub use utils::unix_timestamp;

#[derive(Clone, Serialize, Deserialize, Default)]
pub struct UserSession {
    pub user_id: u64,
    pub last_roll: i64,
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

        // Initialize table
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
}
