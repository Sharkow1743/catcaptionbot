use crate::utils::short_id::ShortId;
use rand::RngExt;
use serde::{Deserialize, Serialize};

pub fn tier_to_score(tier: f32) -> f32 {
    ((tier + 1.0) * 10.0).max(1.0).round()
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum BattleMode {
    /// First player to find a cat with tier >= target_tier
    TargetRarity {
        target_tier: f32,
        target_rarity_name: String,
        timeout_secs: u64,
    },
    /// Fast sprint: highest single rarity rolled within 15-30 seconds wins
    TimeAttack { duration_secs: u64 },
    /// Point accumulation: every rolled cat grants points based on its rarity tier
    ScoreAttack { duration_secs: u64 },
    /// Point threshold race: first player to reach target_score wins immediately
    ThresholdRace {
        target_score: f32,
        timeout_secs: u64,
    },
}

impl BattleMode {
    pub fn random() -> Self {
        let mut rng = rand::rng();
        let roll = rng.random_range(0..100);

        if roll < 30 {
            let accessible_targets = [
                ("Дворовый", -0.40),
                ("Неплохой", -0.10),
                ("Монохромный", 0.05),
                ("Ретро", 0.10),
                ("Глубоководный", 0.25),
                ("Адский", 0.30),
                ("Тепловой", 0.35),
                ("Редкий", 0.35),
                ("Кристальный", 0.40),
            ];
            let idx = rng.random_range(0..accessible_targets.len());
            let (name, tier) = accessible_targets[idx];
            let timeout_secs = rng.random_range(20..=40);

            BattleMode::TargetRarity {
                target_tier: tier,
                target_rarity_name: name.to_string(),
                timeout_secs,
            }
        } else if roll < 55 {
            let duration_secs = rng.random_range(15..=30);
            BattleMode::TimeAttack { duration_secs }
        } else if roll < 80 {
            let duration_secs = rng.random_range(20..=35);
            BattleMode::ScoreAttack { duration_secs }
        } else {
            let target_score = (rng.random_range(4..=7) * 10) as f32;
            let timeout_secs = rng.random_range(25..=45);
            BattleMode::ThresholdRace {
                target_score,
                timeout_secs,
            }
        }
    }

    pub fn title(&self) -> &'static str {
        match self {
            BattleMode::TargetRarity { .. } => "Охота за редкостью",
            BattleMode::TimeAttack { .. } => "Блиц на ранг",
            BattleMode::ScoreAttack { .. } => "Очковая лихорадка",
            BattleMode::ThresholdRace { .. } => "Гонка до порога очков",
        }
    }

    pub fn is_score_mode(&self) -> bool {
        matches!(
            self,
            BattleMode::ScoreAttack { .. } | BattleMode::ThresholdRace { .. }
        )
    }

    pub fn description(&self) -> String {
        match self {
            BattleMode::TargetRarity {
                target_rarity_name,
                timeout_secs,
                ..
            } => {
                format!(
                    "<b>Условие:</b> Лимит {timeout_secs} с\nПобеждает тот, кто <b>первым</b> выбьет кота редкости <b>{target_rarity_name}</b> или выше!"
                )
            }
            BattleMode::TimeAttack { duration_secs } => {
                format!(
                    "<b>Условие:</b> Блиц-таймер {duration_secs} с\nЖмите «Получить» на максимальной скорости! Побеждает самый редкий найденный кот к концу времени."
                )
            }
            BattleMode::ScoreAttack { duration_secs } => {
                format!(
                    "<b>Условие:</b> Лимит {duration_secs} с\nКаждый кот приносит очки в зависимости от ранга! Побеждает набравший наибольшую сумму очков."
                )
            }
            BattleMode::ThresholdRace {
                target_score,
                timeout_secs,
            } => {
                format!(
                    "<b>Условие:</b> Лимит {timeout_secs} с | Цель: {target_score:.0} очков\nПобеждает тот, кто <b>первым</b> наберет {target_score:.0} очков по сумме рангов!"
                )
            }
        }
    }

    pub fn max_duration_secs(&self) -> u64 {
        match self {
            BattleMode::TargetRarity { timeout_secs, .. } => *timeout_secs,
            BattleMode::TimeAttack { duration_secs } => *duration_secs,
            BattleMode::ScoreAttack { duration_secs } => *duration_secs,
            BattleMode::ThresholdRace { timeout_secs, .. } => *timeout_secs,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum BattleStatus {
    WaitingForOpponent,
    Countdown {
        starts_at_epoch_ms: i64,
    },
    Active {
        started_at_epoch_ms: i64,
        expires_at_epoch_ms: i64,
    },
    Finished {
        outcome: BattleOutcome,
    },
    Cancelled,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PlayerBattleState {
    pub user_id: u64,
    pub chat_id: i64,
    pub username: Option<String>,
    pub rolls_count: usize,
    pub score: f32,
    pub best_rarity_id: String,
    pub best_rarity_name: String,
    pub best_tier: f32,
    pub best_tier_timestamp_ms: i64,
    pub last_rarity_name: String,
    pub last_tier: f32,
    pub surrendered: bool,
}

impl PlayerBattleState {
    pub fn new(user_id: u64, chat_id: i64, username: Option<String>) -> Self {
        Self {
            user_id,
            chat_id,
            username,
            rolls_count: 0,
            score: 0.0,
            best_rarity_id: String::new(),
            best_rarity_name: "Обычный".to_string(),
            best_tier: -999.0,
            best_tier_timestamp_ms: 0,
            last_rarity_name: "Обычный".to_string(),
            last_tier: -999.0,
            surrendered: false,
        }
    }

    pub fn display_name(&self) -> String {
        match &self.username {
            Some(u) if !u.trim().is_empty() => {
                let clean = u.trim().trim_start_matches('@');
                format!("@{clean}")
            }
            _ => format!("Игрок {}", self.user_id),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum WinReason {
    TargetReached {
        rarity_name: String,
    },
    HighestTierInTime {
        winning_tier: f32,
        loser_tier: f32,
        winning_rarity: String,
        loser_rarity: String,
    },
    TieBrokenByTime {
        diff_ms: i64,
        winning_rarity: String,
    },
    ScoreReached {
        score: f32,
    },
    HighestScoreInTime {
        winning_score: f32,
        loser_score: f32,
    },
    OpponentSurrendered,
    OpponentTimedOut,
    BothZeroRolls,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum BattleOutcome {
    Winner {
        winner_id: u64,
        loser_id: u64,
        reason: WinReason,
    },
    Draw,
    Cancelled,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BattleSession {
    pub id: ShortId,
    pub mode: BattleMode,
    pub status: BattleStatus,
    pub created_at_ms: i64,
    pub player_a: PlayerBattleState,
    pub player_b: Option<PlayerBattleState>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MatchmakingTicket {
    pub id: ShortId,
    pub user_id: u64,
    pub chat_id: i64,
    pub username: Option<String>,
    pub mode: BattleMode,
    pub created_at_ms: i64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum QueueResult {
    Queued,
    Matched,
    AlreadyInBattle,
}
