use teloxide::types::{
    InputRichBlock, InputRichBlockDivider, InputRichMessage, RichMessageButton, RichText,
};

use crate::content::domain::DomainManager;
use crate::controls::inventory::types::{InventorySlot, InventoryTab};
use crate::controls::ui_kit::*;
use crate::session::UserSession;

const GRID_COLUMNS: usize = 2;

pub fn build_rich_inventory_message(
    session: &UserSession,
    selected: InventorySlot,
    tab: InventoryTab,
    pending_release: Option<InventorySlot>,
) -> InputRichMessage {
    let mut blocks: Vec<InputRichBlock> = Vec::new();

    // ── SUB-MENU: STATS, TRAITS & REROLL ──
    if tab == InventoryTab::Stats {
        let (cat_type, idx) = selected.target();
        if let Some(slot) = session.inventory.get(cat_type, idx) {
            let domain = slot.cat.domain();
            let cost = slot.cat.reroll_shard_cost();

            blocks.push(heading("Характеристики", 2));
            blocks.push(paragraph(bold(slot.cat.image_text())));

            // 1. Core 2x2 Stats Grid
            let stats_table = RichTableBuilder::new()
                .row(vec![
                    cell_left("Резонанс"),
                    cell_right(bold(format!("{:+.2}", slot.cat.stats.reach_bonus))),
                    cell_left("Выносливость"),
                    cell_right(bold(format!("{:.0}%", slot.cat.stats.stamina_mult * 100.0))),
                ])
                .row(vec![
                    cell_left("Хватка"),
                    cell_right(bold(format!("{:+.0}%", slot.cat.stats.catch_bonus * 100.0))),
                    cell_left("Хладнокровие"),
                    cell_right(bold(format!(
                        "{:.0}%",
                        slot.cat.stats.panic_resistance * 100.0
                    ))),
                ])
                .build_block();
            blocks.push(stats_table);

            // 2. Collapsible Stat Descriptions Guide
            let help_table = RichTableBuilder::new()
                .row(vec![
                    cell_left(bold("Резонанс")),
                    cell_left("Повышает максимальный ранг котов"),
                ])
                .row(vec![
                    cell_left(bold("Выносливость")),
                    cell_left("Снижает утомляемость при поиске и ускоряет отдых"),
                ])
                .row(vec![
                    cell_left(bold("Хватка")),
                    cell_left("Повышает шанс забрать кота и снижает вероятность побега"),
                ])
                .row(vec![
                    cell_left(bold("Хладнокровие")),
                    cell_left("Снижает задержки и защищает от штрафов при частом поиске"),
                ])
                .build_block();

            blocks.push(details(
                "Справка по характеристикам",
                vec![help_table],
                false,
            ));

            // 3. Traits Section
            if !slot.cat.traits.is_empty() {
                let mut trait_builder = RichTableBuilder::new().caption("Особенности");
                for t in &slot.cat.traits {
                    trait_builder = trait_builder.row(vec![
                        cell_left(bold(t.name.clone())),
                        cell_left(t.desc.clone()),
                    ]);
                }
                blocks.push(trait_builder.build_block());
            }

            // 4. All-Domain Shard Wallet Table
            let domain_mgr = DomainManager::global();
            let mut shard_builder = RichTableBuilder::new();
            for chunk in domain_mgr.all().chunks(2) {
                let mut row = Vec::new();
                for dom in chunk {
                    let count = session.shards.get(dom.id()).copied().unwrap_or(0);
                    row.push(cell_left(format!("{} {}", dom.icon(), dom.name())));
                    row.push(cell_right(bold(count.to_string())));
                }
                shard_builder = shard_builder.row(row);
            }
            blocks.push(shard_builder.build_block());

            blocks.push(divider());

            // Sub-menu Actions
            blocks.push(buttons(
                vec![
                    RichMessageButton::callback(
                        RichText::from(format!("Перековать ({cost}{})", domain.icon())),
                        "inv:act:reroll",
                    ),
                    RichMessageButton::callback(RichText::from("‹ Назад"), "inv:tab:overview"),
                ],
                "center",
            ));
        }

        return InputRichMessage::blocks(blocks);
    }

    // ── MAIN OVERVIEW VIEW ──
    blocks.push(heading("Ваши коты", 2));

    if let Some(equip_button) = build_equip_button(session, selected) {
        blocks.push(buttons(vec![equip_button], "center"));
    }

    let bag_buttons = build_bag_buttons(session, selected);
    if !bag_buttons.is_empty() {
        blocks.push(divider());
        for chunk in bag_buttons.chunks(GRID_COLUMNS) {
            blocks.push(buttons(chunk.to_vec(), "center"));
        }
    }

    blocks.push(divider());

    if pending_release == Some(selected) {
        blocks.push(paragraph(vec![
            RichText::from("Точно отпустить этого кота?\nЭто действие "),
            bold("необратимо"),
        ]));
    } else {
        let (cat_type, idx) = selected.target();
        match session.inventory.get(cat_type, idx) {
            Some(slot) => {
                let (_, bond_display) = slot.cat.loyalty_status();
                blocks.push(paragraph(vec![
                    bold(slot.cat.desc()),
                    RichText::from(format!("\n{bond_display}")),
                ]));
            }
            None => {
                let empty_text = if cat_type == "equip" {
                    "С собой никого нет"
                } else {
                    "Коробка пуста"
                };
                blocks.push(paragraph(italic(empty_text)));
            }
        }
    }

    if let Some(action_row) = build_action_row(selected, pending_release) {
        for chunk in action_row.chunks(2) {
            blocks.push(buttons(chunk.to_vec(), "center"));
        }
    }

    InputRichMessage::blocks(blocks)
}

fn build_equip_button(session: &UserSession, selected: InventorySlot) -> Option<RichMessageButton> {
    let slot = session.inventory.get("equip", 0)?;
    let icon = slot.cat.rarity().domain_icon();
    let name = format!("{} {}", icon, slot.cat.image_text());
    let is_selected = selected == InventorySlot::Equip;

    let mut button = RichMessageButton::callback(RichText::from(name), "inv:sel:equip");
    if is_selected {
        button = button.style("primary");
    }
    Some(button)
}

fn build_bag_buttons(session: &UserSession, selected: InventorySlot) -> Vec<RichMessageButton> {
    let mut buttons: Vec<RichMessageButton> = Vec::new();

    let Some(bag) = session.inventory.category("inventory") else {
        return buttons;
    };

    for (idx, slot) in bag.indexed_items() {
        let icon = slot.cat.rarity().domain_icon();
        let name = format!("{} {}", icon, slot.cat.image_text());
        let is_selected = selected == InventorySlot::Bag(idx);

        let mut button =
            RichMessageButton::callback(RichText::from(name), format!("inv:sel:bag:{idx}"));
        if is_selected {
            button = button.style("primary");
        }
        buttons.push(button);
    }

    buttons
}

fn build_action_row(
    selected: InventorySlot,
    pending_release: Option<InventorySlot>,
) -> Option<Vec<RichMessageButton>> {
    if pending_release == Some(selected) {
        return Some(vec![
            RichMessageButton::callback(RichText::from("Отпустить"), "inv:confirm_release")
                .style("danger"),
            RichMessageButton::callback(RichText::from("Отмена"), "inv:cancel_release"),
        ]);
    }

    let stats_btn = RichMessageButton::callback(RichText::from("Характеристики"), "inv:tab:stats");

    let mut buttons = Vec::new();
    buttons.push(stats_btn);

    match selected {
        InventorySlot::Equip => {
            buttons.push(RichMessageButton::callback(
                RichText::from("Выложить"),
                "inv:act:unequip",
            ));
            buttons.push(
                RichMessageButton::callback(RichText::from("Отпустить"), "inv:act:release:equip")
                    .style("danger"),
            );
        }
        InventorySlot::Bag(b_idx) => {
            buttons.push(RichMessageButton::callback(
                RichText::from("Взять"),
                format!("inv:act:equip:{b_idx}"),
            ));
            buttons.push(
                RichMessageButton::callback(
                    RichText::from("Отпустить"),
                    format!("inv:act:release:bag:{b_idx}"),
                )
                .style("danger"),
            );
        }
    }

    Some(buttons)
}

pub async fn render_controls_in_place(
    bot: &teloxide::Bot,
    chat_id: teloxide::types::ChatId,
    ctrl_msg_id: teloxide::types::MessageId,
    session: &UserSession,
    selected: InventorySlot,
    tab: InventoryTab,
    pending_release: Option<InventorySlot>,
) -> anyhow::Result<()> {
    let rich_message = build_rich_inventory_message(session, selected, tab, pending_release);

    let res = bot
        .edit_message_rich_text(chat_id, ctrl_msg_id, rich_message)
        .await;

    if let Err(err) = res {
        let err_msg = err.to_string();
        if !err_msg.contains("message is not modified") {
            log::warn!("Failed to edit rich message control board: {err_msg}");
        }
    }

    Ok(())
}
