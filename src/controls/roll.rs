use std::time::{Duration, Instant};
use teloxide::prelude::*;
use teloxide::types::Message;

use crate::content::Content;
use crate::controls::take::cat_inline_keyboard;
use crate::queue::KeyedBufferedQueue;
use crate::session::UserSession;
use crate::state::{AppState, HandlerResult};
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
    let Some(guard) = RollGuard::try_acquire(session.user_id) else {
        let _ = bot.delete_message(msg.chat.id, msg.id).await;
        log::debug!("Rejected spam 'Get' from user {}", session.user_id);
        return Ok(());
    };

    tokio::spawn(async move {
        let _guard = guard;
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

    if let Some(prev_msg_id) = session.last_roll_msg_id {
        let _ = bot
            .edit_message_reply_markup(msg.chat.id, prev_msg_id)
            .await;
    }

    // 1. Send placeholder image
    let placeholder_image_task = async {
        let placeholder_input = app_state.asset_manager.require("placeholder");
        let msg_meta = bot
            .send_photo(msg.chat.id, placeholder_input.into())
            .await?;
        let first_response_time = serve_time.elapsed();

        if let Some(first_photo) = msg_meta.photo().and_then(|photos| photos.first()) {
            app_state
                .asset_manager
                .update_file_id("placeholder", first_photo.file.id.clone());
        }

        Ok((msg_meta, first_response_time))
    };

    // 2. Check for spam mashing strikes while previous roll was in-flight
    let spam_wait_task = async {
        let spam_count = RollGuard::take_spam_count(session.user_id);
        if spam_count > 0 {
            let panic_resistance = session
                .inventory
                .get_ref("equip", 0)
                .map(|s| s.cat.stats.panic_resistance)
                .unwrap_or(0.0);

            let delay_ms =
                ((spam_count as f32 * 400.0 * (1.0 - panic_resistance)).min(2000.0)) as u64;
            let delay = Duration::from_millis(delay_ms);
            tokio::time::sleep(delay).await;
            session.fire_trigger("on_panic_mashing");
            return Ok::<Duration, anyhow::Error>(delay);
        };
        Ok(Duration::ZERO)
    };

    let ((msg_meta, first_response_time), panic_delay_duration) =
        tokio::try_join!(placeholder_image_task, spam_wait_task)?;

    // 3. Pre-roll session update
    let now = unix_timestamp();
    let latest_roll = *session.rolls.get_latest().unwrap_or(&now);
    let between_rolls = now - latest_roll;
    session.between_rolls.push(between_rolls);

    if let Some(equipped) = session.inventory.get_mut("equip", 0) {
        equipped.cat.rolls_count += 1;
    }

    session.fire_trigger("on_roll");

    // 4. Content generation
    let content_fetch_time = Instant::now();
    let mut content = content_queue
        .request(session.user_id, session.clone())
        .await??;
    let image = content.image_mut();
    let content_fetch_time = content_fetch_time.elapsed();

    // 5. Update Telegram media
    let content_send_time = Instant::now();
    let msg_meta = bot
        .edit_message_media(msg.chat.id, msg_meta.id, image.clone().into())
        .reply_markup(cat_inline_keyboard())
        .await?;

    let now = unix_timestamp();

    stats!(
        "Served content in {:?}\nAcknowledge: {:?}\nPanic delay: {:?}\nFetch: {:?}\nSend: {:?}",
        serve_time.elapsed(),
        first_response_time,
        panic_delay_duration,
        content_fetch_time,
        content_send_time.elapsed()
    );

    if let Some(first_photo) = msg_meta.photo().and_then(|photos| photos.first()) {
        let img_id = first_photo.file.id.clone();
        image.set_file_id(img_id);
        session.last_content = Some(content.clone());
    }

    session.rolls.push(now);
    session.last_roll_msg_id = Some(msg_meta.id);

    let rarity = content.rarity().clone();
    session.stats.total_rolls += 1;
    let roll_tier = rarity.tier();
    if session
        .stats
        .best_tier_rolled
        .map_or(true, |b| roll_tier > b)
    {
        session.stats.best_tier_rolled = Some(roll_tier);
        session.stats.best_rarity_name = Some(rarity.name());
    }

    app_state.session_manager.save_session(&session).await?;

    let _ = crate::content::battle::on_battle_roll(
        &app_state.session_manager,
        &bot,
        session.user_id,
        rarity.as_ref(),
    )
    .await;

    Ok(())
}
