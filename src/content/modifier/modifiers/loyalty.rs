use serde::{Deserialize, Serialize};
use std::collections::HashMap;

use crate::content::domain::DomainManager;
use crate::content::loyalty::Loyality;
use crate::content::modifier::{HookArg, Modifier};
use crate::content::rarity::RarityManager;
use crate::utils::unix_timestamp;
use crate::{modifier, notice};

#[derive(Clone, Debug, Serialize, Deserialize, Default, PartialEq)]
pub struct Loyalty {
    pub level: f32,
    pub has_equipped: bool,
}

impl Modifier for Loyalty {
    const ID: &'static str = "loyalty";
    const TRIGGERS: &'static [&'static str] = &[
        "on_roll",
        "on_cat_catch",
        "on_cat_release",
        "on_cat_equip",
        "on_cat_unequip",
    ];
    const HOOKS: &'static [&'static str] = &["rarity_weights"];

    fn is_active(&self) -> bool {
        self.has_equipped
    }

    fn display(&self) -> String {
        String::new()
    }

    fn apply_hook<'a>(
        &self,
        session: &crate::session::UserSession,
        hook: HookArg<'a>,
    ) -> HookArg<'a> {
        let Some(active_slot) = session.inventory.get_ref("equip", 0) else {
            return hook;
        };

        let loyalty_level = active_slot.cat.loyalty_level();
        let equipped_domain_id = active_slot.cat.domain_id();
        let domain_mgr = DomainManager::global();
        let equipped_domain = domain_mgr.get_by_id(equipped_domain_id);
        let rarity_mgr = RarityManager::global();

        hook.on::<HashMap<String, f32>>("rarity_weights", move |mut weights| {
            for (id, weight) in weights.iter_mut() {
                if let Some(rarity) = rarity_mgr.get_by_id(id) {
                    let tier_mult = Loyality::weight_multiplier(loyalty_level, rarity.tier());
                    let domain_mult = if let Some(ref domain) = equipped_domain {
                        domain.weight_multiplier(rarity.domain_id(), loyalty_level)
                    } else {
                        1.0
                    };
                    *weight *= tier_mult * domain_mult;
                }
            }
            weights
        })
    }

    fn on_trigger(&mut self, name: &str, session: &mut crate::session::UserSession) {
        let now = unix_timestamp();

        // 1. Passive regeneration for all benched inventory cats
        if let Some(bag) = session.inventory.category_mut("inventory") {
            for slot in bag.items_mut() {
                slot.cat.tick_loyalty(false, now);
            }
        }

        // 2. Process equipped cat
        if let Some(equipped_slot) = session.inventory.get_mut("equip", 0) {
            equipped_slot.cat.tick_loyalty(true, now);
            let tier = equipped_slot.cat.rarity().tier();

            match name {
                "on_roll" => {
                    equipped_slot.cat.loyality.decay_roll(tier);
                }
                "on_cat_catch" => {
                    equipped_slot.cat.loyality.decay_take(tier);
                }
                "on_cat_release" => {
                    equipped_slot.cat.loyality.boost_release();
                }
                _ => {}
            }

            self.has_equipped = true;
            self.level = equipped_slot.cat.loyalty_level();
            notice!(
                "Loyalty of equipped cat ({}): {:+.2} (trigger: {})",
                equipped_slot.cat.image_text(),
                self.level,
                name
            );
        } else {
            self.has_equipped = false;
            self.level = 0.0;
        }
    }
}

modifier!(Loyalty);
