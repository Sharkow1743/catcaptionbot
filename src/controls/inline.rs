use teloxide::prelude::*;
use teloxide::types::CallbackQuery;

use crate::controls::take::handle_take_cat;
use crate::session::UserSession;
use crate::state::{AppState, HandlerResult};

/// Handles inline-keyboard callbacks. Inventory browsing doesn't use inline
/// keyboards at all (see `controls::inventory`) — this is only for the
/// transient "take the cat" button attached to a freshly rolled photo.
pub async fn handle_inline_callback(
    bot: Bot,
    q: CallbackQuery,
    app_state: AppState,
    mut session: UserSession,
) -> HandlerResult {
    let Some(data) = &q.data else {
        return Ok(());
    };

    let _ = bot.answer_callback_query(q.id.clone()).await;

    let Some(message) = q.message else {
        log::warn!("Received callback query without message context");
        return Ok(());
    };

    let chat_id = message.chat().id;
    let msg_id = message.id();

    if data == "take_cat" {
        handle_take_cat(&mut session, bot, chat_id, msg_id, &app_state.asset_manager).await;
    } else {
        log::warn!("Unhandled callback query data: {data}");
    }

    app_state.session_manager.save_session(&session).await?;
    Ok(())
}
