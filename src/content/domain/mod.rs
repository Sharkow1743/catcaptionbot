pub mod domains;

use std::fmt::Debug;
use std::sync::{Arc, OnceLock};

pub use domains::*;
pub use inventory;

pub trait Domain: Send + Sync + Debug {
    fn id(&self) -> &'static str;
    fn name(&self) -> &'static str;
    fn description(&self) -> &'static str;

    /// Companion/allied domains boosted when this domain is equipped
    fn resonant_domains(&self) -> &'static [&'static str] {
        &[]
    }

    /// Opposing domains suppressed when this domain is equipped
    fn opposing_domains(&self) -> &'static [&'static str] {
        &[]
    }

    /// Computes the cross-domain affinity multiplier for rolling a cat belonging to `target_domain`.
    /// `loyalty` is the equipped companion's loyalty level in [-1.0, 1.0].
    fn weight_multiplier(&self, target_domain: &str, loyalty: f32) -> f32 {
        if self.id() == "neutral" || target_domain == "neutral" {
            return 1.0;
        }

        let l = loyalty.clamp(-1.0, 1.0);
        let pos_loyalty = l.max(0.0);

        if self.resonant_domains().contains(&target_domain) {
            1.0 + 0.80 * pos_loyalty
        } else if self.opposing_domains().contains(&target_domain) {
            (1.0 - 0.70 * pos_loyalty).max(0.05)
        } else if self.id() == target_domain {
            // Minimal/no boost to self prevents monotonic farming loops
            1.0 + 0.05 * pos_loyalty
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
