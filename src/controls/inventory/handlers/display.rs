use teloxide::prelude::*;
use teloxide::types::{Message, ParseMode};

use super::default_selection;
use crate::controls::inventory::renderer::{build_rich_inventory_message, render_card_content};
use crate::controls::inventory::types::InventoryView;
use crate::session::UserSession;
use crate::state::{AppState, DialogueState, HandlerResult, State};

pub async fn display_inventory(
    msg: Message,
    bot: Bot,
    dialogue: DialogueState,
    app_state: AppState,
    mut session: UserSession,
) -> HandlerResult {
    let chat_id = msg.chat.id;

    // Delete user's incoming message
    //let _ = bot.delete_message(chat_id, msg.id).await;

    // Clean up previous control board if exists
    if let Ok(Some(State::Inventory(old_view))) = dialogue.get().await {
        let _ = bot.delete_message(chat_id, old_view.ctrl_msg_id).await;
    }

    let selected = default_selection(&session);

    // 1. Send Message 1: Showcase Card (clean forwardable photo, no buttons)
    let (image, caption, _) = render_card_content(&session, &app_state.asset_manager, selected);
    let card_msg = bot
        .send_photo(chat_id, image.into())
        .caption(caption)
        .parse_mode(ParseMode::Html)
        .await?;

    if let Some(size) = card_msg
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

    // 2. Send Message 2: Rich Message grid — the interactive surface
    let rich_msg = build_rich_inventory_message(&session, selected, None);
    let ctrl_msg = bot.send_rich_message(chat_id, rich_msg).await?;

    let view = InventoryView::new(card_msg.id, ctrl_msg.id, selected);
    dialogue.update(State::Inventory(view)).await?;

    Ok(())
}
