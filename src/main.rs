mod assets;
mod commands;
mod content;
mod controls;
mod logger;
mod macros;
mod queue;
mod session;
mod state;
mod text;
mod utils;

use std::time::Duration;

use assets::AssetManager;
use commands::{Command, handle_help, handle_start};
use content::content_queue;
use controls::handle_inventory_message;
use controls::kb_command::UpdateKbExt;
use controls::{handle_inline_callback, handle_keyboard_callback};
use session::SessionManager;
use state::{AppState, Config, State};
use text::{Morpher, TextComposer};
use text::{QuoteBank, WordGenerator};

use libsqlite3_sys::sqlite3_auto_extension;
use sqlite_vec::sqlite3_vec_init;
use teloxide::{
    dispatching::dialogue::{self, InMemStorage},
    dptree::deps,
    prelude::*,
};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    logger::builder()
        .app(logger::FilterLevel::Detail)
        .external(logger::FilterLevel::Warn)
        .init();

    log::info!("Starting bot");

    unsafe {
        sqlite3_auto_extension(Some(std::mem::transmute(sqlite3_vec_init as *const ())));
    }

    let config = Config::from_env();

    let mut client_builder = reqwest::Client::builder()
        .connect_timeout(Duration::from_secs(config.connect_timeout_secs))
        .pool_max_idle_per_host(usize::MAX);

    if let Some(ref proxy_url) = config.proxy_url {
        client_builder = client_builder.proxy(reqwest::Proxy::all(proxy_url)?);
    }

    let client = client_builder.build()?;
    let bot = Bot::from_env_with_client(client.clone());

    let morpher = Morpher::new(&config.morphology_db_path).await?;
    let word_generator =
        WordGenerator::new_with_local_model(&config.words_db_path, &config.model_dir).await?;
    let quote_bank = QuoteBank::new(&config.sentences_db_path).await?;
    let composer = TextComposer::new(morpher, word_generator, quote_bank);

    let session_manager = SessionManager::new(&config.app_db_path)?;

    // Register application assets
    let asset_manager = AssetManager::new();
    asset_manager.register_static(
        "placeholder",
        include_bytes!("../assets/images/placeholder.jpg"),
        "placeholder.jpg",
    );
    asset_manager.register_static(
        "missing_cat",
        include_bytes!("../assets/images/missing_cat.jpg"),
        "missing_cat.jpg",
    );

    let app_state = AppState {
        composer,
        client,
        session_manager,
        config,
        asset_manager,
    };

    let content_queue = content_queue(app_state.clone()).await;

    let handler = dialogue::enter::<Update, InMemStorage<State>, State, _>()
        .filter_map(|update: Update| update.from().map(|u| u.id))
        .filter_map_async(|user_id: UserId, app_state: AppState| async move {
            app_state.session_manager.get_session(user_id.0).await.ok()
        })
        .branch(
            Update::filter_message()
                .filter_command::<Command>()
                .branch(dptree::case![Command::Start(string)].endpoint(handle_start))
                .branch(dptree::case![Command::Help].endpoint(handle_help)),
        )
        .branch(
            dptree::case![State::Start]
                .branch(Update::filter_kb_callback().endpoint(handle_keyboard_callback)),
        )
        .branch(
            dptree::case![State::Inventory(view)]
                .branch(Update::filter_message().endpoint(handle_inventory_message)),
        )
        .branch(Update::filter_callback_query().endpoint(handle_inline_callback))
        .branch(
            Update::filter_message()
                .map(|| "".to_string())
                .endpoint(handle_start),
        );

    let dispatcher = Dispatcher::builder(bot, handler).dependencies(deps![
        InMemStorage::<State>::new(),
        app_state.clone(),
        content_queue
    ]);

    #[cfg(not(debug_assertions))]
    let mut dispatcher = dispatcher.enable_ctrlc_handler().build();

    #[cfg(debug_assertions)]
    let mut dispatcher = dispatcher.build();

    dispatcher.dispatch().await;

    // Flush dirty queued sessions only when running in release profile
    #[cfg(not(debug_assertions))]
    {
        log::info!("Shutting down");
        if let Err(e) = app_state.session_manager.flush().await {
            log::error!("Error flushing session queue during shutdown: {e:?}");
        }
    }

    Ok(())
}
