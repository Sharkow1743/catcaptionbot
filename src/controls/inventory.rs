use teloxide::prelude::*;
use teloxide::types::{
    ChatId, InputMedia, InputMediaPhoto, KeyboardButton, KeyboardMarkup, Message, MessageId,
    ParseMode,
};

use crate::assets::{AssetManager, CachedImage};
use crate::content::Cat;
use crate::content::inventory::Inventory;
use crate::controls::main_menu_keyboard;
use crate::session::UserSession;
use crate::state::{AppState, DialogueState, HandlerResult, State};

const EMPTY_EQUIP_TEXT: &str =
    "🐾 Слот экипировки пуст. Загляните на «Получить», чтобы поймать кота.";

// ─── Screen state ─────────────────────────────────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InventorySlot {
    Equip,
    Bag(usize),
}

impl InventorySlot {
    #[inline]
    pub fn target(self) -> (&'static str, usize) {
        match self {
            Self::Equip => ("equip", 0),
            Self::Bag(idx) => ("inventory", idx),
        }
    }
}

#[derive(Debug, Clone)]
pub struct InventoryView {
    pub msg_id: MessageId,
    pub selected: InventorySlot,
}

// ─── Keyboard vocabulary ───────────────────────────────────────────────────

const EQUIP_LABEL: &str = "Текущий кот";
const EQUIP_ACTION_LABEL: &str = "Взять";
const BACK_LABEL: &str = "Назад";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum InventoryAction {
    ViewEquip,
    ViewBag(usize),
    Equip,
    Back,
}

impl InventoryAction {
    fn from_text(text: &str) -> Option<Self> {
        match text {
            EQUIP_LABEL => Some(Self::ViewEquip),
            EQUIP_ACTION_LABEL => Some(Self::Equip),
            BACK_LABEL => Some(Self::Back),
            other => other
                .chars()
                .next()
                .and_then(|c| c.to_digit(10))
                .map(|n| Self::ViewBag((n as usize).saturating_sub(1))),
        }
    }

    fn bag_label(idx: usize, name: String) -> String {
        format!("{} {}", idx + 1, name)
    }
}

fn inventory_keyboard(inventory: &Inventory) -> KeyboardMarkup {
    // 1. Combine EQUIP_LABEL and bag items into one vector
    let mut items: Vec<KeyboardButton> = vec![KeyboardButton::new(EQUIP_LABEL)];

    if let Some(bag) = inventory.category("inventory") {
        items.extend(bag.indexed_items().map(|(idx, slot)| {
            KeyboardButton::new(InventoryAction::bag_label(idx, slot.cat.image_text()))
        }));
    }

    // 2. Split evenly across 2 rows
    let mid = (items.len() + 1) / 2;
    let (row1, row2) = items.split_at(mid);

    let mut rows: Vec<Vec<KeyboardButton>> = vec![row1.to_vec()];
    if !row2.is_empty() {
        rows.push(row2.to_vec());
    }

    // 3. Action buttons row
    rows.push(vec![
        KeyboardButton::new(EQUIP_ACTION_LABEL),
        KeyboardButton::new(BACK_LABEL),
    ]);

    KeyboardMarkup::new(rows)
        .resize_keyboard()
        .persistent()
        .input_field_placeholder(" ")
}

// ─── Entry points & Handlers ───────────────────────────────────────────────

pub async fn display_inventory(
    msg: Message,
    bot: Bot,
    dialogue: DialogueState,
    app_state: AppState,
    mut session: UserSession,
) -> HandlerResult {
    let chat_id = msg.chat.id;

    // 1. Send the ReplyKeyboardMarkup separately so it doesn't lock the photo message from edits
    bot.send_message(chat_id, "Инвентарь")
        .reply_markup(inventory_keyboard(&session.inventory))
        .await?;

    let selected = InventorySlot::Equip;
    let (image, caption) = slot_view(&session, &app_state.asset_manager, selected);

    // 2. Send photo without reply_markup so Telegram allows edit_message_media
    let sent = bot
        .send_photo(chat_id, image.into())
        .caption(caption)
        .parse_mode(ParseMode::Html)
        .await?;

    if let Some(size) = sent
        .photo()
        .and_then(|sizes| sizes.last().or_else(|| sizes.first()))
    {
        let file_id = size.file.id.clone();
        let (cat, idx) = selected.target();
        if let Some(slot) = session.inventory.get_mut(cat, idx) {
            slot.cat.image_mut().set_file_id(file_id);
        } else {
            app_state
                .asset_manager
                .update_file_id("missing_cat", file_id);
        }
        app_state.session_manager.save_session(&session).await?;
    }

    dialogue
        .update(State::Inventory(InventoryView {
            msg_id: sent.id,
            selected,
        }))
        .await?;

    Ok(())
}

pub async fn handle_inventory_input(
    msg: Message,
    bot: Bot,
    dialogue: DialogueState,
    app_state: AppState,
    mut session: UserSession,
    view: InventoryView,
) -> HandlerResult {
    let Some(text) = msg.text() else {
        return Ok(());
    };
    let Some(action) = InventoryAction::from_text(text) else {
        return Ok(());
    };
    let chat_id = msg.chat.id;

    // Delete the incoming button tap to keep chat focused on the photo
    let _ = bot.delete_message(chat_id, msg.id).await;

    match action {
        InventoryAction::Back => {
            bot.send_message(chat_id, "Главное меню")
                .reply_markup(main_menu_keyboard())
                .await?;
            dialogue.update(State::Start).await?;
        }

        InventoryAction::ViewEquip => {
            update_view(
                &bot,
                chat_id,
                &dialogue,
                &mut session,
                &app_state,
                view,
                InventorySlot::Equip,
            )
            .await?;
        }

        InventoryAction::ViewBag(idx) => {
            update_view(
                &bot,
                chat_id,
                &dialogue,
                &mut session,
                &app_state,
                view,
                InventorySlot::Bag(idx),
            )
            .await?;
        }

        InventoryAction::Equip => {
            if let InventorySlot::Bag(idx) = view.selected {
                equip_from_bag(&mut session.inventory, idx);
                update_view(
                    &bot,
                    chat_id,
                    &dialogue,
                    &mut session,
                    &app_state,
                    view,
                    InventorySlot::Equip,
                )
                .await?;
            }
        }
    }

    Ok(())
}

// ─── Rendering & Helpers ───────────────────────────────────────────────────

async fn update_view(
    bot: &Bot,
    chat_id: ChatId,
    dialogue: &DialogueState,
    session: &mut UserSession,
    app_state: &AppState,
    view: InventoryView,
    selected: InventorySlot,
) -> HandlerResult {
    render_in_place(
        bot,
        chat_id,
        view.msg_id,
        session,
        &app_state.asset_manager,
        selected,
    )
    .await?;

    app_state.session_manager.save_session(session).await?;
    dialogue
        .update(State::Inventory(InventoryView { selected, ..view }))
        .await?;

    Ok(())
}

async fn render_in_place(
    bot: &Bot,
    chat_id: ChatId,
    msg_id: MessageId,
    session: &mut UserSession,
    asset_manager: &AssetManager,
    selected: InventorySlot,
) -> HandlerResult {
    let (image, caption) = slot_view(&session, asset_manager, selected);

    let photo = InputMediaPhoto::new(image.into())
        .caption(caption.clone())
        .parse_mode(ParseMode::Html);

    match bot
        .edit_message_media(chat_id, msg_id, InputMedia::Photo(photo))
        .await
    {
        Ok(edited) => {
            if let Some(size) = edited
                .photo()
                .and_then(|sizes| sizes.last().or_else(|| sizes.first()))
            {
                let file_id = size.file.id.clone();
                let (cat, idx) = selected.target();
                if let Some(slot) = session.inventory.get_mut(cat, idx) {
                    slot.cat.image_mut().set_file_id(file_id);
                } else {
                    asset_manager.update_file_id("missing_cat", file_id);
                }
            }
        }
        Err(err) => {
            let err_msg = err.to_string();
            if err_msg.contains("message is not modified") {
                return Ok(());
            }

            // Fallback in case media is unchanged: edit caption in place
            let cap_res = bot
                .edit_message_caption(chat_id, msg_id)
                .caption(caption)
                .parse_mode(ParseMode::Html)
                .await;

            if let Err(c_err) = cap_res {
                let c_err_msg = c_err.to_string();
                if !c_err_msg.contains("message is not modified") {
                    return Err(c_err.into());
                }
            }
        }
    }

    Ok(())
}

fn slot_view(
    session: &UserSession,
    asset_manager: &AssetManager,
    selected: InventorySlot,
) -> (CachedImage, String) {
    let (category, idx) = selected.target();
    match session.inventory.get(category, idx) {
        Some(slot) => (
            slot.cat.image().clone(),
            cat_caption(session, category, &slot.cat),
        ),
        None => (
            asset_manager.require("missing_cat"),
            EMPTY_EQUIP_TEXT.to_string(),
        ),
    }
}

fn equip_from_bag(inventory: &mut Inventory, idx: usize) {
    let Some(incoming) = inventory.take("inventory", idx) else {
        return;
    };
    let outgoing = inventory.take("equip", 0);
    inventory.set("equip", 0, incoming);
    if let Some(outgoing) = outgoing {
        inventory.set("inventory", idx, outgoing);
    }
}

fn cat_caption(session: &UserSession, category: &str, cat: &Cat) -> String {
    let desc = if category == "equip" {
        "Текущий кот"
    } else {
        ""
    };

    format!("{desc}\n\n{}", cat_stats(&cat))
}

fn cat_stats(_cat: &Cat) -> String {
    "".to_string()
}
