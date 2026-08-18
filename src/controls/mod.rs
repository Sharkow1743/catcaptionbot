pub mod kb_command;

use std::vec;

use flume::Receiver;
use kb_command::KbCommand;
use reqwest::Client;
use teloxide::prelude::*;
use teloxide::types::{KeyboardButton, KeyboardMarkup, Message};

use crate::AppState;
use crate::content::Content;
use crate::content::rarity::RarityManager;
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
    content_queue: Receiver<Content>,
) -> HandlerResult {
    match cmd {
        KbCommand::Get => {
            let mut content = content_queue.recv_async().await?;

            let image = content.image();

            let img_id = if let Some(photo_meta) = bot
                .send_photo(msg.chat.id, image.clone().into())
                .await?
                .photo()
                && let Some(photo_meta) = photo_meta.get(0)
            {
                Ok(photo_meta.clone().file.id)
            } else {
                Err(anyhow::anyhow!("Cannot get photo id"))
            }?;

            image.set_file_id(img_id);
        }
    }

    Ok(())
}
