use std::time::Duration;

use teloxide::prelude::*;
use teloxide::types::{ChatId, InlineKeyboardMarkup, ParseMode};

use super::types::{
    BattleMode, BattleOutcome, BattleSession, BattleStatus, MatchmakingTicket, PlayerBattleState,
    QueueResult, WinReason, tier_to_score,
};
use crate::content::rarity::Rarity;
use crate::controls::battle_active_keyboard;
use crate::session::SessionManager;
use crate::utils::short_id::ShortId;
use crate::utils::unix_timestamp;

pub async fn find_active_battle(
    session_mgr: &SessionManager,
    user_id: u64,
) -> Option<(ShortId, BattleSession)> {
    let battles = session_mgr
        .find_ephemeral_by_user_typed::<BattleSession>(user_id)
        .await;

    battles.into_iter().find(|(_, b)| {
        matches!(
            b.status,
            BattleStatus::Countdown { .. } | BattleStatus::Active { .. }
        )
    })
}

pub async fn find_waiting_duel(
    session_mgr: &SessionManager,
    user_id: u64,
) -> Option<(ShortId, BattleSession)> {
    let battles = session_mgr
        .find_ephemeral_by_user_typed::<BattleSession>(user_id)
        .await;

    battles.into_iter().find(|(_, b)| {
        matches!(b.status, BattleStatus::WaitingForOpponent) && b.player_a.user_id == user_id
    })
}

pub async fn is_in_active_battle(session_mgr: &SessionManager, user_id: u64) -> bool {
    find_active_battle(session_mgr, user_id).await.is_some()
}

pub async fn queue_random_match(
    session_mgr: &SessionManager,
    bot: &Bot,
    user_id: u64,
    chat_id: i64,
    username: Option<String>,
) -> anyhow::Result<QueueResult> {
    if is_in_active_battle(session_mgr, user_id).await {
        return Ok(QueueResult::AlreadyInBattle);
    }

    cancel_waiting(session_mgr, bot, user_id).await;

    let now = unix_timestamp();
    let all_tickets = session_mgr.all_ephemeral_typed::<MatchmakingTicket>().await;

    let opponent_ticket = all_tickets
        .into_iter()
        .find(|(_, t)| t.user_id != user_id && (now - t.created_at_ms) < 300_000);

    if let Some((opp_id, ticket)) = opponent_ticket {
        session_mgr.remove_ephemeral(opp_id).await;

        let battle_id = ShortId::now_v7();
        let mode = BattleMode::random();
        let battle_session = BattleSession {
            id: battle_id,
            mode,
            status: BattleStatus::Countdown {
                starts_at_epoch_ms: now + 2500,
            },
            created_at_ms: now,
            player_a: PlayerBattleState::new(ticket.user_id, ticket.chat_id, ticket.username),
            player_b: Some(PlayerBattleState::new(user_id, chat_id, username)),
            inline_message_id: None,
        };

        session_mgr
            .set_ephemeral(battle_id, battle_session, [ticket.user_id, user_id])
            .await;

        start_battle_lifecycle(bot.clone(), session_mgr.clone(), battle_id);
        Ok(QueueResult::Matched)
    } else {
        let ticket_id = ShortId::now_v7();
        let ticket = MatchmakingTicket {
            id: ticket_id,
            user_id,
            chat_id,
            username,
            mode: BattleMode::random(),
            created_at_ms: now,
        };

        session_mgr
            .set_ephemeral(ticket_id, ticket, [user_id])
            .await;

        Ok(QueueResult::Queued)
    }
}

pub async fn create_invite_duel(
    session_mgr: &SessionManager,
    bot: &Bot,
    user_id: u64,
    chat_id: i64,
    username: Option<String>,
) -> anyhow::Result<(ShortId, String)> {
    if is_in_active_battle(session_mgr, user_id).await {
        anyhow::bail!("Вы уже находитесь в активной дуэли!");
    }

    cancel_waiting(session_mgr, bot, user_id).await;

    let battle_id = ShortId::now_v7();
    let now = unix_timestamp();
    let battle_session = BattleSession {
        id: battle_id,
        mode: BattleMode::random(),
        status: BattleStatus::WaitingForOpponent,
        created_at_ms: now,
        player_a: PlayerBattleState::new(user_id, chat_id, username),
        player_b: None,
        inline_message_id: None,
    };

    session_mgr
        .set_ephemeral(battle_id, battle_session, [user_id])
        .await;

    let me = bot.get_me().await?;
    let bot_username = me.username();
    let invite_link = format!("https://t.me/{}?start=battle_{}", bot_username, battle_id);

    Ok((battle_id, invite_link))
}

pub async fn join_invite_duel(
    session_mgr: &SessionManager,
    bot: &Bot,
    user_id: u64,
    chat_id: i64,
    username: Option<String>,
    battle_id: ShortId,
) -> anyhow::Result<()> {
    join_invite_duel_with_inline(
        session_mgr,
        bot,
        user_id,
        chat_id,
        username,
        battle_id,
        None,
    )
    .await
}

pub async fn join_invite_duel_with_inline(
    session_mgr: &SessionManager,
    bot: &Bot,
    user_id: u64,
    chat_id: i64,
    username: Option<String>,
    battle_id: ShortId,
    inline_message_id: Option<String>,
) -> anyhow::Result<()> {
    let mut battle = session_mgr
        .get_ephemeral::<BattleSession>(battle_id)
        .await
        .ok_or_else(|| anyhow::anyhow!("Дуэль не найдена или срок действия ссылки истек"))?;

    if battle.status != BattleStatus::WaitingForOpponent {
        anyhow::bail!("Дуэль уже началась или завершена");
    }

    if battle.player_a.user_id == user_id {
        anyhow::bail!("Нельзя вступить в собственную дуэль");
    }

    if is_in_active_battle(session_mgr, user_id).await {
        anyhow::bail!("Вы уже участвуете в другой дуэли");
    }

    cancel_waiting(session_mgr, bot, user_id).await;

    let now = unix_timestamp();
    let player_a_id = battle.player_a.user_id;

    battle.player_b = Some(PlayerBattleState::new(user_id, chat_id, username));
    battle.status = BattleStatus::Countdown {
        starts_at_epoch_ms: now + 2500,
    };
    if inline_message_id.is_some() {
        battle.inline_message_id = inline_message_id;
    }

    session_mgr
        .set_ephemeral(battle_id, battle, [player_a_id, user_id])
        .await;

    start_battle_lifecycle(bot.clone(), session_mgr.clone(), battle_id);
    Ok(())
}

pub async fn cancel_waiting(session_mgr: &SessionManager, bot: &Bot, user_id: u64) -> bool {
    let mut cancelled = false;
    let tickets = session_mgr
        .find_ephemeral_by_user_typed::<MatchmakingTicket>(user_id)
        .await;
    for (tid, _) in tickets {
        session_mgr.remove_ephemeral(tid).await;
        cancelled = true;
    }
    if let Some((bid, battle)) = find_waiting_duel(session_mgr, user_id).await {
        // If an inline invitation message is waiting in a chat, edit it and remove the button
        if let Some(ref inline_msg_id) = battle.inline_message_id {
            let host_label = battle.player_a.display_name();
            let text = format!(
                "<b>Вызов на дуэль отменен</b>\n\nОрганизатор {host_label} отменил поиск соперника"
            );
            let _ = bot
                .edit_message_text_inline(inline_msg_id.clone(), text)
                .parse_mode(ParseMode::Html)
                .reply_markup(InlineKeyboardMarkup::default())
                .await;
        }
        session_mgr.remove_ephemeral(bid).await;
        cancelled = true;
    }
    cancelled
}

pub async fn surrender_battle(
    session_mgr: &SessionManager,
    bot: &Bot,
    user_id: u64,
) -> anyhow::Result<bool> {
    let Some((battle_id, battle)) = find_active_battle(session_mgr, user_id).await else {
        return Ok(false);
    };

    let (winner_id, loser_id) = if battle.player_a.user_id == user_id {
        (
            battle.player_b.as_ref().map(|p| p.user_id).unwrap_or(0),
            user_id,
        )
    } else {
        (battle.player_a.user_id, user_id)
    };

    finalize_battle(
        bot,
        session_mgr,
        battle_id,
        BattleOutcome::Winner {
            winner_id,
            loser_id,
            reason: WinReason::OpponentSurrendered,
        },
    )
    .await?;

    Ok(true)
}

pub async fn on_battle_roll(
    session_mgr: &SessionManager,
    bot: &Bot,
    user_id: u64,
    rarity: &dyn Rarity,
) -> anyhow::Result<Option<BattleOutcome>> {
    let Some((battle_id, _)) = find_active_battle(session_mgr, user_id).await else {
        return Ok(None);
    };

    let mut finish_outcome: Option<BattleOutcome> = None;
    let now = unix_timestamp();
    let roll_tier = rarity.tier();
    let rarity_name = rarity.name().to_string();
    let rarity_id = rarity.id().to_string();
    let gained_score = tier_to_score(roll_tier);

    session_mgr
        .mutate_ephemeral::<BattleSession, _>(battle_id, |battle| {
            if !matches!(battle.status, BattleStatus::Active { .. }) {
                return;
            }

            let is_player_a = battle.player_a.user_id == user_id;
            let (player, opponent_id) = if is_player_a {
                (
                    &mut battle.player_a,
                    battle.player_b.as_ref().map(|p| p.user_id).unwrap_or(0),
                )
            } else if let Some(ref mut p_b) = battle.player_b {
                (p_b, battle.player_a.user_id)
            } else {
                return;
            };

            player.rolls_count += 1;
            player.score += gained_score;
            player.last_rarity_name = rarity_name.clone();
            player.last_tier = roll_tier;

            if roll_tier > player.best_tier || player.best_tier < -900.0 {
                player.best_tier = roll_tier;
                player.best_rarity_id = rarity_id.clone();
                player.best_rarity_name = rarity_name.clone();
                player.best_tier_timestamp_ms = now;
            }

            match &battle.mode {
                BattleMode::TargetRarity { target_tier, .. } => {
                    if roll_tier >= *target_tier {
                        finish_outcome = Some(BattleOutcome::Winner {
                            winner_id: user_id,
                            loser_id: opponent_id,
                            reason: WinReason::TargetReached {
                                rarity_name: rarity_name.clone(),
                            },
                        });
                    }
                }
                BattleMode::ThresholdRace { target_score, .. } => {
                    if player.score >= *target_score {
                        finish_outcome = Some(BattleOutcome::Winner {
                            winner_id: user_id,
                            loser_id: opponent_id,
                            reason: WinReason::ScoreReached {
                                score: player.score,
                            },
                        });
                    }
                }
                BattleMode::TimeAttack { .. } | BattleMode::ScoreAttack { .. } => {}
            }
        })
        .await;

    if let Some(outcome) = finish_outcome {
        finalize_battle(bot, session_mgr, battle_id, outcome.clone()).await?;
        return Ok(Some(outcome));
    }

    Ok(None)
}

pub fn start_battle_lifecycle(bot: Bot, session_mgr: SessionManager, battle_id: ShortId) {
    tokio::spawn(async move {
        let Some(battle) = session_mgr.get_ephemeral::<BattleSession>(battle_id).await else {
            return;
        };

        let p_a = &battle.player_a;
        let p_b = match battle.player_b.as_ref() {
            Some(p) => p,
            None => return,
        };

        let chat_id_a = p_a.chat_id;
        let chat_id_b = p_b.chat_id;

        let p_a_label = p_a.display_name();
        let p_b_label = p_b.display_name();

        let mode_title = battle.mode.title();
        let condition_desc = battle.mode.description();

        let countdown_for_a = format!(
            "<b>ДУЭЛЬ НАЗНАЧЕНА</b>\n\n<b>Соперник:</b> {p_b_label}\n<b>Режим:</b> {mode_title}\n{condition_desc}\n\n<b>Приготовьтесь: 3... 2... 1...</b>"
        );

        let countdown_for_b = format!(
            "<b>ДУЭЛЬ НАЗНАЧЕНА</b>\n\n<b>Соперник:</b> {p_a_label}\n<b>Режим:</b> {mode_title}\n{condition_desc}\n\n<b>Приготовьтесь: 3... 2... 1...</b>"
        );

        let _ = bot
            .send_message(ChatId(chat_id_a), &countdown_for_a)
            .parse_mode(ParseMode::Html)
            .reply_markup(battle_active_keyboard())
            .await;

        let _ = bot
            .send_message(ChatId(chat_id_b), &countdown_for_b)
            .parse_mode(ParseMode::Html)
            .reply_markup(battle_active_keyboard())
            .await;

        if let Some(ref inline_msg_id) = battle.inline_message_id {
            let inline_start = format!(
                "<b>ДУЭЛЬ НАЧАЛАСЬ</b>\n\n{p_a_label} <b>VS</b> {p_b_label}\n<b>Режим:</b> {mode_title}\n\n<i>Битва в процессе...</i>"
            );
            let _ = bot
                .edit_message_text_inline(inline_msg_id.clone(), inline_start)
                .parse_mode(ParseMode::Html)
                .reply_markup(InlineKeyboardMarkup::default())
                .await;
        }

        tokio::time::sleep(Duration::from_millis(2500)).await;

        let now = unix_timestamp();
        let duration_ms = battle.mode.max_duration_secs() * 1000;
        let expires_at = now + duration_ms as i64;

        let is_valid = session_mgr
            .mutate_ephemeral::<BattleSession, _>(battle_id, |b| {
                if matches!(b.status, BattleStatus::Countdown { .. }) {
                    b.status = BattleStatus::Active {
                        started_at_epoch_ms: now,
                        expires_at_epoch_ms: expires_at,
                    };
                    true
                } else {
                    false
                }
            })
            .await
            .unwrap_or(false);

        if !is_valid {
            return;
        }

        let start_msg_for_a = format!("<b>СТАРТ!</b>\nПротивник: <b>{p_b_label}</b>");
        let start_msg_for_b = format!("<b>СТАРТ!</b>\nПротивник: <b>{p_a_label}</b>");

        let _ = bot
            .send_message(ChatId(chat_id_a), start_msg_for_a)
            .parse_mode(ParseMode::Html)
            .reply_markup(battle_active_keyboard())
            .await;
        let _ = bot
            .send_message(ChatId(chat_id_b), start_msg_for_b)
            .parse_mode(ParseMode::Html)
            .reply_markup(battle_active_keyboard())
            .await;

        loop {
            tokio::time::sleep(Duration::from_millis(250)).await;
            let current_now = unix_timestamp();
            let b = match session_mgr.get_ephemeral::<BattleSession>(battle_id).await {
                Some(b) => b,
                None => break,
            };

            match b.status {
                BattleStatus::Active {
                    expires_at_epoch_ms,
                    ..
                } => {
                    if current_now >= expires_at_epoch_ms {
                        let outcome = resolve_timeout_winner(&b);
                        let _ = finalize_battle(&bot, &session_mgr, battle_id, outcome).await;
                        break;
                    }
                }
                _ => break,
            }
        }
    });
}

fn resolve_timeout_winner(battle: &BattleSession) -> BattleOutcome {
    let p_a = &battle.player_a;
    let p_b = match &battle.player_b {
        Some(p) => p,
        None => return BattleOutcome::Cancelled,
    };

    if p_a.surrendered {
        return BattleOutcome::Winner {
            winner_id: p_b.user_id,
            loser_id: p_a.user_id,
            reason: WinReason::OpponentSurrendered,
        };
    }
    if p_b.surrendered {
        return BattleOutcome::Winner {
            winner_id: p_a.user_id,
            loser_id: p_b.user_id,
            reason: WinReason::OpponentSurrendered,
        };
    }

    if p_a.rolls_count == 0 && p_b.rolls_count == 0 {
        return BattleOutcome::Draw;
    }
    if p_a.rolls_count == 0 {
        return BattleOutcome::Winner {
            winner_id: p_b.user_id,
            loser_id: p_a.user_id,
            reason: WinReason::OpponentTimedOut,
        };
    }
    if p_b.rolls_count == 0 {
        return BattleOutcome::Winner {
            winner_id: p_a.user_id,
            loser_id: p_b.user_id,
            reason: WinReason::OpponentTimedOut,
        };
    }

    match battle.mode {
        BattleMode::ScoreAttack { .. } | BattleMode::ThresholdRace { .. } => {
            if (p_a.score - p_b.score).abs() > f32::EPSILON {
                if p_a.score > p_b.score {
                    BattleOutcome::Winner {
                        winner_id: p_a.user_id,
                        loser_id: p_b.user_id,
                        reason: WinReason::HighestScoreInTime {
                            winning_score: p_a.score,
                            loser_score: p_b.score,
                        },
                    }
                } else {
                    BattleOutcome::Winner {
                        winner_id: p_b.user_id,
                        loser_id: p_a.user_id,
                        reason: WinReason::HighestScoreInTime {
                            winning_score: p_b.score,
                            loser_score: p_a.score,
                        },
                    }
                }
            } else if p_a.best_tier > p_b.best_tier {
                BattleOutcome::Winner {
                    winner_id: p_a.user_id,
                    loser_id: p_b.user_id,
                    reason: WinReason::HighestTierInTime {
                        winning_tier: p_a.best_tier,
                        loser_tier: p_b.best_tier,
                        winning_rarity: p_a.best_rarity_name.clone(),
                        loser_rarity: p_b.best_rarity_name.clone(),
                    },
                }
            } else if p_b.best_tier > p_a.best_tier {
                BattleOutcome::Winner {
                    winner_id: p_b.user_id,
                    loser_id: p_a.user_id,
                    reason: WinReason::HighestTierInTime {
                        winning_tier: p_b.best_tier,
                        loser_tier: p_a.best_tier,
                        winning_rarity: p_b.best_rarity_name.clone(),
                        loser_rarity: p_a.best_rarity_name.clone(),
                    },
                }
            } else {
                BattleOutcome::Draw
            }
        }
        BattleMode::TargetRarity { .. } | BattleMode::TimeAttack { .. } => {
            if p_a.best_tier > p_b.best_tier {
                BattleOutcome::Winner {
                    winner_id: p_a.user_id,
                    loser_id: p_b.user_id,
                    reason: WinReason::HighestTierInTime {
                        winning_tier: p_a.best_tier,
                        loser_tier: p_b.best_tier,
                        winning_rarity: p_a.best_rarity_name.clone(),
                        loser_rarity: p_b.best_rarity_name.clone(),
                    },
                }
            } else if p_b.best_tier > p_a.best_tier {
                BattleOutcome::Winner {
                    winner_id: p_b.user_id,
                    loser_id: p_a.user_id,
                    reason: WinReason::HighestTierInTime {
                        winning_tier: p_b.best_tier,
                        loser_tier: p_a.best_tier,
                        winning_rarity: p_b.best_rarity_name.clone(),
                        loser_rarity: p_a.best_rarity_name.clone(),
                    },
                }
            } else if p_a.best_tier_timestamp_ms > 0
                && p_b.best_tier_timestamp_ms > 0
                && p_a.best_tier_timestamp_ms != p_b.best_tier_timestamp_ms
            {
                if p_a.best_tier_timestamp_ms < p_b.best_tier_timestamp_ms {
                    BattleOutcome::Winner {
                        winner_id: p_a.user_id,
                        loser_id: p_b.user_id,
                        reason: WinReason::TieBrokenByTime {
                            diff_ms: p_b.best_tier_timestamp_ms - p_a.best_tier_timestamp_ms,
                            winning_rarity: p_a.best_rarity_name.clone(),
                        },
                    }
                } else {
                    BattleOutcome::Winner {
                        winner_id: p_b.user_id,
                        loser_id: p_a.user_id,
                        reason: WinReason::TieBrokenByTime {
                            diff_ms: p_a.best_tier_timestamp_ms - p_b.best_tier_timestamp_ms,
                            winning_rarity: p_b.best_rarity_name.clone(),
                        },
                    }
                }
            } else {
                BattleOutcome::Draw
            }
        }
    }
}

fn format_player_summary(
    me: &PlayerBattleState,
    opp: &PlayerBattleState,
    outcome: &BattleOutcome,
    duration_secs: f64,
    mode: &BattleMode,
) -> String {
    let opp_label = opp.display_name();
    let is_score_mode = mode.is_score_mode();

    let (header, status_text, reason_text, winning_cat_highlight) = match outcome {
        BattleOutcome::Winner {
            winner_id,
            loser_id: _,
            reason,
        } => {
            let won = *winner_id == me.user_id;
            let header = if won {
                "<b>ПОБЕДА!</b>"
            } else {
                "<b>ПОРАЖЕНИЕ</b>"
            };

            let status = if won {
                format!("Вы одержали победу над <b>{opp_label}</b>!")
            } else {
                format!("Победитель дуэли: <b>{opp_label}</b>")
            };

            let reason_desc = match reason {
                WinReason::TargetReached { rarity_name } => {
                    if won {
                        format!("Вы первым получили кота требуемой редкости: <b>{rarity_name}</b>!")
                    } else {
                        format!(
                            "Соперник <b>{opp_label}</b> первым получил кота редкости <b>{rarity_name}</b>!"
                        )
                    }
                }
                WinReason::HighestTierInTime {
                    winning_tier,
                    loser_tier,
                    winning_rarity,
                    loser_rarity,
                } => {
                    if won {
                        format!(
                            "Ваш кот <b>{winning_rarity}</b> ({winning_tier:+.2}) превзошел кота соперника <b>{loser_rarity}</b> ({loser_tier:+.2})!"
                        )
                    } else {
                        format!(
                            "Кот соперника <b>{winning_rarity}</b> ({winning_tier:+.2}) превзошел вашего кота <b>{loser_rarity}</b> ({loser_tier:+.2})!"
                        )
                    }
                }
                WinReason::TieBrokenByTime {
                    diff_ms,
                    winning_rarity,
                } => {
                    let diff_s = (*diff_ms as f64) / 1000.0;
                    if won {
                        format!(
                            "Равный ранг котов (<b>{winning_rarity}</b>), но вы нашли своего на <b>{diff_s:.2} с</b> быстрее!"
                        )
                    } else {
                        format!(
                            "Равный ранг котов (<b>{winning_rarity}</b>), но <b>{opp_label}</b> нашел своего на <b>{diff_s:.2} с</b> быстрее!"
                        )
                    }
                }
                WinReason::ScoreReached { score } => {
                    if won {
                        format!("Вы первым достигли целевой отметки в <b>{score:.0}</b> очков!")
                    } else {
                        format!(
                            "Соперник <b>{opp_label}</b> первым набрал <b>{score:.0}</b> очков!"
                        )
                    }
                }
                WinReason::HighestScoreInTime {
                    winning_score,
                    loser_score,
                } => {
                    if won {
                        format!(
                            "Вы набрали больше очков за отведенное время ({winning_score:.0} против {loser_score:.0})!"
                        )
                    } else {
                        format!(
                            "Соперник <b>{opp_label}</b> набрал больше очков ({winning_score:.0} против {loser_score:.0})!"
                        )
                    }
                }
                WinReason::OpponentSurrendered => {
                    if won {
                        format!("Соперник <b>{opp_label}</b> сдался")
                    } else {
                        "Вы сдались".to_string()
                    }
                }
                WinReason::OpponentTimedOut => {
                    if won {
                        format!("Соперник <b>{opp_label}</b> не совершил ни одного броска")
                    } else {
                        "Время вышло: вы не сделали ни одного броска".to_string()
                    }
                }
                WinReason::BothZeroRolls => "Ни один из участников не совершил бросков".to_string(),
            };

            let highlight = if won {
                format!("• Ваш победный кот: <b>{}</b>", me.best_rarity_name)
            } else {
                format!(
                    "• Победный кот соперника: <b>{}</b> (последний: <b>{}</b>)",
                    opp.best_rarity_name, opp.last_rarity_name
                )
            };

            (header.to_string(), status, reason_desc, highlight)
        }
        BattleOutcome::Draw => (
            "<b>НИЧЬЯ</b>".to_string(),
            format!("Поединок против <b>{opp_label}</b> завершился с равным счетом"),
            "Ни один из игроков не получил преимущества".to_string(),
            String::new(),
        ),
        BattleOutcome::Cancelled => (
            "<b>ДУЭЛЬ ОТМЕНЕНА</b>".to_string(),
            format!("Поединок против <b>{opp_label}</b> был прерван"),
            "Матч завершен досрочно".to_string(),
            String::new(),
        ),
    };

    let highlight_section = if winning_cat_highlight.is_empty() {
        String::new()
    } else {
        format!("\n{winning_cat_highlight}\n")
    };

    let stats_section = if is_score_mode {
        format!(
            "<b>Результаты:</b>\n├ <b>Вы:</b> {} бросков | Очки: <b>{:.0}</b> | Лучший: <b>{}</b>\n└ <b>{opp_label}:</b> {} бросков | Очки: <b>{:.0}</b> | Лучший: <b>{}</b>",
            me.rolls_count,
            me.score,
            me.best_rarity_name,
            opp.rolls_count,
            opp.score,
            opp.best_rarity_name
        )
    } else {
        format!(
            "<b>Результаты:</b>\n├ <b>Вы:</b> {} бросков | Лучший: <b>{}</b>\n└ <b>{opp_label}:</b> {} бросков | Лучший: <b>{}</b>",
            me.rolls_count, me.best_rarity_name, opp.rolls_count, opp.best_rarity_name
        )
    };

    format!(
        "<b>ДУЭЛЬ ЗАВЕРШЕНА</b>\n\n{}\n{}\n• <i>{}</i>{}\n{}\n\n<b>Чистое время:</b> {:.1} с",
        header, status_text, reason_text, highlight_section, stats_section, duration_secs
    )
}

pub async fn finalize_battle(
    bot: &Bot,
    session_mgr: &SessionManager,
    battle_id: ShortId,
    outcome: BattleOutcome,
) -> anyhow::Result<()> {
    let battle = match session_mgr.get_ephemeral::<BattleSession>(battle_id).await {
        Some(b) => b,
        None => return Ok(()),
    };

    session_mgr.remove_ephemeral(battle_id).await;

    let p_a = &battle.player_a;
    let p_b = match &battle.player_b {
        Some(p) => p,
        None => return Ok(()),
    };

    match &outcome {
        BattleOutcome::Winner {
            winner_id,
            loser_id,
            ..
        } => {
            let _ = session_mgr
                .mutate_session(*winner_id, |s| s.stats.battles_won += 1)
                .await;
            let _ = session_mgr
                .mutate_session(*loser_id, |s| s.stats.battles_lost += 1)
                .await;
        }
        BattleOutcome::Draw => {
            let _ = session_mgr
                .mutate_session(p_a.user_id, |s| s.stats.battles_draw += 1)
                .await;
            let _ = session_mgr
                .mutate_session(p_b.user_id, |s| s.stats.battles_draw += 1)
                .await;
        }
        BattleOutcome::Cancelled => {}
    }

    let now = unix_timestamp();
    let started_at = match battle.status {
        BattleStatus::Active {
            started_at_epoch_ms,
            ..
        } => started_at_epoch_ms,
        BattleStatus::Countdown { starts_at_epoch_ms } => starts_at_epoch_ms,
        _ => battle.created_at_ms,
    };
    let duration_secs = ((now - started_at).max(0) as f64) / 1000.0;

    let summary_a = format_player_summary(p_a, p_b, &outcome, duration_secs, &battle.mode);
    let summary_b = format_player_summary(p_b, p_a, &outcome, duration_secs, &battle.mode);

    let _ = bot
        .send_message(ChatId(p_a.chat_id), &summary_a)
        .parse_mode(ParseMode::Html)
        .reply_markup(crate::controls::main_menu_keyboard())
        .await;

    let _ = bot
        .send_message(ChatId(p_b.chat_id), &summary_b)
        .parse_mode(ParseMode::Html)
        .reply_markup(crate::controls::main_menu_keyboard())
        .await;

    if let Some(ref inline_msg_id) = battle.inline_message_id {
        let p_a_label = p_a.display_name();
        let p_b_label = p_b.display_name();

        let winner_text = match &outcome {
            BattleOutcome::Winner { winner_id, .. } => {
                if *winner_id == p_a.user_id {
                    format!("Победитель: <b>{p_a_label}</b>")
                } else {
                    format!("Победитель: <b>{p_b_label}</b>")
                }
            }
            BattleOutcome::Draw => "<b>Ничья!</b>".to_string(),
            BattleOutcome::Cancelled => "<b>Дуэль отменена</b>".to_string(),
        };

        let inline_final = format!(
            "<b>ДУЭЛЬ ЗАВЕРШЕНА</b>\n\n{p_a_label} <b>VS</b> {p_b_label}\n\n{winner_text}\n\n<b>Счет:</b>\n• {p_a_label}: {} бросков (лучший: {})\n• {p_b_label}: {} бросков (лучший: {})\n\n<b>Время:</b> {:.1} с",
            p_a.rolls_count,
            p_a.best_rarity_name,
            p_b.rolls_count,
            p_b.best_rarity_name,
            duration_secs
        );

        let _ = bot
            .edit_message_text_inline(inline_msg_id.clone(), inline_final)
            .parse_mode(ParseMode::Html)
            .reply_markup(InlineKeyboardMarkup::default())
            .await;
    }

    Ok(())
}

pub async fn is_in_queue_or_waiting(session_mgr: &SessionManager, user_id: u64) -> bool {
    if !session_mgr
        .find_ephemeral_by_user_typed::<MatchmakingTicket>(user_id)
        .await
        .is_empty()
    {
        return true;
    }
    find_waiting_duel(session_mgr, user_id).await.is_some()
}

pub async fn create_invite_duel_with_id(
    session_mgr: &SessionManager,
    bot: &Bot,
    user_id: u64,
    chat_id: i64,
    username: Option<String>,
    battle_id: ShortId,
) -> anyhow::Result<ShortId> {
    if is_in_active_battle(session_mgr, user_id).await {
        anyhow::bail!("Вы уже находитесь в активной дуэли!");
    }

    cancel_waiting(session_mgr, bot, user_id).await;

    let now = unix_timestamp();
    let battle_session = BattleSession {
        id: battle_id,
        mode: BattleMode::random(),
        status: BattleStatus::WaitingForOpponent,
        created_at_ms: now,
        player_a: PlayerBattleState::new(user_id, chat_id, username),
        player_b: None,
        inline_message_id: None,
    };

    session_mgr
        .set_ephemeral(battle_id, battle_session, [user_id])
        .await;

    Ok(battle_id)
}
