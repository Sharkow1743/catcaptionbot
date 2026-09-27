pub mod inline;
pub mod inventory;
pub mod kb_command;
pub mod roll;
pub mod stats;
pub mod take;

pub use inline::handle_inline_callback;
pub use kb_command::KbCommand;

use inventory::{InventoryView, display_inventory};
use roll::serve_cat_roll;
use stats::display_stats;
use teloxide::Bot;
use teloxide::payloads::SendMessageSetters;
use teloxide::prelude::Requester;
use teloxide::types::{KeyboardMarkup, Message};

use crate::content::Content;
use crate::queue::KeyedBufferedQueue;
use crate::reply_kb;
use crate::session::UserSession;
use crate::state::{AppState, DialogueState, HandlerResult, State};

pub fn main_menu_keyboard() -> KeyboardMarkup {
    reply_kb!(
        [
            KbCommand::Stats.to_string(),
            KbCommand::Inventory.to_string(),
            KbCommand::Battles.to_string()
        ],
        [KbCommand::Get.to_string()],
    )
    .persistent()
    .input_field_placeholder(" ")
}

pub fn battle_active_keyboard() -> KeyboardMarkup {
    reply_kb!(
        [
            KbCommand::Inventory.to_string(),
            KbCommand::Surrender.to_string()
        ],
        [KbCommand::Get.to_string()],
    )
    .persistent()
    .input_field_placeholder("Дуэль в процессе")
}

pub fn battles_menu_keyboard(in_queue: bool) -> KeyboardMarkup {
    if in_queue {
        reply_kb!(
            [KbCommand::CancelQueue.to_string()],
            [KbCommand::Back.to_string()]
        )
        .persistent()
        .input_field_placeholder("Поиск соперника...")
    } else {
        reply_kb!(
            [
                KbCommand::RandomBattle.to_string(),
                KbCommand::InviteBattle.to_string()
            ],
            [KbCommand::Back.to_string()]
        )
        .persistent()
        .input_field_placeholder("Выберите режим дуэли")
    }
}

pub fn inventory_menu_keyboard() -> KeyboardMarkup {
    reply_kb!(
        [KbCommand::Stats.to_string(), KbCommand::Battles.to_string()],
        [KbCommand::Get.to_string()],
    )
    .persistent()
    .input_field_placeholder(" ")
}

pub async fn handle_keyboard_callback(
    msg: Message,
    bot: Bot,
    cmd: KbCommand,
    content_queue: KeyedBufferedQueue<u64, UserSession, anyhow::Result<Content>>,
    dialogue: DialogueState,
    session: UserSession,
    app_state: AppState,
) -> HandlerResult {
    let user_id = session.user_id;
    let chat_id = msg.chat.id.0;
    let username = msg.from.as_ref().and_then(|u| u.username.clone());

    match cmd {
        KbCommand::Get => {
            serve_cat_roll(msg, bot, content_queue, session, app_state).await?;
        }
        KbCommand::Inventory => {
            display_inventory(msg, bot, dialogue, app_state, session).await?;
        }
        KbCommand::Battles => {
            let in_battle =
                crate::content::battle::is_in_active_battle(&app_state.session_manager, user_id)
                    .await;
            if in_battle {
                bot.send_message(msg.chat.id, "Вы уже участвуете в активной дуэли")
                    .reply_markup(battle_active_keyboard())
                    .await?;
                return Ok(());
            }

            let in_queue =
                crate::content::battle::is_in_queue_or_waiting(&app_state.session_manager, user_id)
                    .await;
            let text = if in_queue {
                "<b>Поиск дуэли</b>\n\nИдет подбор соперника. Вы можете продолжать получать котов, пока ожидаете оппонента"
            } else {
                "<b>Дуэли</b>\n\n<b>Случайная дуэль</b> — быстрый поиск соперника онлайн\n<b>Позвать друга</b> — создать ссылку-приглашение"
            };

            bot.send_message(msg.chat.id, text)
                .parse_mode(teloxide::types::ParseMode::Html)
                .reply_markup(battles_menu_keyboard(in_queue))
                .await?;
        }
        KbCommand::Stats => {
            display_stats(msg, bot, session, app_state).await?;
        }
        KbCommand::RandomBattle => {
            let res = crate::content::battle::queue_random_match(
                &app_state.session_manager,
                &bot,
                user_id,
                chat_id,
                username,
            )
            .await?;

            match res {
                crate::content::battle::QueueResult::Queued => {
                    bot.send_message(
                        msg.chat.id,
                        "<b>Поиск соперника начат</b>\n\nВы можете продолжать получать котов, пока ожидаете оппонента",
                    )
                    .parse_mode(teloxide::types::ParseMode::Html)
                    .reply_markup(battles_menu_keyboard(true))
                    .await?;
                }
                crate::content::battle::QueueResult::Matched => {}
                crate::content::battle::QueueResult::AlreadyInBattle => {
                    bot.send_message(msg.chat.id, "Вы уже участвуете в активной дуэли")
                        .reply_markup(battle_active_keyboard())
                        .await?;
                }
            }
        }
        KbCommand::InviteBattle => {
            match crate::content::battle::create_invite_duel(
                &app_state.session_manager,
                &bot,
                user_id,
                chat_id,
                username,
            )
            .await
            {
                Ok((_, invite_link)) => {
                    let text = format!(
                        "<b>Дуэль создана</b>\n\nОтправьте эту ссылку оппоненту:\n<code>{}</code>\n\nКак только игрок перейдет по ссылке, дуэль начнется автоматически\nВы можете продолжать получать котов, пока ожидаете оппонента",
                        invite_link
                    );
                    bot.send_message(msg.chat.id, text)
                        .parse_mode(teloxide::types::ParseMode::Html)
                        .reply_markup(battles_menu_keyboard(true))
                        .await?;
                }
                Err(e) => {
                    bot.send_message(msg.chat.id, format!("Не удалось создать дуэль: {e}"))
                        .reply_markup(main_menu_keyboard())
                        .await?;
                }
            }
        }
        KbCommand::CancelQueue => {
            if crate::content::battle::cancel_waiting(&app_state.session_manager, &bot, user_id)
                .await
            {
                bot.send_message(msg.chat.id, "Поиск дуэли отменён")
                    .reply_markup(main_menu_keyboard())
                    .await?;
            } else {
                bot.send_message(msg.chat.id, "У вас нет активного поиска дуэли")
                    .reply_markup(main_menu_keyboard())
                    .await?;
            }
        }
        KbCommand::Surrender => {
            if crate::content::battle::surrender_battle(&app_state.session_manager, &bot, user_id)
                .await?
            {
                // Handled in finalize_battle
            } else {
                bot.send_message(msg.chat.id, "Вы не находитесь в активной дуэли")
                    .reply_markup(main_menu_keyboard())
                    .await?;
            }
        }
        KbCommand::Back => {
            let in_battle =
                crate::content::battle::is_in_active_battle(&app_state.session_manager, user_id)
                    .await;
            let kb = if in_battle {
                battle_active_keyboard()
            } else {
                main_menu_keyboard()
            };

            bot.send_message(msg.chat.id, "Главное меню")
                .reply_markup(kb)
                .await?;
        }
    }
    Ok(())
}

pub async fn handle_inventory_message(
    msg: Message,
    bot: Bot,
    dialogue: DialogueState,
    app_state: AppState,
    session: UserSession,
    view: InventoryView,
    content_queue: KeyedBufferedQueue<u64, UserSession, anyhow::Result<Content>>,
) -> HandlerResult {
    let text = msg.text().unwrap_or_default();

    if let Some(cmd) = KbCommand::from_display_text(text) {
        match cmd {
            KbCommand::Inventory => {
                let _ = bot.delete_message(msg.chat.id, msg.id).await;
                return Ok(());
            }
            KbCommand::Get => {
                let _ = bot.delete_message(msg.chat.id, view.ctrl_msg_id).await;
                dialogue.update(State::Start).await?;

                let in_battle = crate::content::battle::is_in_active_battle(
                    &app_state.session_manager,
                    session.user_id,
                )
                .await;
                let kb = if in_battle {
                    battle_active_keyboard()
                } else {
                    main_menu_keyboard()
                };

                let _ = bot
                    .send_message(
                        msg.chat.id,
                        if in_battle {
                            "Возврат к дуэли"
                        } else {
                            "Главное меню"
                        },
                    )
                    .reply_markup(kb)
                    .await;

                serve_cat_roll(msg, bot, content_queue, session, app_state).await?;
                return Ok(());
            }
            KbCommand::Battles => {
                let _ = bot.delete_message(msg.chat.id, view.ctrl_msg_id).await;
                dialogue.update(State::Start).await?;
                handle_keyboard_callback(
                    msg,
                    bot,
                    KbCommand::Battles,
                    content_queue,
                    dialogue,
                    session,
                    app_state,
                )
                .await?;
                return Ok(());
            }
            KbCommand::Stats => {
                let _ = bot.delete_message(msg.chat.id, view.ctrl_msg_id).await;
                dialogue.update(State::Start).await?;
                handle_keyboard_callback(
                    msg,
                    bot,
                    KbCommand::Stats,
                    content_queue,
                    dialogue,
                    session,
                    app_state,
                )
                .await?;
                return Ok(());
            }
            KbCommand::Surrender => {
                let _ = bot.delete_message(msg.chat.id, view.ctrl_msg_id).await;
                dialogue.update(State::Start).await?;
                crate::content::battle::surrender_battle(
                    &app_state.session_manager,
                    &bot,
                    session.user_id,
                )
                .await?;
                return Ok(());
            }
            _ => {}
        }
    }

    let _ = bot.delete_message(msg.chat.id, view.ctrl_msg_id).await;
    let _ = bot.delete_message(msg.chat.id, msg.id).await;

    let in_battle =
        crate::content::battle::is_in_active_battle(&app_state.session_manager, session.user_id)
            .await;
    let kb = if in_battle {
        battle_active_keyboard()
    } else {
        main_menu_keyboard()
    };

    let _ = bot
        .send_message(
            msg.chat.id,
            if in_battle {
                "Возврат к дуэли"
            } else {
                "Главное меню"
            },
        )
        .reply_markup(kb)
        .await;
    dialogue.update(State::Start).await?;
    Ok(())
}
