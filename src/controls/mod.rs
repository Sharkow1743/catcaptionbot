pub mod inline;
pub mod inventory;
pub mod kb_command;
pub mod roll;
pub mod take;

pub use inline::handle_inline_callback;
pub use kb_command::KbCommand;

use inventory::{InventoryView, display_inventory};
use roll::serve_cat_roll;
use teloxide::Bot;
use teloxide::payloads::SendMessageSetters;
use teloxide::prelude::Requester;
use teloxide::types::{KeyboardMarkup, Message};

use crate::content::Content;
use crate::queue::KeyedBufferedQueue;
use crate::reply_kb;
use crate::session::UserSession;
use crate::state::{AppState, DialogueState, HandlerResult, State};

/// Reply keyboard shown at the top level.
pub fn main_menu_keyboard() -> KeyboardMarkup {
    reply_kb!(
        [KbCommand::Get.to_string()],
        [KbCommand::Inventory.to_string()]
    )
    .persistent()
    .input_field_placeholder(" ")
}

pub async fn handle_keyboard_callback(
    msg: Message,
    bot: Bot,
    cmd: KbCommand,
    content_queue: KeyedBufferedQueue<u64, UserSession, anyhow::Result<Content>>,
    dialogue: DialogueState,
    session: UserSession,
    app_state: AppState,
) -> HandlerResult {
    match cmd {
        KbCommand::Get => {
            serve_cat_roll(msg, bot, content_queue, session, app_state).await?;
        }
        KbCommand::Inventory => {
            display_inventory(msg, bot, dialogue, app_state, session).await?;
        }
    }
    Ok(())
}

pub async fn handle_inventory_message(
    msg: Message,
    bot: Bot,
    dialogue: DialogueState,
    app_state: AppState,
    session: UserSession,
    view: InventoryView,
    content_queue: KeyedBufferedQueue<u64, UserSession, anyhow::Result<Content>>,
) -> HandlerResult {
    let text = msg.text().unwrap_or_default();

    if let Some(cmd) = KbCommand::from_display_text(text) {
        match cmd {
            KbCommand::Inventory => {
                // User clicked "Котейнеры" while already in inventory: no-op & delete message
                let _ = bot.delete_message(msg.chat.id, msg.id).await;
                return Ok(());
            }
            KbCommand::Get => {
                // User clicked "Получить": delete control board & start roll immediately
                let _ = bot.delete_message(msg.chat.id, view.ctrl_msg_id).await;
                dialogue.update(State::Start).await?;
                serve_cat_roll(msg, bot, content_queue, session, app_state).await?;
                return Ok(());
            }
        }
    }

    // User sent any arbitrary text/other message: delete control board, restore Reply KB, and exit
    let _ = bot.delete_message(msg.chat.id, view.ctrl_msg_id).await;
    let _ = bot.delete_message(msg.chat.id, msg.id).await;
    let _ = bot
        .send_message(msg.chat.id, "Главное меню")
        .reply_markup(main_menu_keyboard())
        .await;
    dialogue.update(State::Start).await?;
    Ok(())
}
