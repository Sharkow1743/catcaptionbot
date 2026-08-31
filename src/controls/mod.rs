pub mod kb_command;

use std::time::Instant;

use kb_command::KbCommand;
use teloxide::prelude::*;
use teloxide::types::KeyboardMarkup;

use crate::content::Content;
use crate::queue::KeyedBufferedQueue;
use crate::session::UserSession;
use crate::state::AppState;
use crate::utils::unix_timestamp;
use crate::{notice, stats};

use super::reply_kb;
use super::state::DialogueState;
use super::state::HandlerResult;
use super::state::State;

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
    _dialogue: DialogueState,
    cmd: KbCommand,
    content_queue: KeyedBufferedQueue<u64, UserSession, anyhow::Result<Content>>,
    session: UserSession,
    app_state: AppState,
) -> HandlerResult {
    match cmd {
        KbCommand::Get => {
            let serve_time = Instant::now();
            let user_id = session.user_id;
            let now = unix_timestamp();

            // Atomically fetch-mutate-save the session so concurrent callback
            // updates for the same user can't race and lose an increment
            // (e.g. adrenaline) to each other.
            let session = app_state
                .session_manager
                .mutate_session(user_id, |session| {
                    let latest_roll = *session.rolls.get_latest().unwrap_or(&now);
                    let between_rolls = now - latest_roll;
                    session.between_rolls.push(between_rolls);

                    session.fire_trigger("on_roll");

                    session.rolls.push(now);

                    session.clone()
                })
                .await?;

            let mut content = content_queue
                .request(session.user_id, session.clone())
                .await??;

            let image = content.image();

            let img_id = if let Some(photo_meta) = bot
                .send_photo(msg.chat.id, image.clone().into())
                .await?
                .photo()
                && let Some(first_photo) = photo_meta.first()
            {
                stats!("Served content in {:?}", serve_time.elapsed());
                Ok(first_photo.file.id.clone())
            } else {
                Err(anyhow::anyhow!("Cannot get photo id"))
            }?;

            image.set_file_id(img_id);
        }
    }

    Ok(())
}
