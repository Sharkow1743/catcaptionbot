use teloxide::prelude::*;
use teloxide::types::{CallbackQuery, ChatId, Message, MessageId, ParseMode};

use super::actions::{equip_from_bag, release_cat, unequip_current};
use super::keyboards::{build_inline_keyboard, build_reply_keyboard};
use super::renderer::{render_in_place, slot_view};
use super::types::{InventoryAction, InventorySlot, InventoryView};
use crate::controls::main_menu_keyboard;
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

    // 1. Send Reply Keyboard navigation menu
    let nav_msg = bot
        .send_message(chat_id, "Инвентарь")
        .reply_markup(build_reply_keyboard(&session.inventory))
        .await?;

    // 2. Initial selection
    let selected = if session.inventory.get("equip", 0).is_some() {
        InventorySlot::Equip
    } else if let Some(bag) = session.inventory.category("inventory") {
        bag.indexed_items()
            .next()
            .map(|(idx, _)| InventorySlot::Bag(idx))
            .unwrap_or(InventorySlot::Equip)
    } else {
        InventorySlot::Equip
    };

    let (image, caption, is_occupied) = slot_view(&session, &app_state.asset_manager, selected);
    let inline_kb = build_inline_keyboard(selected, is_occupied);

    // 3. Send photo message with contextual inline actions
    let sent = bot
        .send_photo(chat_id, image.into())
        .caption(caption)
        .reply_markup(inline_kb)
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
            nav_msg_id: Some(nav_msg.id),
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

    // Delete user's incoming message to keep the chat clean
    let _ = bot.delete_message(chat_id, msg.id).await;

    match action {
        InventoryAction::Back => {
            // Clean up previous navigation message when leaving the menu
            if let Some(nav_id) = view.nav_msg_id {
                let _ = bot.delete_message(chat_id, nav_id).await;
            }

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
    }

    Ok(())
}

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
        Ok(Some(State::Inventory(view))) => view,
        _ => InventoryView {
            msg_id,
            selected: InventorySlot::Equip,
            nav_msg_id: None,
        },
    };

    if let Some(idx_str) = data.strip_prefix("inv:equip:") {
        let Ok(idx) = idx_str.parse::<usize>() else {
            let _ = bot.answer_callback_query(q.id).await;
            return Ok(());
        };

        match equip_from_bag(&mut session, idx) {
            Ok(()) => {
                let _ = bot
                    .answer_callback_query(q.id)
                    .text("Кот экипирован!")
                    .await;
                let new_selected = InventorySlot::Equip;

                // 1. Edit photo card in place
                render_in_place(
                    &bot,
                    chat_id,
                    current_view.msg_id,
                    &mut session,
                    &app_state.asset_manager,
                    new_selected,
                )
                .await?;

                let new_nav = bot
                    .send_message(chat_id, "Инвентарь обновлен")
                    .reply_markup(build_reply_keyboard(&session.inventory))
                    .await?;

                if let Some(old_nav_id) = current_view.nav_msg_id {
                    let _ = bot.delete_message(chat_id, old_nav_id).await;
                }

                app_state.session_manager.save_session(&session).await?;
                dialogue
                    .update(State::Inventory(InventoryView {
                        msg_id: current_view.msg_id,
                        selected: new_selected,
                        nav_msg_id: Some(new_nav.id),
                    }))
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
    } else if data == "inv:unequip" {
        match unequip_current(&mut session) {
            Ok(new_bag_idx) => {
                let _ = bot
                    .answer_callback_query(q.id)
                    .text("Кот перемещен в инвентарь")
                    .await;
                let new_selected = InventorySlot::Bag(new_bag_idx);

                render_in_place(
                    &bot,
                    chat_id,
                    current_view.msg_id,
                    &mut session,
                    &app_state.asset_manager,
                    new_selected,
                )
                .await?;

                let new_nav = bot
                    .send_message(chat_id, "Инвентарь обновлен")
                    .reply_markup(build_reply_keyboard(&session.inventory))
                    .await?;

                if let Some(old_nav_id) = current_view.nav_msg_id {
                    let _ = bot.delete_message(chat_id, old_nav_id).await;
                }

                app_state.session_manager.save_session(&session).await?;
                dialogue
                    .update(State::Inventory(InventoryView {
                        msg_id: current_view.msg_id,
                        selected: new_selected,
                        nav_msg_id: Some(new_nav.id),
                    }))
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
    } else if let Some(target_slot_str) = data.strip_prefix("inv:release:") {
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

        match release_cat(&mut session, slot) {
            Ok(()) => {
                let _ = bot
                    .answer_callback_query(q.id)
                    .text("Кот отпущен на волю")
                    .await;

                let next_slot = if session.inventory.get("equip", 0).is_some() {
                    InventorySlot::Equip
                } else if let Some(bag) = session.inventory.category("inventory") {
                    bag.indexed_items()
                        .next()
                        .map(|(idx, _)| InventorySlot::Bag(idx))
                        .unwrap_or(InventorySlot::Equip)
                } else {
                    InventorySlot::Equip
                };

                render_in_place(
                    &bot,
                    chat_id,
                    current_view.msg_id,
                    &mut session,
                    &app_state.asset_manager,
                    next_slot,
                )
                .await?;

                let new_nav = bot
                    .send_message(chat_id, "Инвентарь обновлен")
                    .reply_markup(build_reply_keyboard(&session.inventory))
                    .await?;

                if let Some(old_nav_id) = current_view.nav_msg_id {
                    let _ = bot.delete_message(chat_id, old_nav_id).await;
                }

                app_state.session_manager.save_session(&session).await?;
                dialogue
                    .update(State::Inventory(InventoryView {
                        msg_id: current_view.msg_id,
                        selected: next_slot,
                        nav_msg_id: Some(new_nav.id),
                    }))
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
    }

    Ok(())
}

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
