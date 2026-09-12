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

    fn weight_multiplier(&self, target_domain: &str, loyalty: f32, _equipped_tier: f32) -> f32 {
        if self.id() == "neutral" {
            return 1.0;
        }

        let l = loyalty.clamp(-1.0, 1.0);
        let pos_loyalty = l.max(0.0);

        if target_domain == "neutral" {
            return (1.0 - 0.25 * pos_loyalty).max(0.75);
        }

        if self.primary_resonance() == Some(target_domain) {
            1.0 + 0.70 * pos_loyalty
        } else if self.id() == target_domain {
            1.0 + 0.30 * pos_loyalty
        } else if self.primary_opposing() == Some(target_domain) {
            (1.0 - 0.50 * pos_loyalty).max(0.15)
        } else if self.secondary_opposing() == Some(target_domain) {
            (1.0 - 0.25 * pos_loyalty).max(0.25)
        } else {
            1.0
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
