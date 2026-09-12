use std::time::Duration;

use teloxide::prelude::*;
use teloxide::types::{ChatId, ParseMode};

use super::types::{
    BattleMode, BattleOutcome, BattleSession, BattleStatus, MatchmakingTicket, PlayerBattleState,
    QueueResult, WinReason,
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
) -> anyhow::Result<QueueResult> {
    if is_in_active_battle(session_mgr, user_id).await {
        return Ok(QueueResult::AlreadyInBattle);
    }

    cancel_waiting(session_mgr, user_id).await;

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
            player_a: PlayerBattleState::new(ticket.user_id, ticket.chat_id),
            player_b: Some(PlayerBattleState::new(user_id, chat_id)),
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
) -> anyhow::Result<(ShortId, String)> {
    if is_in_active_battle(session_mgr, user_id).await {
        anyhow::bail!("Вы уже находитесь в активной дуэли!");
    }

    cancel_waiting(session_mgr, user_id).await;

    let battle_id = ShortId::now_v7();
    let now = unix_timestamp();
    let battle_session = BattleSession {
        id: battle_id,
        mode: BattleMode::random(),
        status: BattleStatus::WaitingForOpponent,
        created_at_ms: now,
        player_a: PlayerBattleState::new(user_id, chat_id),
        player_b: None,
    };

    session_mgr
        .set_ephemeral(battle_id, battle_session, [user_id])
        .await;

    let me = bot.get_me().await?;
    let username = me.username();
    let invite_link = format!("https://t.me/{}?start=battle_{}", username, battle_id);

    Ok((battle_id, invite_link))
}

pub async fn join_invite_duel(
    session_mgr: &SessionManager,
    bot: &Bot,
    user_id: u64,
    chat_id: i64,
    battle_id: ShortId,
) -> anyhow::Result<()> {
    let mut battle = session_mgr
        .get_ephemeral::<BattleSession>(battle_id)
        .await
        .ok_or_else(|| anyhow::anyhow!("Дуэль не найдена или срок действия ссылки истёк"))?;

    if battle.status != BattleStatus::WaitingForOpponent {
        anyhow::bail!("Дуэль уже началась или завершена");
    }

    if battle.player_a.user_id == user_id {
        anyhow::bail!("Вы не можете вступить в свою собственную дуэль");
    }

    if is_in_active_battle(session_mgr, user_id).await {
        anyhow::bail!("Вы уже находитесь в другой активной дуэли");
    }

    cancel_waiting(session_mgr, user_id).await;

    let now = unix_timestamp();
    let player_a_id = battle.player_a.user_id;

    battle.player_b = Some(PlayerBattleState::new(user_id, chat_id));
    battle.status = BattleStatus::Countdown {
        starts_at_epoch_ms: now + 2500,
    };

    session_mgr
        .set_ephemeral(battle_id, battle, [player_a_id, user_id])
        .await;

    start_battle_lifecycle(bot.clone(), session_mgr.clone(), battle_id);
    Ok(())
}

pub async fn cancel_waiting(session_mgr: &SessionManager, user_id: u64) -> bool {
    let mut cancelled = false;
    let tickets = session_mgr
        .find_ephemeral_by_user_typed::<MatchmakingTicket>(user_id)
        .await;
    for (tid, _) in tickets {
        session_mgr.remove_ephemeral(tid).await;
        cancelled = true;
    }
    if let Some((bid, _)) = find_waiting_duel(session_mgr, user_id).await {
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
    let rarity_name = rarity.name();
    let rarity_id = rarity.id();

    session_mgr
        .mutate_ephemeral::<BattleSession, _>(battle_id, |battle| {
            if !matches!(battle.status, BattleStatus::Active { .. }) {
                return;
            }

            let is_player_a = battle.player_a.user_id == user_id;
            let (player, opp_rolls, opp_best_tier, opponent_id) = if is_player_a {
                (
                    &mut battle.player_a,
                    battle.player_b.as_ref().map(|p| p.rolls_count).unwrap_or(0),
                    battle
                        .player_b
                        .as_ref()
                        .map(|p| p.best_tier)
                        .unwrap_or(-999.0),
                    battle.player_b.as_ref().map(|p| p.user_id).unwrap_or(0),
                )
            } else if let Some(ref mut p_b) = battle.player_b {
                (
                    p_b,
                    battle.player_a.rolls_count,
                    battle.player_a.best_tier,
                    battle.player_a.user_id,
                )
            } else {
                return;
            };

            player.rolls_count += 1;
            if roll_tier > player.best_tier || player.best_tier < -900.0 {
                player.best_tier = roll_tier;
                player.best_rarity_id = rarity_id.clone();
                player.best_rarity_name = rarity_name.clone();
                player.best_tier_timestamp_ms = now;
            }

            match &battle.mode {
                // Target Rarity mode: instant win on >= target_tier
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
                // Burst Sprint mode: check if both completed their roll allocation
                BattleMode::BurstSprint { max_rolls, .. } => {
                    if player.rolls_count >= *max_rolls && opp_rolls >= *max_rolls {
                        if player.best_tier > opp_best_tier {
                            finish_outcome = Some(BattleOutcome::Winner {
                                winner_id: user_id,
                                loser_id: opponent_id,
                                reason: WinReason::BurstLimitCompleted,
                            });
                        } else if opp_best_tier > player.best_tier {
                            finish_outcome = Some(BattleOutcome::Winner {
                                winner_id: opponent_id,
                                loser_id: user_id,
                                reason: WinReason::BurstLimitCompleted,
                            });
                        } else {
                            finish_outcome = Some(BattleOutcome::Draw);
                        }
                    }
                }
                BattleMode::TimeAttack { .. } => {}
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

        let chat_id_a = battle.player_a.chat_id;
        let chat_id_b = match battle.player_b.as_ref() {
            Some(p) => p.chat_id,
            None => return,
        };

        let condition_desc = battle.mode.description();
        let countdown_msg = format!(
            "⚔️ <b>ДУЭЛЬ НАЗНАЧЕНА!</b>\n\n{}\n\n⏳ <b>Приготовьтесь: 3... 2... 1...</b>",
            condition_desc
        );

        let _ = bot
            .send_message(ChatId(chat_id_a), &countdown_msg)
            .parse_mode(ParseMode::Html)
            .reply_markup(battle_active_keyboard())
            .await;

        let _ = bot
            .send_message(ChatId(chat_id_b), &countdown_msg)
            .parse_mode(ParseMode::Html)
            .reply_markup(battle_active_keyboard())
            .await;

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

        let start_msg = "🔥 <b>СТАРТ! Жмите «Получить» на полную мощность!</b>";
        let _ = bot
            .send_message(ChatId(chat_id_a), start_msg)
            .parse_mode(ParseMode::Html)
            .reply_markup(battle_active_keyboard())
            .await;
        let _ = bot
            .send_message(ChatId(chat_id_b), start_msg)
            .parse_mode(ParseMode::Html)
            .reply_markup(battle_active_keyboard())
            .await;

        loop {
            tokio::time::sleep(Duration::from_millis(300)).await;
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

    if p_a.best_tier > p_b.best_tier {
        BattleOutcome::Winner {
            winner_id: p_a.user_id,
            loser_id: p_b.user_id,
            reason: WinReason::HighestTierInTime {
                winning_tier: p_a.best_tier,
                loser_tier: p_b.best_tier,
            },
        }
    } else if p_b.best_tier > p_a.best_tier {
        BattleOutcome::Winner {
            winner_id: p_b.user_id,
            loser_id: p_a.user_id,
            reason: WinReason::HighestTierInTime {
                winning_tier: p_b.best_tier,
                loser_tier: p_a.best_tier,
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
                },
            }
        } else {
            BattleOutcome::Winner {
                winner_id: p_b.user_id,
                loser_id: p_a.user_id,
                reason: WinReason::TieBrokenByTime {
                    diff_ms: p_a.best_tier_timestamp_ms - p_b.best_tier_timestamp_ms,
                },
            }
        }
    } else {
        BattleOutcome::Draw
    }
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

    let (result_header, reason_text) = match &outcome {
        BattleOutcome::Winner {
            winner_id, reason, ..
        } => {
            let winner_mention = if *winner_id == p_a.user_id {
                "Игрок 1"
            } else {
                "Игрок 2"
            };
            let desc = match reason {
                WinReason::TargetReached { rarity_name } => {
                    format!("Первым получен кот редкости <b>{rarity_name}</b>!")
                }
                WinReason::HighestTierInTime {
                    winning_tier,
                    loser_tier,
                } => {
                    format!("Высший ранг кота ({winning_tier:+.2} против {loser_tier:+.2})!")
                }
                WinReason::TieBrokenByTime { diff_ms } => {
                    let diff_s = (*diff_ms as f64) / 1000.0;
                    format!(
                        "Ничья по рангу, но победный кот найден на <b>{diff_s:.2}с</b> быстрее!"
                    )
                }
                WinReason::BurstLimitCompleted => "Победа по итогам серии бросков!".to_string(),
                WinReason::OpponentSurrendered => "Соперник сдался!".to_string(),
                WinReason::OpponentTimedOut => "Время дуэли истекло!".to_string(),
                WinReason::BothZeroRolls => "Никто не сделал ни одного броска.".to_string(),
            };
            (format!("🏆 <b>ПОБЕДИТЕЛЬ: {winner_mention}</b>"), desc)
        }
        BattleOutcome::Draw => (
            "🤝 <b>НИЧЬЯ!</b>".to_string(),
            "Игроки показали абсолютно равные результаты!".to_string(),
        ),
        BattleOutcome::Cancelled => (
            "❌ <b>ДУЭЛЬ ОТМЕНЕНА</b>".to_string(),
            "Матч был прерван.".to_string(),
        ),
    };

    let summary = format!(
        "🏁 <b>ДУЭЛЬ ЗАВЕРШЕНА</b>\n
        {}
        📌 <i>{}</i>\n
        📊 <b>Результаты дуэли:</b>
        ├ 👤 <b>Игрок 1:</b> {} бросков | Лучший: <b>{}</b>
        └ 👤 <b>Игрок 2:</b> {} бросков | Лучший: <b>{}</b>\n
        ⏱ <b>Чистое время:</b> {:.1} сек",
        result_header,
        reason_text,
        p_a.rolls_count,
        p_a.best_rarity_name,
        p_b.rolls_count,
        p_b.best_rarity_name,
        duration_secs
    );

    let _ = bot
        .send_message(ChatId(p_a.chat_id), &summary)
        .parse_mode(ParseMode::Html)
        .reply_markup(crate::controls::main_menu_keyboard())
        .await;

    let _ = bot
        .send_message(ChatId(p_b.chat_id), &summary)
        .parse_mode(ParseMode::Html)
        .reply_markup(crate::controls::main_menu_keyboard())
        .await;

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
