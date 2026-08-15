pub mod kb_command;

use kb_command::KbCommand;
use teloxide::prelude::*;
use teloxide::types::{KeyboardButton, KeyboardMarkup, Message};

use crate::text_generation::{Pos, RandomWordDb};

use super::reply_kb;
use super::state::DialogueState;
use super::state::HandlerResult;
use super::state::State;
use super::text_generation::morph::SqlMorpher;

impl State {
    pub fn keyboard(&self) -> Option<KeyboardMarkup> {
        let kb = match self {
            State::Start => reply_kb!([KbCommand::Get.to_string()]),
        }
        .persistent()
        .input_field_placeholder(" ");

        if kb.keyboard.is_empty() {
            None
        } else {
            Some(kb)
        }
    }
}

pub async fn handle_keyboard_callback(
    msg: Message,
    bot: Bot,
    dialogue: DialogueState,
    cmd: KbCommand, // <-- Injected automatically by the filter!
    morpher: SqlMorpher,
    word_generator: RandomWordDb,
) -> HandlerResult {
    match cmd {
        KbCommand::Get => {
            let mut words = word_generator
                .get_random_words_by_pos_and_similarity(
                    &[Pos::Adj],
                    word_generator
                        .get_or_create_embedding("космический")
                        .await?,
                    1,
                    Some(15),
                )
                .await?;

            words.extend(word_generator.get_random_words(1).await?);

            bot.send_message(msg.chat.id, morpher.harmonize(words).await)
                .await?;
        }
    }

    Ok(())
}
