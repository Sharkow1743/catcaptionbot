use std::time::Duration;

use teloxide::prelude::*;
use teloxide::types::{ChatId, Message, MessageId, ParseMode};

use super::default_selection;
use crate::controls::inventory::renderer::{
    build_rich_inventory_message, render_card_content, render_controls_in_place,
};
use crate::controls::inventory::types::{InventoryTab, InventoryView};
use crate::controls::inventory_menu_keyboard;
use crate::session::UserSession;
use crate::state::{AppState, DialogueState, HandlerResult, State};
use crate::utils::unix_timestamp;

pub async fn display_inventory(
    msg: Message,
    bot: Bot,
    dialogue: DialogueState,
    app_state: AppState,
    mut session: UserSession,
) -> HandlerResult {
    let chat_id = msg.chat.id;

    if session.last_content.is_some() {
        session.last_content = None;
    }
    if let Some(last_roll_msg_id) = session.last_roll_msg_id {
        let _ = bot
            .edit_message_reply_markup(msg.chat.id, last_roll_msg_id)
            .await;
    }

    // Update passive loyalty to current moment before displaying
    let now = unix_timestamp();
    if let Some(equipped) = session.inventory.get_mut("equip", 0) {
        equipped.cat.tick_loyalty(true, now);
    }
    if let Some(bag) = session.inventory.category_mut("inventory") {
        for slot in bag.items_mut() {
            slot.cat.tick_loyalty(false, now);
        }
    }

    // Clean up previous control board if exists
    if let Ok(Some(State::Inventory(old_view))) = dialogue.get().await {
        let _ = bot.delete_message(chat_id, old_view.ctrl_msg_id).await;
    }

    // Switch reply keyboard to inventory mode via a separate message
    let _ = bot
        .send_message(chat_id, "Ваши коты")
        .reply_markup(inventory_menu_keyboard())
        .await;

    let selected = default_selection(&session);

    // 1. Send Message 1: Showcase Card
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

    // 2. Send Message 2: Rich Message grid
    let rich_msg = build_rich_inventory_message(&session, selected, InventoryTab::Overview, None);
    let ctrl_msg = bot.send_rich_message(chat_id, rich_msg).await?;

    let view = InventoryView::new(card_msg.id, ctrl_msg.id, selected);
    dialogue.update(State::Inventory(view)).await?;

    // 3. Spawn background periodic updater with gradual backoff
    spawn_inventory_updater(
        bot,
        chat_id,
        ctrl_msg.id,
        dialogue,
        app_state,
        session.user_id,
    );

    Ok(())
}

fn spawn_inventory_updater(
    bot: Bot,
    chat_id: ChatId,
    ctrl_msg_id: MessageId,
    dialogue: DialogueState,
    app_state: AppState,
    user_id: u64,
) {
    tokio::spawn(async move {
        let intervals = [
            Duration::from_secs(4),
            Duration::from_secs(4),
            Duration::from_secs(5),
            Duration::from_secs(6),
            Duration::from_secs(8),
            Duration::from_secs(10),
            Duration::from_secs(15),
            Duration::from_secs(20),
            Duration::from_secs(30),
            Duration::from_secs(45),
            Duration::from_secs(60),
        ];

        let mut last_loyalty_state: Vec<i32> = Vec::new();

        for interval in intervals {
            tokio::time::sleep(interval).await;

            let current_view = match dialogue.get().await {
                Ok(Some(State::Inventory(view))) if view.ctrl_msg_id == ctrl_msg_id => view,
                _ => break,
            };

            let Ok(mut session) = app_state.session_manager.get_session(user_id).await else {
                break;
            };

            let now = unix_timestamp();
            let mut any_can_regen = false;
            let mut current_loyalty_state = Vec::new();

            if let Some(equipped) = session.inventory.get_mut("equip", 0) {
                if equipped.cat.loyalty_level() < 1.0 {
                    any_can_regen = true;
                }
                equipped.cat.tick_loyalty(true, now);
                current_loyalty_state.push((equipped.cat.loyalty_level() * 100.0).round() as i32);
            }

            if let Some(bag) = session.inventory.category_mut("inventory") {
                for slot in bag.items_mut() {
                    if slot.cat.loyalty_level() < 1.0 {
                        any_can_regen = true;
                    }
                    slot.cat.tick_loyalty(false, now);
                    current_loyalty_state.push((slot.cat.loyalty_level() * 100.0).round() as i32);
                }
            }

            if !any_can_regen {
                break;
            }

            if current_loyalty_state == last_loyalty_state {
                continue;
            }
            last_loyalty_state = current_loyalty_state;

            let _ = app_state.session_manager.save_session(&session).await;

            if render_controls_in_place(
                &bot,
                chat_id,
                current_view.ctrl_msg_id,
                &session,
                current_view.selected,
                current_view.tab,
                current_view.pending_release,
            )
            .await
            .is_err()
            {
                break;
            }
        }
    });
}
