use crate::utils::unix_timestamp;
use serde::{Deserialize, Serialize};

// =========================================================================
//  BALANCING CONSTANTS
// =========================================================================

const BENCH_REGEN_PER_SEC: f32 = 0.0025;
const EQUIPPED_REGEN_MULT: f32 = 0.25;
const BASE_ROLL_DECAY: f32 = 0.025;
const BASE_TAKE_DECAY: f32 = 0.08;
const BASE_RELEASE_BOOST: f32 = 0.15;

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
            level: 0.0,
            last_update: unix_timestamp(),
        }
    }

    pub fn level(&self) -> f32 {
        self.level
    }

    pub fn update(&mut self, is_equipped: bool, rarity_tier: f32, now: i64, passive_mult: f32) {
        let elapsed_ms = (now - self.last_update).max(0);
        let elapsed_secs = (elapsed_ms as f32) / 1000.0;
        self.last_update = now;

        if elapsed_secs <= 0.0 {
            return;
        }

        let tier_bonus = (1.0 + rarity_tier * 0.5).max(0.5);
        let slot_mult = if is_equipped {
            EQUIPPED_REGEN_MULT
        } else {
            1.0
        };
        let rate = BENCH_REGEN_PER_SEC * slot_mult * tier_bonus * passive_mult;

        self.level = (self.level + rate * elapsed_secs).clamp(-1.0, 1.0);
    }

    pub fn decay_roll(&mut self, rarity_tier: f32, domain_decay_mult: f32) {
        let resistance = (1.0 + rarity_tier * 0.4).max(0.6);
        let delta = (BASE_ROLL_DECAY * domain_decay_mult) / resistance;
        self.level = (self.level - delta).clamp(-1.0, 1.0);
    }

    pub fn decay_take(&mut self, rarity_tier: f32, domain_take_mult: f32) {
        let resistance = (1.0 + rarity_tier * 0.4).max(0.6);
        let delta = (BASE_TAKE_DECAY * domain_take_mult) / resistance;
        self.level = (self.level - delta).clamp(-1.0, 1.0);
    }

    pub fn boost_release(&mut self, domain_release_mult: f32) {
        let gain = BASE_RELEASE_BOOST * domain_release_mult;
        self.level = (self.level + gain).clamp(-1.0, 1.0);
    }

    pub fn base_tier_multiplier(loyalty_level: f32, target_tier: f32, _equipped_tier: f32) -> f32 {
        if loyalty_level < 0.0 {
            let penalty = (0.25 * loyalty_level * target_tier.max(0.0)).exp();
            let trash_boost = if target_tier < 0.0 {
                1.0 + 0.20 * (-loyalty_level) * (-target_tier)
            } else {
                1.0
            };
            return penalty * trash_boost;
        }

        if target_tier > 0.0 {
            1.0 + 0.15 * loyalty_level * target_tier.clamp(0.0, 1.0)
        } else {
            1.0 - 0.15 * loyalty_level * (-target_tier).clamp(0.0, 1.0)
        }
    }
}
