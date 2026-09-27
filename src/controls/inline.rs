use std::str::FromStr;

use teloxide::prelude::*;
use teloxide::types::{
    CallbackQuery, InlineKeyboardButton, InlineKeyboardMarkup, InlineQuery, InlineQueryResult,
    InlineQueryResultArticle, InputMessageContent, InputMessageContentText, ParseMode,
};
use teloxide::utils::html::escape;

use crate::controls::inventory::handle_inventory_callback;
use crate::controls::take::handle_take_cat;
use crate::session::UserSession;
use crate::state::{AppState, DialogueState, HandlerResult};
use crate::utils::short_id::ShortId;

pub async fn handle_inline_query(bot: Bot, q: InlineQuery, app_state: AppState) -> HandlerResult {
    let user_id = q.from.id.0;
    let query = q.query.trim().to_lowercase();
    let session = app_state
        .session_manager
        .get_session(user_id)
        .await
        .unwrap_or_else(|_| UserSession::new(user_id));

    let raw_username = q.from.username.as_deref().unwrap_or("Игрок");
    let safe_username = escape(raw_username);

    let mut results: Vec<InlineQueryResult> = Vec::new();

    // ── 1. DUEL CHALLENGE ──
    if query.is_empty()
        || query.contains("battle")
        || query.contains("duel")
        || query.contains("дуэль")
        || query.contains("бой")
    {
        let bot_username = bot
            .get_me()
            .await
            .ok()
            .and_then(|me| me.user.username)
            .unwrap_or_default();

        let duel_id = if let Some((existing_id, _)) =
            crate::content::battle::find_waiting_duel(&app_state.session_manager, user_id).await
        {
            existing_id
        } else {
            let battle_id = ShortId::now_v7();
            let _ = crate::content::battle::create_invite_duel_with_id(
                &app_state.session_manager,
                user_id,
                user_id as i64,
                q.from.username.clone(),
                battle_id,
            )
            .await;
            battle_id
        };

        let content = InputMessageContentText::new(format!(
            "<b>Вызов на дуэль</b>\n\n<b>Организатор:</b> @{safe_username}"
        ))
        .parse_mode(ParseMode::Html);

        let invite_url_str = format!("https://t.me/{}?start=battle_{}", bot_username, duel_id);
        let button = if let Ok(url) = reqwest::Url::parse(&invite_url_str) {
            InlineKeyboardButton::url("Принять вызов", url)
        } else {
            InlineKeyboardButton::callback(
                "Принять вызов",
                format!("inline_duel:{duel_id}:{user_id}"),
            )
        };

        let kb = InlineKeyboardMarkup::new([[button]]);

        let article = InlineQueryResultArticle::new(
            format!("duel_{duel_id}"),
            "Позвать на дуэль",
            InputMessageContent::Text(content),
        )
        .description("Отправить приглашение на дуэль в этот чат")
        .reply_markup(kb);

        results.push(InlineQueryResult::Article(article));
    }

    // ── 2. PLAYER STATISTICS ──
    if query.is_empty()
        || query.contains("stat")
        || query.contains("стат")
        || query.contains("профиль")
        || query.contains("profile")
    {
        let total_duels =
            session.stats.battles_won + session.stats.battles_lost + session.stats.battles_draw;
        let win_ratio = if total_duels > 0 {
            (session.stats.battles_won as f32 / total_duels as f32) * 100.0
        } else {
            0.0
        };

        let best_rarity = session.stats.best_rarity_name.as_deref().unwrap_or("—");

        let stats_text = format!(
            "<b>Статистика @{}</b>\n\n<b>Коты:</b>\n• Поймано: <b>{}</b>\n• Отпущено: <b>{}</b>\n• Увидено: <b>{}</b>\n• Рекордная редкость: <b>{}</b>\n\n<b>Дуэли:</b>\n• Побед: <b>{}</b>\n• Поражений: <b>{}</b>\n• Ничьих: <b>{}</b>\n• Всего: {}\n• Процент побед: <b>{:.1}%</b>",
            safe_username,
            session.stats.total_caught,
            session.stats.total_released,
            session.stats.total_rolls,
            escape(best_rarity),
            session.stats.battles_won,
            session.stats.battles_lost,
            session.stats.battles_draw,
            total_duels,
            win_ratio
        );

        let content = InputMessageContentText::new(stats_text).parse_mode(ParseMode::Html);
        let article = InlineQueryResultArticle::new(
            format!("stats_{user_id}"),
            "Поделиться статистикой",
            InputMessageContent::Text(content),
        )
        .description("Отправить общую статистику и сражений");

        results.push(InlineQueryResult::Article(article));
    }

    if let Err(e) = bot
        .answer_inline_query(q.id, results)
        .is_personal(true)
        .cache_time(0)
        .await
    {
        log::error!("Failed to answer inline query: {e:?}");
    }

    Ok(())
}

pub async fn handle_inline_duel_callback(
    bot: Bot,
    q: CallbackQuery,
    app_state: AppState,
) -> HandlerResult {
    let Some(ref data) = q.data else {
        return Ok(());
    };

    let Some(duel_data) = data.strip_prefix("inline_duel:") else {
        return Ok(());
    };

    let parts: Vec<&str> = duel_data.split(':').collect();
    if parts.len() >= 2 {
        let battle_id = ShortId::from_str(parts[0]).unwrap_or_else(|_| ShortId::now_v7());
        let host_id: u64 = parts[1].parse().unwrap_or(0);
        let challenger_id = q.from.id.0;
        let challenger_username = q.from.username.clone();

        if challenger_id == host_id {
            let _ = bot
                .answer_callback_query(q.id)
                .text("Вы не можете принять собственный вызов")
                .show_alert(true)
                .await;
            return Ok(());
        }

        let _ = bot.answer_callback_query(q.id).text("Дуэль принята!").await;

        if app_state
            .session_manager
            .get_ephemeral::<crate::content::battle::BattleSession>(battle_id)
            .await
            .is_none()
        {
            let _ = crate::content::battle::create_invite_duel_with_id(
                &app_state.session_manager,
                host_id,
                host_id as i64,
                None,
                battle_id,
            )
            .await;
        }

        let _ = crate::content::battle::join_invite_duel(
            &app_state.session_manager,
            &bot,
            challenger_id,
            challenger_id as i64,
            challenger_username,
            battle_id,
        )
        .await;
    }

    Ok(())
}

pub async fn handle_inline_callback(
    bot: Bot,
    q: CallbackQuery,
    app_state: AppState,
    dialogue: DialogueState,
    mut session: UserSession,
) -> HandlerResult {
    let Some(data) = q.data.clone() else {
        return Ok(());
    };

    let Some(message) = q.message.clone() else {
        let _ = bot.answer_callback_query(q.id).await;
        return Ok(());
    };

    let chat_id = message.chat().id;
    let msg_id = message.id();

    if data == "take_cat" {
        let _ = bot.answer_callback_query(q.id).await;
        handle_take_cat(&mut session, bot, chat_id, msg_id, &app_state).await;
        app_state.session_manager.save_session(&session).await?;
    } else if data.starts_with("inv:") {
        handle_inventory_callback(bot, q, chat_id, msg_id, &data, app_state, dialogue, session)
            .await?;
    } else {
        let _ = bot.answer_callback_query(q.id).await;
    }

    Ok(())
}
