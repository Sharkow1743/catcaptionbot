use teloxide::types::{InlineKeyboardButton, InlineKeyboardMarkup, KeyboardButton, KeyboardMarkup};

use super::types::{BACK_LABEL, EQUIP_LABEL, InventoryAction, InventorySlot};
use crate::content::inventory::Inventory;

pub fn build_reply_keyboard(inventory: &Inventory) -> KeyboardMarkup {
    let mut items: Vec<KeyboardButton> = Vec::new();

    // 1. Only display "Текущий кот" if the equip slot is occupied
    if inventory.get("equip", 0).is_some() {
        items.push(KeyboardButton::new(EQUIP_LABEL));
    }

    // 2. Only display buttons for occupied bag slots
    if let Some(bag) = inventory.category("inventory") {
        for (idx, slot) in bag.indexed_items() {
            items.push(KeyboardButton::new(InventoryAction::bag_label(
                idx,
                &slot.cat.image_text(),
            )));
        }
    }

    // 3. Split navigation buttons evenly into up to 2 rows
    let mut rows: Vec<Vec<KeyboardButton>> = Vec::new();
    if !items.is_empty() {
        let mid = (items.len() + 1) / 2;
        let (row1, row2) = items.split_at(mid);
        rows.push(row1.to_vec());
        if !row2.is_empty() {
            rows.push(row2.to_vec());
        }
    }

    // 4. Back button always pinned at the bottom
    rows.push(vec![KeyboardButton::new(BACK_LABEL)]);

    KeyboardMarkup::new(rows)
        .resize_keyboard()
        .persistent()
        .input_field_placeholder(" ")
}

pub fn build_inline_keyboard(selected: InventorySlot, is_occupied: bool) -> InlineKeyboardMarkup {
    if !is_occupied {
        return InlineKeyboardMarkup::default();
    }

    match selected {
        InventorySlot::Bag(idx) => InlineKeyboardMarkup::new(vec![vec![
            InlineKeyboardButton::callback("Взять", format!("inv:equip:{idx}")),
            InlineKeyboardButton::callback("Отпустить", format!("inv:release:bag:{idx}")),
        ]]),
        InventorySlot::Equip => InlineKeyboardMarkup::new(vec![vec![
            InlineKeyboardButton::callback("Снять", "inv:unequip"),
            InlineKeyboardButton::callback("Отпустить", "inv:release:equip"),
        ]]),
    }
}
