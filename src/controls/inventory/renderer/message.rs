use teloxide::prelude::*;
use teloxide::types::{
    ChatId, InputRichBlock, InputRichBlockButtons, InputRichBlockDivider, InputRichBlockParagraph,
    InputRichBlockSectionHeading, InputRichMessage, MessageId, RichMessageButton, RichText,
    RichTextBold, RichTextItalic,
};

use crate::controls::inventory::types::InventorySlot;
use crate::session::UserSession;

const GRID_COLUMNS: usize = 2;

pub fn build_rich_inventory_message(
    session: &UserSession,
    selected: InventorySlot,
    pending_release: Option<InventorySlot>,
) -> InputRichMessage {
    let mut blocks: Vec<InputRichBlock> = Vec::new();

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

    blocks.push(InputRichBlock::Paragraph(InputRichBlockParagraph {
        text: build_detail_text(session, selected, pending_release),
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

fn build_detail_text(
    session: &UserSession,
    selected: InventorySlot,
    pending_release: Option<InventorySlot>,
) -> RichText {
    if pending_release == Some(selected) {
        return RichText::from(vec![
            RichText::from("Точно отпустить этого кота?\nЭто действие "),
            RichText::from(RichTextBold::new("необратимо")),
        ]);
    }

    let (cat_type, idx) = selected.target();
    match session.inventory.get(cat_type, idx) {
        Some(slot) => {
            let (_, bond_display) = slot.cat.loyalty_status();

            RichText::from(vec![
                RichText::from(RichTextBold::new(slot.cat.desc())),
                RichText::from(format!("\n{bond_display}")),
            ])
        }
        None => {
            let empty_text = if cat_type == "equip" {
                "С собой никого"
            } else {
                "Пусто"
            };
            RichText::from(RichTextItalic::new(empty_text))
        }
    }
}

fn build_action_row(
    session: &UserSession,
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

    let (cat_type, idx) = selected.target();
    if session.inventory.get(cat_type, idx).is_none() {
        return None;
    }

    Some(match selected {
        InventorySlot::Equip => vec![
            RichMessageButton::callback(RichText::from("Выложить"), "inv:act:unequip"),
            RichMessageButton::callback(RichText::from("Отпустить"), "inv:act:release:equip")
                .style("danger"),
        ],
        InventorySlot::Bag(b_idx) => vec![
            RichMessageButton::callback(RichText::from("Взять"), format!("inv:act:equip:{b_idx}")),
            RichMessageButton::callback(
                RichText::from("Отпустить"),
                format!("inv:act:release:bag:{b_idx}"),
            )
            .style("danger"),
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
