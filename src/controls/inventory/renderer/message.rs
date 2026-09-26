use teloxide::types::{
    InputRichBlock, InputRichBlockButtons, InputRichBlockDetails, InputRichBlockDivider,
    InputRichBlockParagraph, InputRichBlockSectionHeading, InputRichBlockTable, InputRichMessage,
    MessageId, RichBlockTableCell, RichMessageButton, RichText, RichTextBold, RichTextItalic,
};

use crate::content::domain::DomainManager;
use crate::controls::inventory::types::{InventorySlot, InventoryTab};
use crate::session::UserSession;

const GRID_COLUMNS: usize = 2;

fn bold(text: impl Into<RichText>) -> RichText {
    RichTextBold::new(text).into()
}

fn text(val: impl Into<String>) -> RichText {
    RichText::from(val.into())
}

fn table_cell(text: impl Into<RichText>, is_header: bool, align: &str) -> RichBlockTableCell {
    RichBlockTableCell {
        text: Some(text.into()),
        is_header: Some(is_header),
        colspan: Some(1),
        rowspan: Some(1),
        align: align.to_string(),
        valign: "middle".to_string(),
    }
}

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

            blocks.push(InputRichBlock::Heading(InputRichBlockSectionHeading {
                text: RichText::from("Характеристики"),
                size: 2,
            }));

            blocks.push(InputRichBlock::Paragraph(InputRichBlockParagraph {
                text: bold(slot.cat.image_text()),
            }));

            // 1. Core 2x2 Stats Grid
            let stats_table = InputRichBlock::Table(InputRichBlockTable {
                cells: vec![
                    vec![
                        table_cell(text("Резонанс"), false, "left"),
                        table_cell(
                            bold(format!("{:+.2}", slot.cat.stats.reach_bonus)),
                            false,
                            "right",
                        ),
                        table_cell(text("Выносливость"), false, "left"),
                        table_cell(
                            bold(format!("{:.0}%", slot.cat.stats.stamina_mult * 100.0)),
                            false,
                            "right",
                        ),
                    ],
                    vec![
                        table_cell(text("Хватка"), false, "left"),
                        table_cell(
                            bold(format!("{:+.0}%", slot.cat.stats.catch_bonus * 100.0)),
                            false,
                            "right",
                        ),
                        table_cell(text("Хладнокровие"), false, "left"),
                        table_cell(
                            bold(format!("{:.0}%", slot.cat.stats.panic_resistance * 100.0)),
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
            blocks.push(stats_table);

            // 2. Collapsible Stat Descriptions Guide
            let help_table = InputRichBlock::Table(InputRichBlockTable {
                cells: vec![
                    vec![
                        table_cell(bold("Резонанс"), false, "left"),
                        table_cell(text("Повышает максимальный ранг котов"), false, "left"),
                    ],
                    vec![
                        table_cell(bold("Выносливость"), false, "left"),
                        table_cell(
                            text("Снижает утомляемость при поиске и ускоряет отдых"),
                            false,
                            "left",
                        ),
                    ],
                    vec![
                        table_cell(bold("Хватка"), false, "left"),
                        table_cell(
                            text("Повышает шанс забрать кота и снижает вероятность побега"),
                            false,
                            "left",
                        ),
                    ],
                    vec![
                        table_cell(bold("Хладнокровие"), false, "left"),
                        table_cell(
                            text("Снижает задержки и защищает от штрафов при частом поиске"),
                            false,
                            "left",
                        ),
                    ],
                ],
                is_bordered: Some(false),
                is_striped: Some(true),
                is_compact: Some(true),
                caption: None,
            });

            blocks.push(InputRichBlock::Details(InputRichBlockDetails {
                summary: RichText::from("Справка по характеристикам"),
                blocks: vec![help_table],
                is_open: Some(false),
            }));

            // 3. Traits Section
            if !slot.cat.traits.is_empty() {
                let mut trait_cells = Vec::new();
                for t in &slot.cat.traits {
                    trait_cells.push(vec![
                        table_cell(bold(text(&t.name)), false, "left"),
                        table_cell(text(&t.desc), false, "left"),
                    ]);
                }

                blocks.push(InputRichBlock::Table(InputRichBlockTable {
                    cells: trait_cells,
                    is_bordered: Some(false),
                    is_striped: Some(true),
                    is_compact: Some(true),
                    caption: Some(RichText::from("Особенности")),
                }));
            }

            // 4. All-Domain Shard Wallet Table
            let domain_mgr = DomainManager::global();
            let mut shard_cells: Vec<Vec<RichBlockTableCell>> = Vec::new();
            let all_domains = domain_mgr.all();

            for chunk in all_domains.chunks(2) {
                let mut row = Vec::new();
                for dom in chunk {
                    let count = session.shards.get(dom.id()).copied().unwrap_or(0);
                    row.push(table_cell(
                        text(format!("{} {}", dom.icon(), dom.name())),
                        false,
                        "left",
                    ));
                    row.push(table_cell(bold(count.to_string()), false, "right"));
                }
                shard_cells.push(row);
            }

            let shards_table = InputRichBlock::Table(InputRichBlockTable {
                cells: shard_cells,
                is_bordered: Some(false),
                is_striped: Some(true),
                is_compact: Some(true),
                caption: None,
            });
            blocks.push(shards_table);

            blocks.push(InputRichBlock::Divider(InputRichBlockDivider {}));

            // Sub-menu Actions
            blocks.push(InputRichBlock::Buttons(
                InputRichBlockButtons::new(vec![
                    RichMessageButton::callback(
                        RichText::from(format!("Перековать ({cost}{})", domain.icon())),
                        "inv:act:reroll",
                    ),
                    RichMessageButton::callback(RichText::from("‹ Назад"), "inv:tab:overview"),
                ])
                .align("center"),
            ));
        }

        return InputRichMessage::blocks(blocks);
    }

    // ── MAIN OVERVIEW VIEW ──
    blocks.push(InputRichBlock::Heading(InputRichBlockSectionHeading {
        text: RichText::from("Ваши коты"),
        size: 2,
    }));

    if let Some(equip_button) = build_equip_button(session, selected) {
        blocks.push(InputRichBlock::Buttons(
            InputRichBlockButtons::new(vec![equip_button]).align("center"),
        ));
    }

    let bag_buttons = build_bag_buttons(session, selected);
    if !bag_buttons.is_empty() {
        blocks.push(InputRichBlock::Divider(InputRichBlockDivider {}));
        for chunk in bag_buttons.chunks(GRID_COLUMNS) {
            blocks.push(InputRichBlock::Buttons(
                InputRichBlockButtons::new(chunk.to_vec()).align("center"),
            ));
        }
    }

    blocks.push(InputRichBlock::Divider(InputRichBlockDivider {}));

    if pending_release == Some(selected) {
        blocks.push(InputRichBlock::Paragraph(InputRichBlockParagraph {
            text: RichText::from(vec![
                RichText::from("Точно отпустить этого кота?\nЭто действие "),
                bold("необратимо"),
            ]),
        }));
    } else {
        let (cat_type, idx) = selected.target();
        match session.inventory.get(cat_type, idx) {
            Some(slot) => {
                let (_, bond_display) = slot.cat.loyalty_status();

                blocks.push(InputRichBlock::Paragraph(InputRichBlockParagraph {
                    text: RichText::from(vec![
                        bold(slot.cat.desc()),
                        RichText::from(format!("\n{bond_display}")),
                    ]),
                }));
            }
            None => {
                let empty_text = if cat_type == "equip" {
                    "С собой никого нет"
                } else {
                    "Коробка пуста"
                };
                blocks.push(InputRichBlock::Paragraph(InputRichBlockParagraph {
                    text: RichText::from(RichTextItalic::new(empty_text)),
                }));
            }
        }
    }

    if let Some(action_row) = build_action_row(selected, pending_release) {
        for chunk in action_row.chunks(2) {
            blocks.push(InputRichBlock::Buttons(
                InputRichBlockButtons::new(chunk.to_vec()).align("center"),
            ));
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
