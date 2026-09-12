use teloxide::prelude::*;
use teloxide::types::{ChatId, InputMedia, InputMediaPhoto, MessageId, ParseMode};

use crate::assets::{AssetManager, CachedImage};
use crate::controls::inventory::types::InventorySlot;
use crate::session::UserSession;

pub fn render_card_content(
    session: &UserSession,
    asset_manager: &AssetManager,
    selected: InventorySlot,
) -> (CachedImage, String, bool) {
    let (category, idx) = selected.target();
    match session.inventory.get(category, idx) {
        Some(slot) => {
            let status = if category == "equip" {
                "С собой"
            } else {
                "В коробке"
            };
            let caption = format!(
                "<blockquote>{}</blockquote>\n<i>{status}</i>",
                slot.cat.quote()
            );
            (slot.cat.image().clone(), caption, true)
        }
        None => {
            let caption = if category == "equip" {
                "С собой никого"
            } else {
                "Коробка пуста"
            };
            (
                asset_manager.require("missing_cat"),
                caption.to_string(),
                false,
            )
        }
    }
}

pub async fn render_card_in_place(
    bot: &Bot,
    chat_id: ChatId,
    card_msg_id: MessageId,
    session: &UserSession,
    asset_manager: &AssetManager,
    selected: InventorySlot,
) -> anyhow::Result<()> {
    let (image, caption, _) = render_card_content(session, asset_manager, selected);

    let photo = InputMediaPhoto::new(image.into())
        .caption(caption)
        .parse_mode(ParseMode::Html);

    let res = bot
        .edit_message_media(chat_id, card_msg_id, InputMedia::Photo(photo))
        .await;

    match res {
        Ok(edited) => {
            if let Some(size) = edited
                .photo()
                .and_then(|sizes| sizes.last().or_else(|| sizes.first()))
            {
                let file_id = size.file.id.clone();
                let (cat, idx) = selected.target();
                if session.inventory.get(cat, idx).is_none() {
                    asset_manager.update_file_id("missing_cat", file_id);
                }
            }
        }
        Err(err) => {
            let err_msg = err.to_string();
            if !err_msg.contains("message is not modified") {
                log::warn!("Failed to edit showcase card media: {err_msg}");
                return Err(err.into());
            }
        }
    }

    Ok(())
}
