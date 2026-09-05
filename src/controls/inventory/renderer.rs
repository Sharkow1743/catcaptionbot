use teloxide::prelude::*;
use teloxide::types::{ChatId, InputMedia, InputMediaPhoto, MessageId, ParseMode};

use super::keyboards::build_inline_keyboard;
use super::types::InventorySlot;
use crate::assets::{AssetManager, CachedImage};
use crate::content::Cat;
use crate::session::UserSession;

pub const EMPTY_EQUIP_TEXT: &str = "Пусто";
pub const EMPTY_BAG_TEXT: &str = "Пусто";

pub fn slot_view(
    session: &UserSession,
    asset_manager: &AssetManager,
    selected: InventorySlot,
) -> (CachedImage, String, bool) {
    let (category, idx) = selected.target();
    match session.inventory.get(category, idx) {
        Some(slot) => (
            slot.cat.image().clone(),
            cat_caption(session, category, &slot.cat),
            true,
        ),
        None => {
            let text = if category == "equip" {
                EMPTY_EQUIP_TEXT
            } else {
                EMPTY_BAG_TEXT
            };
            (
                asset_manager.require("missing_cat"),
                text.to_string(),
                false,
            )
        }
    }
}

pub async fn render_in_place(
    bot: &Bot,
    chat_id: ChatId,
    msg_id: MessageId,
    session: &mut UserSession,
    asset_manager: &AssetManager,
    selected: InventorySlot,
) -> anyhow::Result<()> {
    let (image, caption, is_occupied) = slot_view(session, asset_manager, selected);
    let inline_kb = build_inline_keyboard(selected, is_occupied);

    let photo = InputMediaPhoto::new(image.into())
        .caption(caption.clone())
        .parse_mode(ParseMode::Html);

    match bot
        .edit_message_media(chat_id, msg_id, InputMedia::Photo(photo))
        .reply_markup(inline_kb.clone())
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

            let cap_res = bot
                .edit_message_caption(chat_id, msg_id)
                .caption(caption)
                .reply_markup(inline_kb)
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

fn cat_caption(_session: &UserSession, category: &str, _cat: &Cat) -> String {
    let desc = if category == "equip" {
        "Текущий кот"
    } else {
        ""
    };

    desc.to_string()
}
