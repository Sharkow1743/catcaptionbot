use reqwest::Client;
use teloxide::{dispatching::dialogue::InMemStorage, prelude::*};

use crate::session::SessionManager;
use crate::text::TextComposer;

pub type DialogueState = Dialogue<State, InMemStorage<State>>;
pub type HandlerResult = Result<(), Box<dyn std::error::Error + Send + Sync>>;

#[derive(Clone, Default)]
pub enum State {
    #[default]
    Start,
}

#[derive(Clone)]
pub struct AppState {
    pub composer: TextComposer,
    pub client: Client,
    pub session_manager: SessionManager,
    pub config: Config,
}

#[derive(Debug, Clone)]
pub struct Config {
    pub proxy_url: Option<String>,
    pub morphology_db_path: String,
    pub words_db_path: String,
    pub app_db_path: String,
    pub model_dir: String,
    pub image_cache_dir: String,
    pub connect_timeout_secs: u64,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            proxy_url: std::env::var("TELOXIDE_PROXY")
                .ok()
                .or_else(|| Some("socks5h://127.0.0.1:8086".to_string())),
            morphology_db_path: std::env::var("MORPHOLOGY_DB")
                .unwrap_or_else(|_| "morphology.db".to_string()),
            words_db_path: std::env::var("WORDS_DB").unwrap_or_else(|_| "words.db".to_string()),
            app_db_path: std::env::var("APP_DB").unwrap_or_else(|_| "app.db".to_string()),
            model_dir: std::env::var("MODEL_DIR").unwrap_or_else(|_| "robert-tiny2".to_string()),
            image_cache_dir: std::env::var("IMAGE_CACHE_DIR")
                .unwrap_or_else(|_| ".image_cache".to_string()),
            connect_timeout_secs: 30,
        }
    }
}

impl Config {
    pub fn from_env() -> Self {
        dotenvy::dotenv().ok();
        Self::default()
    }
}
