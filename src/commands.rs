use std::str::FromStr;

use super::state::{DialogueState, HandlerResult};

use crate::controls::main_menu_keyboard;
use crate::utils::short_id::ShortId;
use teloxide::{prelude::*, utils::command::BotCommands};

#[derive(BotCommands, Clone)]
#[command(rename_rule = "snake_case", description = "Доступные команды:")]
pub enum Command {
    #[command(aliases = ["h"], description = "Показать это сообщение")]
    Help,
    #[command(description = "Запустить бота")]
    Start(String),
}

pub async fn handle_start(
    bot: Bot,
    msg: Message,
    dialogue: DialogueState,
    start: String,
    app_state: crate::state::AppState,
) -> HandlerResult {
    dialogue.reset().await?;
    let start_arg = start.trim();

    if let Some(battle_id_str) = start_arg.strip_prefix("battle_") {
        if let Ok(battle_id) = ShortId::from_str(battle_id_str) {
            let user_id = msg.from.as_ref().map(|u| u.id.0).unwrap_or(0);
            let username = msg.from.as_ref().and_then(|u| u.username.clone());
            match crate::content::battle::join_invite_duel(
                &app_state.session_manager,
                &bot,
                user_id,
                msg.chat.id.0,
                username,
                battle_id,
            )
            .await
            {
                Ok(_) => return Ok(()),
                Err(e) => {
                    bot.send_message(
                        msg.chat.id,
                        format!("Не удалось присоединиться к дуэли: {e}"),
                    )
                    .reply_markup(main_menu_keyboard())
                    .await?;
                    return Ok(());
                }
            }
        }
    }

    let welcome = "<b>Добро пожаловать!</b>\n\nНажмите кнопку \"Получить кота\" с говорящим названием, чтобы начать играть";
    bot.send_message(msg.chat.id, welcome)
        .parse_mode(teloxide::types::ParseMode::Html)
        .reply_markup(main_menu_keyboard())
        .await?;
    Ok(())
}

pub async fn handle_help(bot: Bot, msg: Message) -> HandlerResult {
    bot.send_message(msg.chat.id, Command::descriptions().to_string())
        .await?;
    Ok(())
}
