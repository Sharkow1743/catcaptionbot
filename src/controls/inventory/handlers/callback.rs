use teloxide::prelude::*;
use teloxide::types::{CallbackQuery, ChatId, MessageId};

use super::{default_selection, view::apply_inventory_view};
use crate::controls::inventory::actions::{equip_from_bag, release_cat, unequip_current};
use crate::controls::inventory::types::{InventorySlot, InventoryView};
use crate::session::UserSession;
use crate::state::{AppState, DialogueState, HandlerResult, State};

pub async fn handle_inventory_callback(
    bot: Bot,
    q: CallbackQuery,
    chat_id: ChatId,
    msg_id: MessageId,
    data: &str,
    app_state: AppState,
    dialogue: DialogueState,
    mut session: UserSession,
) -> HandlerResult {
    let current_view = match dialogue.get().await {
        Ok(Some(State::Inventory(view))) if view.ctrl_msg_id == msg_id => view,
        _ => {
            let _ = bot
                .answer_callback_query(q.id)
                .text("Меню устарело")
                .show_alert(true)
                .await;
            let _ = bot.delete_message(chat_id, msg_id).await;
            return Ok(());
        }
    };

    if data == "inv:sel:equip" {
        let _ = bot.answer_callback_query(q.id).await;
        let next_view = InventoryView {
            selected: InventorySlot::Equip,
            pending_release: None,
            ..current_view
        };
        apply_inventory_view(
            &bot,
            chat_id,
            &dialogue,
            &mut session,
            &app_state,
            next_view,
        )
        .await?;
    } else if let Some(idx_str) = data.strip_prefix("inv:sel:bag:") {
        let _ = bot.answer_callback_query(q.id).await;
        if let Ok(idx) = idx_str.parse::<usize>() {
            let next_view = InventoryView {
                selected: InventorySlot::Bag(idx),
                pending_release: None,
                ..current_view
            };
            apply_inventory_view(
                &bot,
                chat_id,
                &dialogue,
                &mut session,
                &app_state,
                next_view,
            )
            .await?;
        }
    } else if let Some(idx_str) = data.strip_prefix("inv:act:equip:") {
        let Ok(idx) = idx_str.parse::<usize>() else {
            let _ = bot.answer_callback_query(q.id).await;
            return Ok(());
        };

        match equip_from_bag(&mut session, idx) {
            Ok(()) => {
                let _ = bot
                    .answer_callback_query(q.id)
                    .text("Кот взят с собой")
                    .await;
                let next_view = InventoryView {
                    selected: InventorySlot::Equip,
                    pending_release: None,
                    ..current_view
                };
                apply_inventory_view(
                    &bot,
                    chat_id,
                    &dialogue,
                    &mut session,
                    &app_state,
                    next_view,
                )
                .await?;
            }
            Err(err) => {
                let _ = bot
                    .answer_callback_query(q.id)
                    .text(err)
                    .show_alert(true)
                    .await;
            }
        }
    } else if data == "inv:act:unequip" {
        match unequip_current(&mut session) {
            Ok(new_bag_idx) => {
                let _ = bot
                    .answer_callback_query(q.id)
                    .text("Кот перемещен в коробку")
                    .await;
                let next_view = InventoryView {
                    selected: InventorySlot::Bag(new_bag_idx),
                    pending_release: None,
                    ..current_view
                };
                apply_inventory_view(
                    &bot,
                    chat_id,
                    &dialogue,
                    &mut session,
                    &app_state,
                    next_view,
                )
                .await?;
            }
            Err(err) => {
                let _ = bot
                    .answer_callback_query(q.id)
                    .text(err)
                    .show_alert(true)
                    .await;
            }
        }
    } else if let Some(target_slot_str) = data.strip_prefix("inv:act:release:") {
        let slot_to_release = if target_slot_str == "equip" {
            Some(InventorySlot::Equip)
        } else if let Some(idx_str) = target_slot_str.strip_prefix("bag:") {
            idx_str.parse::<usize>().ok().map(InventorySlot::Bag)
        } else {
            None
        };

        let Some(slot) = slot_to_release else {
            let _ = bot.answer_callback_query(q.id).await;
            return Ok(());
        };

        let _ = bot.answer_callback_query(q.id).await;
        let next_view = InventoryView {
            selected: slot,
            pending_release: Some(slot),
            ..current_view
        };
        apply_inventory_view(
            &bot,
            chat_id,
            &dialogue,
            &mut session,
            &app_state,
            next_view,
        )
        .await?;
    } else if data == "inv:confirm_release" {
        let Some(slot) = current_view.pending_release else {
            let _ = bot.answer_callback_query(q.id).await;
            return Ok(());
        };

        match release_cat(&mut session, slot) {
            Ok(()) => {
                let _ = bot
                    .answer_callback_query(q.id)
                    .text("Кот отпущен на волю")
                    .await;
                let next_view = InventoryView {
                    selected: default_selection(&session),
                    pending_release: None,
                    ..current_view
                };
                apply_inventory_view(
                    &bot,
                    chat_id,
                    &dialogue,
                    &mut session,
                    &app_state,
                    next_view,
                )
                .await?;
            }
            Err(err) => {
                let _ = bot
                    .answer_callback_query(q.id)
                    .text(err)
                    .show_alert(true)
                    .await;
            }
        }
    } else if data == "inv:cancel_release" {
        let _ = bot.answer_callback_query(q.id).await;
        let next_view = InventoryView {
            pending_release: None,
            ..current_view
        };
        apply_inventory_view(
            &bot,
            chat_id,
            &dialogue,
            &mut session,
            &app_state,
            next_view,
        )
        .await?;
    }

    app_state.session_manager.save_session(&session).await?;

    Ok(())
}
