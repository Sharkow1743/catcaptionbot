mod commands;
mod content;
mod controls;
mod macros;
mod state;
mod text;

use std::sync::Arc;
use std::time::Duration;

use commands::{Command, handle_help, handle_start};
use content::rarity::RarityManager;
use content::{WorkerPoolConfig, content_queue, spawn_producer};
use controls::handle_keyboard_callback;
use controls::kb_command::UpdateKbExt;
use flume::Receiver;
use state::State;
use text::WordGenerator;
use text::{Morpher, TextComposer};

use libsqlite3_sys::sqlite3_auto_extension;
use reqwest::Client;
use sqlite_vec::sqlite3_vec_init;
use teloxide::{
    dispatching::dialogue::{self, InMemStorage},
    dptree::deps,
    prelude::*,
};

#[derive(Clone)]
struct AppState {
    pub composer: TextComposer,
    pub client: Client,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    pretty_env_logger::init();
    log::info!("Starting bot");

    unsafe {
        sqlite3_auto_extension(Some(std::mem::transmute(sqlite3_vec_init as *const ())));
    }

    dotenvy::dotenv().ok();

    let proxy_url =
        std::env::var("TELOXIDE_PROXY").unwrap_or_else(|_| "socks5h://127.0.0.1:8086".to_string());

    let client = reqwest::Client::builder()
        .proxy(reqwest::Proxy::all(&proxy_url)?)
        .connect_timeout(Duration::from_secs(30))
        .pool_max_idle_per_host(usize::MAX)
        .build()?;

    let bot = Bot::from_env_with_client(client.clone());

    let morpher = Morpher::new("morphology.db").await?;
    let word_generator = WordGenerator::new("words.db").await?;
    let composer = TextComposer::new(morpher, word_generator);

    let app_state = AppState { composer, client };

    let content_queue = content_queue(app_state.clone()).await;

    let handler = dialogue::enter::<Update, InMemStorage<State>, State, _>()
        .branch(
            Update::filter_message()
                .filter_command::<Command>()
                .branch(dptree::case![Command::Start(string)].endpoint(handle_start))
                .branch(dptree::case![Command::Help].endpoint(handle_help)),
        )
        .branch(Update::filter_kb_callback().endpoint(handle_keyboard_callback));

    Dispatcher::builder(bot, handler)
        .dependencies(deps![
            InMemStorage::<State>::new(),
            app_state,
            content_queue
        ])
        .enable_ctrlc_handler()
        .build()
        .dispatch()
        .await;
    Ok(())
}
