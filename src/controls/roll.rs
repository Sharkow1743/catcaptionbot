use std::time::Instant;

use teloxide::prelude::*;
use teloxide::types::Message;

use crate::content::Content;
use crate::controls::take::cat_inline_keyboard;
use crate::queue::KeyedBufferedQueue;
use crate::session::UserSession;
use crate::state::{AppState, HandlerResult};
use crate::utils::unix_timestamp;
use crate::{notice, stats};

pub async fn serve_cat_roll(
    msg: Message,
    bot: Bot,
    content_queue: KeyedBufferedQueue<u64, UserSession, anyhow::Result<Content>>,
    session: UserSession,
    app_state: AppState,
) -> HandlerResult {
    let serve_time = Instant::now();
    let user_id = session.user_id;
    let now = unix_timestamp();

    // 1. Send placeholder image via asset manager
    let placeholder_input = app_state.asset_manager.require("placeholder");
    let msg_meta = bot
        .send_photo(msg.chat.id, placeholder_input.into())
        .await?;
    let first_response_time = serve_time.elapsed();

    // Update cached placeholder Telegram FileId
    if let Some(first_photo) = msg_meta.photo().and_then(|photos| photos.first()) {
        app_state
            .asset_manager
            .update_file_id("placeholder", first_photo.file.id.clone());
    }

    // 2. Remove reply markup from previous roll
    if let Some(prev_msg_id) = session.last_roll_msg_id {
        let _ = bot
            .edit_message_reply_markup(msg.chat.id, prev_msg_id)
            .await;
    }

    // 3. Pre-roll session update
    let session = app_state
        .session_manager
        .mutate_session(user_id, |session| {
            let latest_roll = *session.rolls.get_latest().unwrap_or(&now);
            let between_rolls = now - latest_roll;
            notice!("Time between rolls: {between_rolls}ms");
            session.between_rolls.push(between_rolls);

            session.fire_trigger("on_roll");
            session.rolls.push(now);
            session.last_roll_msg_id = Some(msg_meta.id);

            session.clone()
        })
        .await?;

    // 4. Request and generate content
    let mut content = content_queue
        .request(session.user_id, session.clone())
        .await??;
    let image = content.image_mut();

    // 5. Update placeholder with generated image & inline actions
    let msg_meta = bot
        .edit_message_media(msg.chat.id, msg_meta.id, image.clone().into())
        .reply_markup(cat_inline_keyboard())
        .await?;

    stats!(
        "Served content in {:?}\nAcknowledge response: {:?}",
        serve_time.elapsed(),
        first_response_time
    );

    // 6. Cache file ID for newly uploaded content
    if let Some(first_photo) = msg_meta.photo().and_then(|photos| photos.first()) {
        let img_id = first_photo.file.id.clone();
        image.set_file_id(img_id);

        app_state
            .session_manager
            .mutate_session(user_id, |session| session.last_content = Some(content))
            .await?;
    }

    Ok(())
}
