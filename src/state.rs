use reqwest::Client;
use teloxide::{dispatching::dialogue::InMemStorage, prelude::*};

use crate::assets::AssetManager;
use crate::controls::inventory::InventoryView;
use crate::session::SessionManager;
use crate::text::TextComposer;

pub type DialogueState = Dialogue<State, InMemStorage<State>>;
pub type HandlerResult = Result<(), Box<dyn std::error::Error + Send + Sync>>;

/// The bot's top-level FSM. Lives here because it's dispatch machinery, not
/// because any particular screen's data belongs here — `InventoryView` is
/// owned and defined by `controls::inventory`; this just references it.
#[derive(Clone, Default)]
pub enum State {
    #[default]
    Start,
    Inventory(InventoryView),
}

#[derive(Clone)]
pub struct AppState {
    pub composer: TextComposer,
    pub client: Client,
    pub session_manager: SessionManager,
    pub config: Config,
    pub asset_manager: AssetManager,
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
    pub sentences_db_path: String,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            proxy_url: std::env::var("TELOXIDE_PROXY")
                .ok()
                .or_else(|| Some("socks5h://127.0.0.1:9050".to_string())),
            morphology_db_path: std::env::var("MORPHOLOGY_DB")
                .unwrap_or_else(|_| "assets/text/morphology.db".to_string()),
            words_db_path: std::env::var("WORDS_DB")
                .unwrap_or_else(|_| "assets/text/words.db".to_string()),
            app_db_path: std::env::var("APP_DB").unwrap_or_else(|_| "app_database".to_string()),
            model_dir: std::env::var("MODEL_DIR")
                .unwrap_or_else(|_| "assets/robert-tiny2".to_string()),
            image_cache_dir: std::env::var("IMAGE_CACHE_DIR")
                .unwrap_or_else(|_| ".image_cache".to_string()),
            connect_timeout_secs: 5,
            sentences_db_path: std::env::var("SENTENCES_DB")
                .unwrap_or_else(|_| "assets/text/sentences.db".to_string()),
        }
    }
}

impl Config {
    pub fn from_env() -> Self {
        dotenvy::dotenv().ok();
        Self::default()
    }
}
