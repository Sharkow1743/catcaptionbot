use crate::utils::unix_timestamp;
use serde::{Deserialize, Serialize};

// =========================================================================
//  BALANCING CONSTANTS
// =========================================================================

/// Resting on the bench in the inventory box takes ~3.5 minutes for full recovery (0 -> +1.0)
const BENCH_REGEN_PER_SEC: f32 = 0.005;

/// 1. Equipped cats do not recover passively while on active duty
const EQUIPPED_REGEN_MULT: f32 = 0.0;

/// Mild roll decay (~35-40 rolls from full loyalty)
const BASE_ROLL_DECAY: f32 = 0.030;

/// 2. Moderate catch decay (-0.12)
const BASE_TAKE_DECAY: f32 = 0.12;

/// Releasing cats grants a loyalty boost (+0.20)
const BASE_RELEASE_BOOST: f32 = 0.20;

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
        let resistance = (1.0 + rarity_tier * 0.3).clamp(0.7, 1.4);
        let delta = (BASE_ROLL_DECAY * domain_decay_mult) / resistance;
        self.level = (self.level - delta).clamp(-1.0, 1.0);
    }

    pub fn decay_take(&mut self, rarity_tier: f32, domain_take_mult: f32) {
        let resistance = (1.0 + rarity_tier * 0.3).clamp(0.7, 1.4);
        let delta = (BASE_TAKE_DECAY * domain_take_mult) / resistance;
        self.level = (self.level - delta).clamp(-1.0, 1.0);
    }

    pub fn boost_release(&mut self, domain_release_mult: f32) {
        let gain = BASE_RELEASE_BOOST * domain_release_mult;
        self.level = (self.level + gain).clamp(-1.0, 1.0);
    }

    pub fn base_tier_multiplier(
        loyalty_level: f32,
        target_tier: f32,
        effective_cap: f32,
        is_unequipped: bool,
    ) -> f32 {
        let loyalty = loyalty_level.clamp(-1.0, 1.0);
        let pos_loyalty = loyalty.max(0.0);
        let neg_loyalty = (-loyalty).max(0.0);

        let steepness = if is_unequipped {
            8.5
        } else {
            7.0 - 3.0 * pos_loyalty
        };

        // 1. Cap factor: Suppresses any tier higher than the current effective cap
        let cap_factor = if target_tier > effective_cap {
            let diff = target_tier - effective_cap;
            (-steepness * diff * (1.0 + 0.5 * diff)).exp().max(0.0002)
        } else {
            1.0
        };

        if is_unequipped {
            return cap_factor;
        }

        // 2. Equipped loyalty modifiers
        if loyalty < 0.0 {
            let trash_boost = if target_tier < 0.0 {
                1.0 + 0.35 * neg_loyalty * (-target_tier).clamp(0.0, 1.0)
            } else {
                (0.40 * loyalty * target_tier.max(0.0)).exp()
            };
            return (cap_factor * trash_boost).max(0.0001);
        }

        let tier_mod = if target_tier > 0.0 {
            let dampening = (1.0 - (target_tier - 0.5).max(0.0) * 0.7).clamp(0.3, 1.0);
            0.70 + 0.55 * loyalty * dampening
        } else {
            let bad_severity = (-target_tier).clamp(0.0, 1.0);
            (1.25 - 0.90 * loyalty * bad_severity).max(0.25)
        };

        (cap_factor * tier_mod).max(0.0001)
    }
}
