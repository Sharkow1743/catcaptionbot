use serde::{Deserialize, Serialize};

use crate::utils::unix_timestamp;

// =========================================================================
//  BALANCING CONSTANTS
// =========================================================================

/// Base passive loyalty regeneration per second when resting on the bench (~15% per minute)
const BENCH_REGEN_PER_SEC: f32 = 0.0025;
/// Multiplier for passive regeneration when the cat is active (equipped) (~3.75% per minute)
const EQUIPPED_REGEN_MULT: f32 = 0.25;
/// Base loyalty wear per roll
const BASE_ROLL_DECAY: f32 = 0.025;
/// Base loyalty wear when catching/taking a new cat (jealousy penalty)
const BASE_TAKE_DECAY: f32 = 0.08;
/// Loyalty bonus gained when releasing an unneeded cat
const RELEASE_BOOST: f32 = 0.15;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Loyality {
    pub level: f32,
    pub last_update: i64,
}

impl Default for Loyality {
    fn default() -> Self {
        Self::new()
    }
}

impl Loyality {
    pub fn new() -> Self {
        Self {
            level: 0.0, // Neutral start per anti-laundering design
            last_update: unix_timestamp(),
        }
    }

    pub fn level(&self) -> f32 {
        self.level
    }

    /// Computes passive regeneration up to `now` and advances the timestamp.
    pub fn update(&mut self, is_equipped: bool, rarity_tier: f32, now: i64) {
        let elapsed_ms = (now - self.last_update).max(0);
        let elapsed_secs = (elapsed_ms as f32) / 1000.0;
        self.last_update = now;

        if elapsed_secs <= 0.0 {
            return;
        }

        // Better rarities regenerate faster: tier is mapped [-1.0, 1.0] -> multiplier [0.75, 1.5]
        let tier_bonus = (1.0 + rarity_tier * 0.5).max(0.5);
        let slot_mult = if is_equipped {
            EQUIPPED_REGEN_MULT
        } else {
            1.0
        };
        let rate = BENCH_REGEN_PER_SEC * slot_mult * tier_bonus;

        self.level = (self.level + rate * elapsed_secs).clamp(-1.0, 1.0);
    }

    /// Applies wear when using this cat to roll. Higher tier cats suffer less decay.
    pub fn decay_roll(&mut self, rarity_tier: f32) {
        let resistance = (1.0 + rarity_tier * 0.4).max(0.6);
        let delta = BASE_ROLL_DECAY / resistance;
        self.level = (self.level - delta).clamp(-1.0, 1.0);
    }

    /// Applies jealousy wear when catching/taking another cat.
    pub fn decay_take(&mut self, rarity_tier: f32) {
        let resistance = (1.0 + rarity_tier * 0.4).max(0.6);
        let delta = BASE_TAKE_DECAY / resistance;
        self.level = (self.level - delta).clamp(-1.0, 1.0);
    }

    /// Releasing another cat gives devotion bonus to the current companion.
    pub fn boost_release(&mut self) {
        self.level = (self.level + RELEASE_BOOST).clamp(-1.0, 1.0);
    }

    /// Weight multiplier curve: high loyalty boosts higher tiers and suppresses lower tiers;
    /// low loyalty boosts lower tiers and suppresses higher tiers; neutral loyalty has no effect.
    pub fn weight_multiplier(loyalty_level: f32, rarity_tier: f32) -> f32 {
        if loyalty_level.abs() < 0.001 {
            return 1.0;
        }
        // Formula: exp(k * L * T)
        (1.25 * loyalty_level * rarity_tier).exp()
    }
}
