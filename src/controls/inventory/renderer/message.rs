use teloxide::prelude::*;
use teloxide::types::{
    ChatId, InputRichBlock, InputRichBlockBlockQuotation, InputRichBlockButtons,
    InputRichBlockDivider, InputRichBlockParagraph, InputRichMessage, MessageId, RichMessageButton,
    RichText,
};

use crate::controls::inventory::types::InventorySlot;
use crate::session::UserSession;

const GRID_COLUMNS: usize = 2;

/// Builds the sole interactive surface: a 2-column grid of every slot
/// (equip pinned first, then bag), a status/detail panel for whichever
/// slot is selected, and a contextual action row (or release confirmation).
pub fn build_rich_inventory_message(
    session: &UserSession,
    selected: InventorySlot,
    pending_release: Option<InventorySlot>,
) -> InputRichMessage {
    let mut blocks: Vec<InputRichBlock> = Vec::new();

    blocks.push(InputRichBlock::Paragraph(InputRichBlockParagraph {
        text: RichText::from("Ваши коты"),
    }));

    for chunk in build_grid_buttons(session, selected).chunks(GRID_COLUMNS) {
        blocks.push(InputRichBlock::Buttons(
            InputRichBlockButtons::new(chunk.to_vec()).align("center"),
        ));
    }

    blocks.push(InputRichBlock::Divider(InputRichBlockDivider {}));

    blocks.push(InputRichBlock::Blockquote(InputRichBlockBlockQuotation {
        blocks: vec![InputRichBlock::Paragraph(InputRichBlockParagraph {
            text: RichText::from(build_detail_text(session, selected, pending_release)),
        })],
        credit: None,
    }));

    if let Some(action_row) = build_action_row(session, selected, pending_release) {
        for chunk in action_row.chunks(2) {
            blocks.push(InputRichBlock::Buttons(
                InputRichBlockButtons::new(chunk.to_vec()).align("center"),
            ));
        }
    }

    InputRichMessage::blocks(blocks)
}

fn build_grid_buttons(session: &UserSession, selected: InventorySlot) -> Vec<RichMessageButton> {
    let mut grid_buttons: Vec<RichMessageButton> = Vec::new();

    let equip_item = session.inventory.get("equip", 0);
    let equip_selected = selected == InventorySlot::Equip;
    if let Some(slot) = equip_item {
        let name = slot.cat.image_text();
        let label = if equip_selected {
            format!("◢ ◇ {name}")
        } else {
            format!("◇ {name}")
        };

        grid_buttons.push(RichMessageButton::callback(
            RichText::from(label),
            "inv:sel:equip",
        ));
    }

    if let Some(bag) = session.inventory.category("inventory") {
        for (idx, slot) in bag.indexed_items() {
            let name = slot.cat.image_text();
            let is_selected = selected == InventorySlot::Bag(idx);
            let label = if is_selected {
                format!("◢ ⌂ {name}")
            } else {
                format!("⌂ {name}")
            };
            grid_buttons.push(RichMessageButton::callback(
                RichText::from(label),
                format!("inv:sel:bag:{idx}"),
            ));
        }
    }

    grid_buttons
}

fn build_detail_text(
    session: &UserSession,
    selected: InventorySlot,
    pending_release: Option<InventorySlot>,
) -> String {
    if pending_release == Some(selected) {
        return "Внимание: точно отпустить этого кота?\nЭто действие необратимо.".to_string();
    }

    let (cat_type, idx) = selected.target();
    match session.inventory.get(cat_type, idx) {
        Some(slot) => {
            let loyalty = slot.cat.loyalty_level();
            let loyalty_pct = (loyalty * 100.0).round() as i32;
            let status_text = if loyalty >= 0.6 {
                "Преданный"
            } else if loyalty >= 0.2 {
                "Дружелюбный"
            } else if loyalty > -0.2 {
                "Нейтральный"
            } else if loyalty > -0.6 {
                "Недовольный"
            } else {
                "Обиженный"
            };

            format!(
                "Имя: {}\nРедкость: {}\nПреданность: {:+}% ({})",
                slot.cat.image_text(),
                slot.cat.rarity().name(),
                loyalty_pct,
                status_text
            )
        }
        None => "Слот пуст.".to_string(),
    }
}

fn build_action_row(
    session: &UserSession,
    selected: InventorySlot,
    pending_release: Option<InventorySlot>,
) -> Option<Vec<RichMessageButton>> {
    if pending_release == Some(selected) {
        return Some(vec![
            RichMessageButton::callback(RichText::from("Да, отпустить"), "inv:confirm_release"),
            RichMessageButton::callback(RichText::from("Отмена"), "inv:cancel_release"),
        ]);
    }

    let (cat_type, idx) = selected.target();
    if session.inventory.get(cat_type, idx).is_none() {
        return None;
    }

    Some(match selected {
        InventorySlot::Equip => vec![
            RichMessageButton::callback(RichText::from("Выложить"), "inv:act:unequip"),
            RichMessageButton::callback(RichText::from("Отпустить"), "inv:act:release:equip"),
        ],
        InventorySlot::Bag(b_idx) => vec![
            RichMessageButton::callback(RichText::from("Взять"), format!("inv:act:equip:{b_idx}")),
            RichMessageButton::callback(
                RichText::from("Отпустить"),
                format!("inv:act:release:bag:{b_idx}"),
            ),
        ],
    })
}

pub async fn render_controls_in_place(
    bot: &Bot,
    chat_id: ChatId,
    ctrl_msg_id: MessageId,
    session: &UserSession,
    selected: InventorySlot,
    pending_release: Option<InventorySlot>,
) -> anyhow::Result<()> {
    let rich_message = build_rich_inventory_message(session, selected, pending_release);

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
