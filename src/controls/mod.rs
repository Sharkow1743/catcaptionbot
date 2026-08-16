pub mod kb_command;

use std::vec;

use kb_command::KbCommand;
use teloxide::prelude::*;
use teloxide::types::{KeyboardButton, KeyboardMarkup, Message};

use crate::text::WordSpec;

use super::reply_kb;
use super::state::DialogueState;
use super::state::HandlerResult;
use super::state::State;
use super::text::{Morpher, Pos, TextComposer, WordGenerator};

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
    cmd: KbCommand,
    composer: TextComposer,
) -> HandlerResult {
    match cmd {
        KbCommand::Get => {}
    }

    Ok(())
}
