use std::collections::HashMap;

use teloxide::prelude::*;
use teloxide::types::{
    InputRichBlock, InputRichBlockDetails, InputRichBlockDivider, InputRichBlockSectionHeading,
    InputRichBlockTable, InputRichMessage, RichBlockTableCell, RichText, RichTextBold,
    RichTextItalic,
};

use crate::content::domain::DomainManager;
use crate::content::rarity::RarityManager;
use crate::controls::{battle_active_keyboard, main_menu_keyboard};
use crate::session::UserSession;
use crate::state::{AppState, HandlerResult};

// MARK: - Helpers

fn bold(text: impl Into<RichText>) -> RichText {
    RichTextBold::new(text).into()
}

fn text(val: impl Into<String>) -> RichText {
    RichText::from(val.into())
}

fn table_cell(text: impl Into<RichText>, is_header: bool, align: &str) -> RichBlockTableCell {
    table_cell_colspan(text, is_header, align, 1)
}

fn table_cell_colspan(
    text: impl Into<RichText>,
    is_header: bool,
    align: &str,
    colspan: u32,
) -> RichBlockTableCell {
    RichBlockTableCell {
        text: Some(text.into()),
        is_header: Some(is_header),
        colspan: Some(colspan),
        rowspan: Some(1),
        align: align.to_string(),
        valign: "middle".to_string(),
    }
}

pub fn format_rarity_chance(pct: f32) -> String {
    if pct <= 0.0 {
        "0%".to_string()
    } else if pct >= 1.0 {
        format!("{:.1}%", pct)
    } else {
        format!("{:.2}%", pct)
    }
}

pub fn format_one_in_n(pct: f32) -> String {
    if pct <= 0.0 {
        "—".to_string()
    } else {
        let one_in_n = (100.0 / pct).round() as u64;
        format!("1:{}", one_in_n.max(1))
    }
}

// MARK: - Main Message Builder

pub fn build_rich_stats_message(session: &UserSession) -> InputRichMessage {
    let rarity_mgr = RarityManager::global();
    let domain_mgr = DomainManager::global();

    let mut blocks: Vec<InputRichBlock> = Vec::new();

    // 1. Title
    blocks.push(InputRichBlock::Heading(InputRichBlockSectionHeading {
        text: RichText::from("Статистика"),
        size: 1,
    }));
    blocks.push(InputRichBlock::Divider(InputRichBlockDivider {}));

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
    blocks.push(InputRichBlock::Heading(InputRichBlockSectionHeading {
        text: RichText::from("Коты"),
        size: 2,
    }));

    let activity_table = InputRichBlock::Table(InputRichBlockTable {
        cells: vec![
            vec![
                table_cell(text("Увидено"), false, "left"),
                table_cell(text(session.stats.total_rolls.to_string()), false, "right"),
            ],
            vec![
                table_cell(text("Поймано"), false, "left"),
                table_cell(text(session.stats.total_caught.to_string()), false, "right"),
            ],
            vec![
                table_cell(text("Сбежало"), false, "left"),
                table_cell(
                    text(session.stats.total_escaped.to_string()),
                    false,
                    "right",
                ),
            ],
            vec![
                table_cell(text("Отпущено"), false, "left"),
                table_cell(
                    text(session.stats.total_released.to_string()),
                    false,
                    "right",
                ),
            ],
            vec![
                table_cell(text("Рекордная редкость"), false, "left"),
                table_cell(bold(best_rarity_display), false, "right"),
            ],
        ],
        is_bordered: Some(false),
        is_striped: Some(true),
        is_compact: Some(true),
        caption: None,
    });
    blocks.push(activity_table);

    // 3. PvP Table
    let total_duels =
        session.stats.battles_won + session.stats.battles_lost + session.stats.battles_draw;
    let win_ratio = if total_duels > 0 {
        session.stats.battles_won as f32 / total_duels as f32
    } else {
        0.0
    };

    blocks.push(InputRichBlock::Heading(InputRichBlockSectionHeading {
        text: RichText::from("Дуэли"),
        size: 2,
    }));

    let pvp_table = InputRichBlock::Table(InputRichBlockTable {
        cells: vec![
            vec![
                table_cell(text("Победы"), false, "left"),
                table_cell(text(session.stats.battles_won.to_string()), false, "right"),
            ],
            vec![
                table_cell(text("Поражения"), false, "left"),
                table_cell(text(session.stats.battles_lost.to_string()), false, "right"),
            ],
            vec![
                table_cell(text("Ничьи"), false, "left"),
                table_cell(text(session.stats.battles_draw.to_string()), false, "right"),
            ],
            vec![
                table_cell(text("Всего"), false, "left"),
                table_cell(text(total_duels.to_string()), false, "right"),
            ],
            vec![
                table_cell(text("Частота побед"), false, "left"),
                table_cell(
                    text(format!(
                        "{} {:.1}%",
                        render_progress_bar(win_ratio, 8),
                        win_ratio * 100.0
                    )),
                    false,
                    "right",
                ),
            ],
        ],
        is_bordered: Some(false),
        is_striped: Some(true),
        is_compact: Some(true),
        caption: None,
    });
    blocks.push(pvp_table);

    blocks.push(InputRichBlock::Divider(InputRichBlockDivider {}));

    // 5. Odds Table with Relative Multipliers & 1:N Column
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
                .unwrap_or(r.weight())
                .max(0.0001)
        })
        .sum();

    let mut visible_rarities: Vec<_> = rarity_mgr.all().iter().filter(|r| !r.is_secret()).collect();
    visible_rarities.sort_by(|a, b| b.tier().partial_cmp(&a.tier()).unwrap());

    let mut rarity_rows: Vec<Vec<RichBlockTableCell>> = vec![vec![
        table_cell_colspan(bold("Редкость"), true, "left", 5),
        table_cell(bold("Текущий шанс"), true, "center"),
        table_cell("", true, "right"),
        table_cell(bold("Множитель"), true, "right"),
    ]];

    for r in visible_rarities {
        let base_pct = if total_base_weight > 0.0 {
            (r.weight() / total_base_weight) * 100.0
        } else {
            0.0
        };

        let curr_weight = modified_weights_map
            .get(&r.id())
            .copied()
            .unwrap_or(r.weight())
            .max(0.0001);

        let curr_pct = if total_modified_weight > 0.0 {
            (curr_weight / total_modified_weight) * 100.0
        } else {
            0.0
        };

        // Relative multiplier calculation (e.g. +70% or -50%)
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

        rarity_rows.push(vec![
            table_cell_colspan(text(label), false, "left", 5),
            table_cell(text(format_rarity_chance(curr_pct)), false, "center"),
            table_cell(text(format_one_in_n(curr_pct)), false, "center"),
            table_cell(text(delta_str), false, "right"),
        ]);
    }

    let odds_table = InputRichBlock::Table(InputRichBlockTable {
        cells: rarity_rows,
        is_bordered: Some(false),
        is_striped: Some(true),
        is_compact: Some(true),
        caption: None,
    });

    blocks.push(InputRichBlock::Details(InputRichBlockDetails {
        summary: RichText::from("Таблица редкостей"),
        blocks: vec![odds_table],
        is_open: Some(false),
    }));

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

    let in_battle =
        crate::content::battle::is_in_active_battle(&app_state.session_manager, user_id).await;
    let kb = if in_battle {
        battle_active_keyboard()
    } else {
        main_menu_keyboard()
    };

    bot.send_rich_message(msg.chat.id, rich_message)
        .reply_markup(kb)
        .await?;

    Ok(())
}

fn render_progress_bar(ratio: f32, length: usize) -> String {
    let clamped = ratio.clamp(0.0, 1.0);
    let filled = (clamped * length as f32).round() as usize;
    let empty = length.saturating_sub(filled);
    format!("[{}{}]", "■".repeat(filled), "□".repeat(empty))
}
