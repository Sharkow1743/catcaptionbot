use super::state::{DialogueState, HandlerResult};

use super::maybe_kb;
use teloxide::{prelude::*, utils::command::BotCommands};

#[derive(BotCommands, Clone)]
#[command(rename_rule = "snake_case", description = "Available commands:")]
pub enum Command {
    #[command(aliases = ["h"], description = "Display this message")]
    Help,
    #[command(description = "Start the bot")]
    Start(String),
}

pub async fn handle_start(
    bot: Bot,
    msg: Message,
    dialogue: DialogueState,
    _start: String,
) -> HandlerResult {
    dialogue.reset().await?;
    let kb = dialogue.get_or_default().await?.keyboard();
    let welcome = "[START MESSAGE]";
    maybe_kb!(bot.send_message(msg.chat.id, welcome), kb).await?;
    Ok(())
}

pub async fn handle_help(bot: Bot, msg: Message) -> HandlerResult {
    bot.send_message(msg.chat.id, Command::descriptions().to_string())
        .await?;
    Ok(())
}
