use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct UserStats {
    pub total_rolls: u64,
    pub total_caught: u64,
    pub total_released: u64,
    pub total_escaped: u64,
    pub battles_won: u64,
    pub battles_lost: u64,
    pub battles_draw: u64,
    pub best_tier_rolled: Option<f32>,
    pub best_rarity_name: Option<String>,
}
