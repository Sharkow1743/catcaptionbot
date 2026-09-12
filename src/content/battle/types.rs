use crate::utils::short_id::ShortId;
use rand::RngExt;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum BattleMode {
    /// First player to find a cat with tier >= target_tier (accessible mid-tier targets)
    TargetRarity {
        target_tier: f32,
        target_rarity_name: String,
        timeout_secs: u64,
    },
    /// Pure sprint: highest rarity rolled within 15-45 seconds wins
    TimeAttack { duration_secs: u64 },
    /// Fast roll sprint: exactly N rolls each, best single cat wins
    BurstSprint { max_rolls: usize, timeout_secs: u64 },
}

impl BattleMode {
    pub fn random() -> Self {
        let mut rng = rand::rng();
        let roll = rng.random_range(0..100);

        if roll < 45 {
            // Target Rarity: Pick accessible tiers (-0.50 to +0.45) so duels resolve cleanly & quickly
            let accessible_targets = [
                ("Необычный", -0.50),
                ("Редкий", -0.20),
                ("Ретро VHS", 0.10),
                ("Глубоководный", 0.30),
                ("Эпический", 0.35),
                ("Кристальный", 0.45),
            ];
            let idx = rng.random_range(0..accessible_targets.len());
            let (name, tier) = accessible_targets[idx];
            let timeout_secs = rng.random_range(30..=50);

            BattleMode::TargetRarity {
                target_tier: tier,
                target_rarity_name: name.to_string(),
                timeout_secs,
            }
        } else if roll < 80 {
            // Time Attack: strictly between 20 and 45 seconds (never > 60s, never < 10s)
            let duration_secs = rng.random_range(20..=40);
            BattleMode::TimeAttack { duration_secs }
        } else {
            // Burst Sprint: 8 to 14 rolls sprint
            let max_rolls = rng.random_range(8..=12);
            BattleMode::BurstSprint {
                max_rolls,
                timeout_secs: 35,
            }
        }
    }

    pub fn description(&self) -> String {
        match self {
            BattleMode::TargetRarity {
                target_rarity_name,
                timeout_secs,
                ..
            } => {
                format!(
                    "🎯 <b>Гонка за качеством</b> (макс. {timeout_secs}с)
                    Побеждает тот, кто <b>первым</b> выбьет кота редкости <b>{target_rarity_name}</b> или выше!"
                )
            }
            BattleMode::TimeAttack { duration_secs } => {
                format!(
                    "⚡ <b>Блиц-дуэль ({duration_secs} сек)</b>
                    Жмите ролл на предельной скорости! Побеждает самый редкий кот к моменту сирены."
                )
            }
            BattleMode::BurstSprint {
                max_rolls,
                timeout_secs,
            } => {
                format!(
                    "🎲 <b>Спринт на {max_rolls} бросков</b> (лимит {timeout_secs}с)
                    У каждого ровно {max_rolls} попыток! Побеждает наивысший результат."
                )
            }
        }
    }

    pub fn max_duration_secs(&self) -> u64 {
        match self {
            BattleMode::TargetRarity { timeout_secs, .. } => *timeout_secs,
            BattleMode::TimeAttack { duration_secs } => *duration_secs,
            BattleMode::BurstSprint { timeout_secs, .. } => *timeout_secs,
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
    pub rolls_count: usize,
    pub best_rarity_id: String,
    pub best_rarity_name: String,
    pub best_tier: f32,
    pub best_tier_timestamp_ms: i64,
    pub surrendered: bool,
}

impl PlayerBattleState {
    pub fn new(user_id: u64, chat_id: i64) -> Self {
        Self {
            user_id,
            chat_id,
            rolls_count: 0,
            best_rarity_id: String::new(),
            best_rarity_name: "Обычный".to_string(),
            best_tier: -999.0,
            best_tier_timestamp_ms: 0,
            surrendered: false,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum WinReason {
    TargetReached { rarity_name: String },
    HighestTierInTime { winning_tier: f32, loser_tier: f32 },
    TieBrokenByTime { diff_ms: i64 },
    BurstLimitCompleted,
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
    pub mode: BattleMode,
    pub created_at_ms: i64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum QueueResult {
    Queued,
    Matched,
    AlreadyInBattle,
}
