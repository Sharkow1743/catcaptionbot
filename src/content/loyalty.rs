use crate::utils::unix_timestamp;
use serde::{Deserialize, Serialize};

const BENCH_REGEN_PER_SEC: f32 = 0.015;
const EQUIPPED_REGEN_MULT: f32 = 0.0;
const BASE_ROLL_DECAY: f32 = 0.035;
const BASE_TAKE_DECAY: f32 = 0.08;
const BASE_RELEASE_BOOST: f32 = 0.25;
pub const BASE_UNEQUIPPED_CAP: f32 = 0.35;

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
            level: 1.0,
            last_update: unix_timestamp(),
        }
    }

    pub fn level(&self) -> f32 {
        self.level.clamp(0.0, 1.0)
    }

    pub fn update(&mut self, is_equipped: bool, rarity_tier: f32, now: i64, passive_mult: f32) {
        let elapsed_ms = (now - self.last_update).max(0);
        let elapsed_secs = (elapsed_ms as f32) / 1000.0;
        self.last_update = now;

        if elapsed_secs <= 0.0 {
            return;
        }

        let tier_bonus = (1.0 + rarity_tier * 0.3).max(0.5);
        let slot_mult = if is_equipped {
            EQUIPPED_REGEN_MULT
        } else {
            1.0
        };
        let rate = BENCH_REGEN_PER_SEC * slot_mult * tier_bonus * passive_mult;

        self.level = (self.level + rate * elapsed_secs).clamp(0.0, 1.0);
    }

    pub fn decay_roll(&mut self, rarity_tier: f32, domain_decay_mult: f32, stamina_mult: f32) {
        let resistance = ((1.0 + rarity_tier * 0.3) * stamina_mult.max(0.5)).clamp(0.5, 2.5);
        let delta = (BASE_ROLL_DECAY * domain_decay_mult) / resistance;
        self.level = (self.level - delta).clamp(0.0, 1.0);
    }

    pub fn decay_take(&mut self, rarity_tier: f32, domain_take_mult: f32) {
        let resistance = (1.0 + rarity_tier * 0.3).clamp(0.7, 1.5);
        let delta = (BASE_TAKE_DECAY * domain_take_mult) / resistance;
        self.level = (self.level - delta).clamp(0.0, 1.0);
    }

    pub fn boost_release(&mut self, domain_release_mult: f32) {
        let gain = BASE_RELEASE_BOOST * domain_release_mult;
        self.level = (self.level + gain).clamp(0.0, 1.0);
    }

    pub fn base_tier_multiplier(
        loyalty_level: f32,
        target_tier: f32,
        effective_cap: f32,
        is_unequipped: bool,
    ) -> f32 {
        let loyalty = loyalty_level.clamp(0.0, 1.0);
        let steepness = if is_unequipped {
            8.5
        } else {
            6.5 - 2.5 * loyalty
        };

        let cap_factor = if target_tier > effective_cap {
            let diff = target_tier - effective_cap;
            (-steepness * diff * (1.0 + 0.5 * diff)).exp().max(0.001)
        } else {
            1.0
        };

        if is_unequipped {
            return cap_factor;
        }

        let tier_mod = if target_tier > 0.0 {
            0.75 + 0.50 * loyalty
        } else {
            (1.10 - 0.30 * loyalty).max(0.80)
        };

        (cap_factor * tier_mod).max(0.001)
    }
}
