use std::time::Instant;

use teloxide::prelude::*;
use teloxide::types::Message;

use crate::content::Content;
use crate::controls::take::cat_inline_keyboard;
use crate::queue::KeyedBufferedQueue;
use crate::session::UserSession;
use crate::state::{AppState, DialogueState, HandlerResult, State};
use crate::utils::roll_guard::RollGuard;
use crate::utils::unix_timestamp;
use crate::{notice, stats};

pub async fn serve_cat_roll(
    msg: Message,
    bot: Bot,
    content_queue: KeyedBufferedQueue<u64, UserSession, anyhow::Result<Content>>,
    session: UserSession,
    app_state: AppState,
) -> HandlerResult {
    // 1. Try to acquire the roll lock
    let Some(guard) = RollGuard::try_acquire(session.user_id) else {
        // ── REJECTION ACTIONS (Runs in real-time when spammed) ───────────────
        let _ = bot.delete_message(msg.chat.id, msg.id).await;
        notice!("Rejected spam 'Get' from user {}", session.user_id);
        return Ok(());
    };

    // 2. Spawn the background roll task so Teloxide can process incoming updates
    tokio::spawn(async move {
        let _guard = guard; // Keeps the lock until this background task finishes
        if let Err(e) = do_cat_roll(msg, bot, content_queue, session, app_state).await {
            log::error!("Error during cat roll: {e:?}");
        }
    });

    Ok(())
}

async fn do_cat_roll(
    msg: Message,
    bot: Bot,
    content_queue: KeyedBufferedQueue<u64, UserSession, anyhow::Result<Content>>,
    mut session: UserSession,
    app_state: AppState,
) -> anyhow::Result<()> {
    let serve_time = Instant::now();

    // 1. Send placeholder image
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
    let now = unix_timestamp();
    let latest_roll = *session.rolls.get_latest().unwrap_or(&now);
    let between_rolls = now - latest_roll;
    session.between_rolls.push(between_rolls);

    session.fire_trigger("on_roll");

    // 4. Request and generate content
    let content_fetch_time = Instant::now();

    let mut content = content_queue
        .request(session.user_id, session.clone())
        .await??;
    let image = content.image_mut();

    let content_fetch_time = content_fetch_time.elapsed();

    // 5. Update placeholder with generated image & inline actions
    let content_send_time = Instant::now();

    let msg_meta = bot
        .edit_message_media(msg.chat.id, msg_meta.id, image.clone().into())
        .reply_markup(cat_inline_keyboard())
        .await?;

    let now = unix_timestamp();

    stats!(
        "Served content in {:?}\nAcknowledge response: {:?}\nFetch: {:?}\nContent send: {:?}",
        serve_time.elapsed(),
        first_response_time,
        content_fetch_time,
        content_send_time.elapsed()
    );

    // 6. Cache file ID for newly uploaded content
    if let Some(first_photo) = msg_meta.photo().and_then(|photos| photos.first()) {
        let img_id = first_photo.file.id.clone();
        image.set_file_id(img_id);

        session.last_content = Some(content.clone());
    }

    session.rolls.push(now);
    session.last_roll_msg_id = Some(msg_meta.id);

    let rarity = content.rarity().clone();

    // User statistic
    session.stats.total_rolls += 1;
    let roll_tier = rarity.tier();
    if session
        .stats
        .best_tier_rolled
        .map_or(true, |best| roll_tier > best)
    {
        session.stats.best_tier_rolled = Some(roll_tier);
        session.stats.best_rarity_name = Some(rarity.name());
    }

    app_state.session_manager.save_session(&session).await?;

    // 7. Intercept roll for active battle
    let _ = crate::content::battle::on_battle_roll(
        &app_state.session_manager,
        &bot,
        session.user_id,
        rarity.as_ref(),
    )
    .await;

    Ok(())
}
