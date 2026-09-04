use teloxide::prelude::*;
use teloxide::types::CallbackQuery;

use crate::controls::take::handle_take_cat;
use crate::session::UserSession;
use crate::state::{AppState, HandlerResult};

pub async fn handle_inline_callback(
    bot: Bot,
    q: CallbackQuery,
    app_state: AppState,
    mut session: UserSession,
) -> HandlerResult {
    let Some(data) = &q.data else {
        return Ok(());
    };

    bot.answer_callback_query(q.id).await?;

    let Some(message) = q.message else {
        log::warn!("Received callback query without message context");
        return Ok(());
    };

    let chat_id = message.chat().id;
    let msg_id = message.id();

    match data.as_str() {
        "take_cat" => {
            handle_take_cat(&mut session, bot, chat_id, msg_id, &app_state.asset_manager).await;
        }
        unknown => {
            log::debug!("Unhandled callback query data: {unknown}");
        }
    }

    app_state.session_manager.save_session(&session).await?;
    Ok(())
}
