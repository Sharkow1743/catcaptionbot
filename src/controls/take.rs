use teloxide::prelude::*;
use teloxide::types::{
    ChatId, InlineKeyboardButton, InlineKeyboardMarkup, InputMedia, InputMediaPhoto, MessageId,
};

use crate::assets::AssetManager;
use crate::content::Cat;
use crate::content::inventory::Slot;
use crate::session::UserSession;

pub fn cat_inline_keyboard() -> InlineKeyboardMarkup {
    InlineKeyboardMarkup::new([[InlineKeyboardButton::callback("Забрать", "take_cat")]])
}

pub async fn handle_take_cat(
    session: &mut UserSession,
    bot: Bot,
    chat_id: ChatId,
    msg_id: MessageId,
    asset_manager: &AssetManager,
) {
    let Some(content) = session.last_content.take() else {
        return;
    };

    // 1. Calculate catch chance via modifier hook
    let catch_probability: f32 = session
        .fire_hook("cat_catch_probability", 1.0_f32)
        .clamp(0.0, 1.0);

    let is_caught = if catch_probability >= 1.0 {
        true
    } else if catch_probability <= 0.0 {
        false
    } else {
        rand::random::<f32>() < catch_probability
    };

    if !is_caught {
        session.fire_trigger("on_cat_escape");
        cat_escape(
            bot,
            chat_id,
            msg_id,
            asset_manager,
            "Кот оказался слишком шустрым и убежал!",
        )
        .await;
        return;
    }

    // 2. Try placing into equipment or general inventory
    let slot = Slot {
        cat: Cat::new(content),
    };
    let placed = match session.inventory.push("equip", slot) {
        Ok(()) => true,
        Err(slot) => session.inventory.push("inventory", slot).is_ok(),
    };

    if placed {
        session.fire_trigger("on_cat_catch");
        let _ = bot
            .edit_message_caption(chat_id, msg_id)
            .caption("Этот кот теперь ваш!")
            .reply_markup(InlineKeyboardMarkup::default())
            .await;
    } else {
        session.fire_trigger("on_cat_escape");
        cat_escape(bot, chat_id, msg_id, asset_manager, "Ему не нашлось места").await;
    }
}

pub async fn cat_escape(
    bot: Bot,
    chat_id: ChatId,
    msg_id: MessageId,
    asset_manager: &AssetManager,
    reason: &str,
) {
    let escape_image = asset_manager.require("missing_cat");
    let caption = if reason.is_empty() {
        "Кот сбежал!".to_string()
    } else {
        format!("Кот сбежал! {}", reason)
    };

    let photo = InputMediaPhoto::new(escape_image.into()).caption(caption);
    if let Ok(msg_meta) = bot
        .edit_message_media(chat_id, msg_id, InputMedia::Photo(photo))
        .reply_markup(InlineKeyboardMarkup::default())
        .await
    {
        if let Some(first_photo) = msg_meta.photo().and_then(|photos| photos.first()) {
            // was mistakenly cached under "cat_escape" — the registered key is "missing_cat"
            asset_manager.update_file_id("missing_cat", first_photo.file.id.clone());
        }
    }
}
