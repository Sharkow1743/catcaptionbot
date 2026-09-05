pub mod inline;
pub mod inventory;
pub mod kb_command;
pub mod roll;
pub mod take;

pub use inline::handle_inline_callback;
pub use kb_command::KbCommand;

use inventory::{InventoryView, display_inventory, handle_inventory_input};
use roll::serve_cat_roll;
use teloxide::Bot;
use teloxide::types::{KeyboardMarkup, Message};

use crate::content::Content;
use crate::queue::KeyedBufferedQueue;
use crate::reply_kb;
use crate::session::UserSession;
use crate::state::{AppState, DialogueState, HandlerResult};

/// Reply keyboard shown at the top level, outside the inventory view.
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
) -> HandlerResult {
    handle_inventory_input(msg, bot, dialogue, app_state, session, view).await
}
