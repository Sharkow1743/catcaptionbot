use std::collections::HashMap;

use teloxide::prelude::*;
use teloxide::types::{InputRichBlock, InputRichMessage};

use crate::content::rarity::RarityManager;
use crate::controls::resolve_user_menu_keyboard;
use crate::controls::ui_kit::*;
use crate::session::UserSession;
use crate::state::{AppState, HandlerResult};

pub fn format_rarity_chance(pct: f32) -> String {
    if pct <= 0.0 || !pct.is_finite() {
        "0%".to_string()
    } else if pct >= 1.0 {
        format!("{:.1}%", pct)
    } else if pct >= 0.01 {
        format!("{:.2}%", pct)
    } else if pct >= 0.0005 {
        format!("{:.3}%", pct)
    } else {
        "0.001%".to_string()
    }
}

pub fn format_one_in_n(pct: f32) -> String {
    if pct <= 0.0 || !pct.is_finite() {
        return "—".to_string();
    }

    let n = (100.0 / pct as f64).round().max(1.0);

    if n >= 999_500_000.0 {
        format!("1:{}B", (n / 1_000_000_000.0).round() as u64)
    } else if n >= 999_500.0 {
        format!("1:{}M", (n / 1_000_000.0).round() as u64)
    } else if n >= 999.5 {
        format!("1:{}K", (n / 1_000.0).round() as u64)
    } else {
        format!("1:{}", n as u64)
    }
}

pub fn build_rich_stats_message(session: &UserSession) -> InputRichMessage {
    let rarity_mgr = RarityManager::global();
    let mut blocks: Vec<InputRichBlock> = Vec::new();

    // 1. Title
    blocks.push(heading("Статистика", 1));
    blocks.push(divider());

    let best_rarity_display = if let Some(ref name) = session.stats.best_rarity_name {
        if let Some(r) = rarity_mgr
            .all()
            .iter()
            .find(|r| r.name() == *name || r.id() == *name)
        {
            format!("{} {}", r.domain_icon(), r.name())
        } else {
            name.clone()
        }
    } else {
        "—".to_string()
    };

    // 2. Activity Table
    blocks.push(heading("Коты", 2));
    let activity_table = RichTableBuilder::new()
        .kv_row("Увидено", session.stats.total_rolls.to_string())
        .kv_row("Поймано", session.stats.total_caught.to_string())
        .kv_row("Сбежало", session.stats.total_escaped.to_string())
        .kv_row("Отпущено", session.stats.total_released.to_string())
        .row(vec![
            cell_left("Рекордная редкость"),
            cell_right(bold(best_rarity_display)),
        ])
        .build_block();
    blocks.push(activity_table);

    // 3. PvP Table
    let total_duels =
        session.stats.battles_won + session.stats.battles_lost + session.stats.battles_draw;
    let win_ratio = if total_duels > 0 {
        session.stats.battles_won as f32 / total_duels as f32
    } else {
        0.0
    };

    blocks.push(heading("Дуэли", 2));
    let pvp_table = RichTableBuilder::new()
        .kv_row("Победы", session.stats.battles_won.to_string())
        .kv_row("Поражения", session.stats.battles_lost.to_string())
        .kv_row("Ничьи", session.stats.battles_draw.to_string())
        .kv_row("Всего", total_duels.to_string())
        .row(vec![
            cell_left("Частота побед"),
            cell_right(format!(
                "{} {:.1}%",
                render_progress_bar(win_ratio, 8),
                win_ratio * 100.0
            )),
        ])
        .build_block();
    blocks.push(pvp_table);

    blocks.push(divider());

    // 4. Odds Table
    let total_base_weight = rarity_mgr.total_weight();
    let weights_map: HashMap<String, f32> = rarity_mgr
        .all()
        .iter()
        .map(|r| (r.id(), r.weight()))
        .collect();
    let modified_weights_map = session.fire_hook("rarity_weights", weights_map);

    let total_modified_weight: f32 = rarity_mgr
        .all()
        .iter()
        .map(|r| {
            modified_weights_map
                .get(&r.id())
                .copied()
                .unwrap_or_else(|| r.weight())
                .max(0.0)
        })
        .sum();

    let mut visible_rarities: Vec<_> = rarity_mgr.all().iter().filter(|r| !r.is_secret()).collect();
    visible_rarities.sort_by(|a, b| b.tier().partial_cmp(&a.tier()).unwrap());

    let mut odds_builder = RichTableBuilder::new().row(vec![
        cell_header(bold("Редкость"), "left"),
        cell_header(bold("Текущий"), "right"),
        cell_header(bold("шанс"), "left"),
        cell_header(bold("Множ."), "center"),
    ]);

    for r in visible_rarities {
        let base_pct = if total_base_weight > 0.0 {
            (r.weight() / total_base_weight) * 100.0
        } else {
            0.0
        };

        let curr_weight = modified_weights_map
            .get(&r.id())
            .copied()
            .unwrap_or_else(|| r.weight())
            .max(0.0);

        let curr_pct = if total_modified_weight > 0.0 {
            (curr_weight / total_modified_weight) * 100.0
        } else {
            0.0
        };

        let relative_ratio = if base_pct > 0.0 {
            curr_pct / base_pct
        } else {
            1.0
        };

        let relative_diff_pct = (relative_ratio - 1.0) * 100.0;
        let delta_str = if relative_diff_pct >= 5.0 {
            format!("+{:.0}%    🟩", relative_diff_pct)
        } else if relative_diff_pct <= -5.0 {
            format!("{:.0}%    🟥", relative_diff_pct)
        } else {
            "—".to_string()
        };

        let label = format!("{} {}", r.domain_icon(), r.name());

        odds_builder = odds_builder.row(vec![
            cell_left(label),
            cell_center(format_rarity_chance(curr_pct)),
            cell_center(format_one_in_n(curr_pct)),
            cell_right(delta_str),
        ]);
    }

    blocks.push(details(
        "Таблица редкостей",
        vec![odds_builder.build_block()],
        false,
    ));

    InputRichMessage::blocks(blocks)
}

pub async fn display_stats(
    msg: Message,
    bot: Bot,
    session: UserSession,
    app_state: AppState,
) -> HandlerResult {
    let user_id = session.user_id;
    let rich_message = build_rich_stats_message(&session);
    let (kb, _) = resolve_user_menu_keyboard(&app_state.session_manager, user_id).await;

    bot.send_rich_message(msg.chat.id, rich_message)
        .reply_markup(kb)
        .await?;

    Ok(())
}
