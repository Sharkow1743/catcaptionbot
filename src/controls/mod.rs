pub mod kb_command;

use std::time::Instant;

use bonsaidb::core::connection::Session;
use kb_command::KbCommand;
use teloxide::dispatching::dialogue::GetChatId;
use teloxide::prelude::*;
use teloxide::types::{
    FileId, InlineKeyboardButton, InlineKeyboardMarkup, InputFile, InputMedia, InputMediaPhoto,
    KeyboardMarkup, MessageId,
};

use crate::content::inventory::Slot;
use crate::content::{CachedImage, Content};
use crate::queue::KeyedBufferedQueue;
use crate::session::UserSession;
use crate::state::AppState;
use crate::utils::unix_timestamp;
use crate::{notice, stats};

use super::reply_kb;
use super::state::DialogueState;
use super::state::HandlerResult;
use super::state::State;

const PLACEHOLDER_IMAGE: &[u8] = include_bytes!("../../placeholder.jpg");

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

            // placeholder image getting
            let cached_placeholder_id = {
                let read_guard = app_state.placeholder_file_id.read().await;
                read_guard.clone()
            };

            let placeholder_input = match cached_placeholder_id {
                Some(file_id) => CachedImage::from_file_id(file_id),
                None => CachedImage::from_bytes(PLACEHOLDER_IMAGE),
            };

            // placeholder sending
            let msg_meta = bot
                .send_photo(msg.chat.id, placeholder_input.into())
                .await?;
            let first_response_time = serve_time.elapsed();

            if let Some(id) = session.last_roll_msg_id {
                let _ = bot.edit_message_reply_markup(msg.chat.id, id).await;
            }

            // pre roll session update
            let session = app_state
                .session_manager
                .mutate_session(user_id, |session| {
                    let latest_roll = *session.rolls.get_latest().unwrap_or(&now);
                    let between_rolls = now - latest_roll;
                    notice!("{between_rolls}");
                    session.between_rolls.push(between_rolls);

                    session.fire_trigger("on_roll");

                    session.rolls.push(now);

                    session.last_roll_msg_id = Some(msg_meta.id);

                    session.clone()
                })
                .await?;

            // content getting/generation
            let mut content = content_queue
                .request(session.user_id, session.clone())
                .await??;
            let image = content.image();

            // image senging
            let msg_meta = bot
                .edit_message_media(msg.chat.id, msg_meta.id, image.clone().into())
                .reply_markup(cat_inline_keyboard())
                .await?;

            stats!(
                "Served content in {:?}\nAcknowledge response: {:?}",
                serve_time.elapsed(),
                first_response_time
            );

            if let Some(first_photo) = msg_meta.photo().and_then(|photos| photos.first()) {
                let img_id = first_photo.file.id.clone();

                image.set_file_id(img_id);

                app_state
                    .session_manager
                    .mutate_session(user_id, |session| session.last_content = Some(content))
                    .await?;
            };
        }
    }

    Ok(())
}

fn cat_inline_keyboard() -> InlineKeyboardMarkup {
    InlineKeyboardMarkup::new([[InlineKeyboardButton::callback("Забрать", "take_cat")]])
}

pub async fn handle_inline_callback(
    bot: Bot,
    q: CallbackQuery,
    app_state: AppState,
    mut session: UserSession,
) -> HandlerResult {
    if let Some(data) = &q.data {
        bot.answer_callback_query(q.id.clone()).await?;

        // 1. Extract the regular message reference (ChatId + MessageId)
        let Some(message) = q.message else {
            log::warn!(
                "Received callback query without message context: {:?}",
                q.id
            );
            return Ok(());
        };

        let chat_id = message.chat().id;
        let msg_id = message.id();

        let cached_placeholder_id = {
            let read_guard = app_state.placeholder_file_id.read().await;
            read_guard.clone()
        };

        let placeholder_input = match cached_placeholder_id {
            Some(file_id) => CachedImage::from_file_id(file_id),
            None => CachedImage::from_bytes(PLACEHOLDER_IMAGE),
        };

        match data.as_str() {
            "take_cat" => {
                take_cat(&mut session, bot, chat_id, msg_id, placeholder_input).await;
            }
            _ => {}
        }

        app_state.session_manager.save_session(&session).await?;
    }

    Ok(())
}

async fn take_cat(
    session: &mut UserSession,
    bot: Bot,
    chat_id: ChatId,
    msg_id: MessageId,
    placeholder_image: CachedImage,
) {
    let mut caption = "";
    if let Some(content) = session.last_content.clone() {
        let slot = Slot { cat: content };
        if let Err(slot) = session.inventory.push("equip", slot) {
            if let Ok(_) = session.inventory.push("inventory", slot) {
                // Fixed: Non-inline edit caption method
                let _ = bot
                    .edit_message_caption(chat_id, msg_id)
                    .caption("Этот кот теперь ваш")
                    .reply_markup(InlineKeyboardMarkup::default())
                    .await;
                return;
            } else {
                caption = "Ему не нашлось места";
            }
        }
    };

    cat_escape(bot, chat_id, msg_id, placeholder_image, caption).await;
}

async fn cat_escape(
    bot: Bot,
    chat_id: ChatId,
    msg_id: MessageId,
    placeholder_image: CachedImage,
    caption: &str,
) {
    let photo =
        InputMediaPhoto::new(placeholder_image.into()).caption(format!("Кот сбежал! {}", caption));

    // Fixed: Non-inline edit media method
    let _ = bot
        .edit_message_media(chat_id, msg_id, InputMedia::Photo(photo))
        .reply_markup(InlineKeyboardMarkup::default())
        .await;
}
