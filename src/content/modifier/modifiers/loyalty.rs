use serde::{Deserialize, Serialize};
use std::collections::HashMap;

use crate::content::loyalty::Loyality;
use crate::content::modifier::{HookArg, Modifier};
use crate::content::rarity::RarityManager;
use crate::controls::take::CatchContext;
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
    const HOOKS: &'static [&'static str] = &["rarity_weights", "cat_catch_context"];

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
        let active_slot = session.inventory.get_ref("equip", 0);
        let is_unequipped = active_slot.is_none();

        let (loyalty_level, equipped_tier, equipped_domain) = if let Some(slot) = active_slot {
            (
                slot.cat.loyalty_level(),
                slot.cat.rarity().tier(),
                Some(slot.cat.domain()),
            )
        } else {
            (0.0, -0.50, None)
        };

        let rarity_mgr = RarityManager::global();
        let equipped_domain_catch = equipped_domain.clone();

        hook.on::<HashMap<String, f32>>("rarity_weights", move |mut weights| {
            for (id, weight) in weights.iter_mut() {
                if let Some(rarity) = rarity_mgr.get_by_id(id) {
                    let (effective_cap, domain_mult) = if let Some(ref domain) = equipped_domain {
                        domain.domain_eval(rarity.domain_id(), loyalty_level, equipped_tier)
                    } else {
                        (crate::content::loyalty::BASE_UNEQUIPPED_CAP, 1.0)
                    };

                    let base_mult = Loyality::base_tier_multiplier(
                        loyalty_level,
                        rarity.tier(),
                        effective_cap,
                        is_unequipped,
                    );

                    *weight *= base_mult * domain_mult;
                }
            }
            weights
        })
        .on::<CatchContext>("cat_catch_context", move |mut ctx| {
            let domain_mod = if let Some(ref domain) = equipped_domain_catch {
                domain.catch_modifier(
                    ctx.target_domain_id,
                    ctx.target_tier,
                    loyalty_level,
                    ctx.time_since_last_roll_ms,
                )
            } else {
                0.0
            };

            let loyalty_mod = if loyalty_level >= 0.0 {
                loyalty_level * 0.12
            } else {
                loyalty_level * 0.35
            };

            ctx.probability = (ctx.probability + loyalty_mod + domain_mod).clamp(0.05, 0.98);

            ctx
        })
    }

    fn on_trigger(&mut self, name: &str, session: &mut crate::session::UserSession) {
        let now = unix_timestamp();

        let (target_dom, target_tier) = session
            .last_content
            .as_ref()
            .map(|c| (c.rarity().domain_id(), c.rarity().tier()))
            .unwrap_or(("neutral", 0.0));

        if let Some(bag) = session.inventory.category_mut("inventory") {
            for slot in bag.items_mut() {
                slot.cat.tick_loyalty(false, now);
            }
        }

        if let Some(equipped_slot) = session.inventory.get_mut("equip", 0) {
            let domain = equipped_slot.cat.domain();
            equipped_slot.cat.tick_loyalty(true, now);
            let tier = equipped_slot.cat.rarity().tier();
            let stamina_mult = equipped_slot.cat.stats.stamina_mult;

            match name {
                "on_roll" => equipped_slot.cat.loyality.decay_roll(
                    tier,
                    domain.roll_decay_multiplier(),
                    stamina_mult,
                ),
                "on_cat_catch" => {
                    let decay_mult = domain.take_decay_multiplier(target_dom, target_tier);
                    equipped_slot.cat.loyality.decay_take(tier, decay_mult);
                }
                "on_cat_release" => equipped_slot
                    .cat
                    .loyality
                    .boost_release(domain.release_boost_multiplier()),
                _ => {}
            }

            self.has_equipped = true;
            self.level = equipped_slot.cat.loyalty_level();
        } else {
            self.has_equipped = false;
            self.level = 0.0;
        }
    }
}

modifier!(Loyalty);
