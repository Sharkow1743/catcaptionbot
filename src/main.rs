mod commands;
mod controls;
mod macros;
mod state;
mod text_generation;

use std::time::Duration;

use commands::{Command, handle_help, handle_start};
use controls::handle_keyboard_callback;
use controls::kb_command::UpdateKbExt;
use state::State;
use text_generation::morph::SqlMorpher;
use text_generation::random::RandomWordDb;

use libsqlite3_sys::sqlite3_auto_extension;
use reqwest::Client;
use sqlite_vec::sqlite3_vec_init;
use teloxide::{
    dispatching::dialogue::{self, InMemStorage},
    dptree::deps,
    prelude::*,
};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    pretty_env_logger::init();
    log::info!("Starting bot");

    unsafe {
        sqlite3_auto_extension(Some(std::mem::transmute(sqlite3_vec_init as *const ())));
    }

    dotenvy::dotenv().ok();

    let client = Client::builder()
        .timeout(Duration::from_secs(30))
        .connect_timeout(Duration::from_secs(15))
        .pool_max_idle_per_host(0) // Disable connection reuse
        .build()
        .expect("Failed to build client");

    let bot = Bot::from_env_with_client(client);

    let morpher = SqlMorpher::new("morphology.db").await?;
    let word_generator = RandomWordDb::new("words.db").await?;

    let handler = dialogue::enter::<Update, InMemStorage<State>, State, _>()
        .branch(
            Update::filter_message()
                .filter_command::<Command>()
                .branch(dptree::case![Command::Start(string)].endpoint(handle_start))
                .branch(dptree::case![Command::Help].endpoint(handle_help)),
        )
        .branch(Update::filter_kb_callback().endpoint(handle_keyboard_callback));

    Dispatcher::builder(bot, handler)
        .dependencies(deps![InMemStorage::<State>::new(), morpher, word_generator])
        .enable_ctrlc_handler()
        .build()
        .dispatch()
        .await;
    Ok(())
}
