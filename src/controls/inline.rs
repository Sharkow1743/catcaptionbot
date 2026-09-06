use teloxide::prelude::*;
use teloxide::types::CallbackQuery;

use crate::controls::inventory::handle_inventory_callback;
use crate::controls::take::handle_take_cat;
use crate::session::UserSession;
use crate::state::{AppState, DialogueState, HandlerResult};

pub async fn handle_inline_callback(
    bot: Bot,
    q: CallbackQuery,
    app_state: AppState,
    dialogue: DialogueState,
    mut session: UserSession,
) -> HandlerResult {
    let Some(data) = q.data.clone() else {
        return Ok(());
    };

    let Some(message) = q.message.clone() else {
        log::warn!("Received callback query without message context");
        let _ = bot.answer_callback_query(q.id).await;
        return Ok(());
    };

    let chat_id = message.chat().id;
    let msg_id = message.id();

    if data == "take_cat" {
        let _ = bot.answer_callback_query(q.id).await;
        handle_take_cat(
            &mut session,
            bot,
            chat_id,
            msg_id,
            &app_state.asset_manager,
            &app_state.composer.quotes,
        )
        .await;
        app_state.session_manager.save_session(&session).await?;
    } else if data.starts_with("inv:") {
        handle_inventory_callback(bot, q, chat_id, msg_id, &data, app_state, dialogue, session)
            .await?;
    } else {
        let _ = bot.answer_callback_query(q.id).await;
        log::warn!("Unhandled callback query data: {data}");
    }

    Ok(())
}
