pub mod domains;

use std::fmt::Debug;
use std::sync::{Arc, OnceLock};

pub use domains::*;
pub use inventory;

pub trait Domain: Send + Sync + Debug {
    fn id(&self) -> &'static str;
    fn name(&self) -> &'static str;
    fn icon(&self) -> &'static str {
        "○"
    }

    // Rock-Paper-Scissors cycle
    fn primary_resonance(&self) -> Option<&'static str> {
        None
    }
    fn secondary_resonance(&self) -> Option<&'static str> {
        None
    }
    fn primary_opposing(&self) -> Option<&'static str> {
        None
    }
    fn secondary_opposing(&self) -> Option<&'static str> {
        None
    }

    // Dynamic gameplay parameters
    fn roll_decay_multiplier(&self) -> f32 {
        1.0
    }

    fn take_decay_multiplier(&self, _target_domain: &str, _target_tier: f32) -> f32 {
        1.0
    }

    fn passive_regen_multiplier(&self) -> f32 {
        1.0
    }

    fn release_boost_multiplier(&self) -> f32 {
        1.0
    }

    /// Situational catch rate modifier based on the wild cat's domain, tier, loyalty, and roll speed
    fn catch_modifier(
        &self,
        _target_domain: &str,
        _target_tier: f32,
        _loyalty: f32,
        _time_between_rolls_ms: i64,
    ) -> f32 {
        0.0
    }

    fn domain_reach_bonus(&self, target_domain: &str, loyalty: f32) -> f32 {
        if self.id() == "neutral" {
            return 0.05;
        }

        let pos_loyalty = loyalty.clamp(-1.0, 1.0).max(0.0);

        if self.primary_resonance() == Some(target_domain) {
            0.35 + 0.20 * pos_loyalty
        } else if self.id() == target_domain {
            // Own domain gets NO reach bonus (0.0). Players must use resonant domain to climb.
            0.0
        } else if self.primary_opposing() == Some(target_domain) {
            -0.25
        } else if self.secondary_opposing() == Some(target_domain) {
            -0.15
        } else if target_domain == "neutral" {
            0.05
        } else {
            0.05
        }
    }

    fn weight_multiplier(&self, target_domain: &str, loyalty: f32, _equipped_tier: f32) -> f32 {
        if self.id() == "neutral" {
            let pos_loyalty = loyalty.clamp(-1.0, 1.0).max(0.0);
            return (1.0 - 0.20 * pos_loyalty).max(0.80);
        }

        let l = loyalty.clamp(-1.0, 1.0);
        let pos_loyalty = l.max(0.0);

        if target_domain == "neutral" {
            return (1.0 - 0.35 * pos_loyalty).max(0.65);
        }

        if self.primary_resonance() == Some(target_domain) {
            1.0 + 0.50 * pos_loyalty
        } else if self.id() == target_domain {
            // Own domain is not attracted; slightly diluted to encourage domain rotation
            (1.0 - 0.10 * pos_loyalty).max(0.90)
        } else if self.primary_opposing() == Some(target_domain) {
            (1.0 - 0.55 * pos_loyalty).max(0.20)
        } else if self.secondary_opposing() == Some(target_domain) {
            (1.0 - 0.35 * pos_loyalty).max(0.35)
        } else {
            1.0
        }
    }

    fn domain_eval(&self, target_domain: &str, loyalty: f32, equipped_tier: f32) -> (f32, f32) {
        let pos_loyalty = loyalty.clamp(-1.0, 1.0).max(0.0);

        if self.id() == "neutral" {
            let cap = (equipped_tier + 0.20 + 0.10 * pos_loyalty).max(0.35);
            let mult = (1.0 - 0.20 * pos_loyalty).max(0.80);
            return (cap, mult);
        }

        if target_domain == "neutral" {
            return (0.10, (1.0 - 0.40 * pos_loyalty).max(0.50));
        }

        if self.primary_resonance() == Some(target_domain) {
            // Resonance: The only domain that unlocks higher tiers & heavy boost
            let cap = equipped_tier + 0.35 + 0.25 * pos_loyalty;
            let mult = 1.0 + 0.75 * pos_loyalty;
            (cap, mult)
        } else if self.id() == target_domain {
            // Own domain: Cannot climb self; forces swapping
            let cap = (equipped_tier - 0.05).max(0.20);
            let mult = (1.0 - 0.20 * pos_loyalty).max(0.70);
            (cap, mult)
        } else if self.primary_opposing() == Some(target_domain) {
            // Primary opposing: Heavily restricted
            (0.15, (1.0 - 0.65 * pos_loyalty).max(0.15))
        } else if self.secondary_opposing() == Some(target_domain) {
            // Secondary opposing: Restricted
            (0.25, (1.0 - 0.45 * pos_loyalty).max(0.30))
        } else {
            // Unrelated: Baseline starter cap
            (0.35, 1.0)
        }
    }
}

pub struct DomainRegistration(pub fn() -> Arc<dyn Domain>);
inventory::collect!(DomainRegistration);

#[macro_export]
macro_rules! register_domain {
    ($expr:expr) => {
        $crate::content::domain::inventory::submit! {
            $crate::content::domain::DomainRegistration(|| std::sync::Arc::new($expr))
        }
    };
}

#[derive(Clone)]
pub struct DomainManager {
    domains: Vec<Arc<dyn Domain>>,
}

impl DomainManager {
    pub fn new() -> Self {
        Self {
            domains: Vec::new(),
        }
    }

    pub fn from_registry() -> Self {
        let mut mgr = Self::new();
        for reg in inventory::iter::<DomainRegistration> {
            mgr.domains.push((reg.0)());
        }
        mgr
    }

    pub fn global() -> &'static DomainManager {
        static GLOBAL: OnceLock<DomainManager> = OnceLock::new();
        GLOBAL.get_or_init(DomainManager::from_registry)
    }

    pub fn all(&self) -> &[Arc<dyn Domain>] {
        &self.domains
    }

    pub fn get_by_id(&self, id: &str) -> Option<Arc<dyn Domain>> {
        self.domains.iter().find(|d| d.id() == id).cloned()
    }

    pub fn default_domain() -> Arc<dyn Domain> {
        DomainManager::global()
            .get_by_id("neutral")
            .unwrap_or_else(|| Arc::new(domains::NeutralDomain))
    }
}
