use serde::{Deserialize, Serialize};
use std::collections::HashMap;

use crate::content::modifier::{HookArg, Modifier};
use crate::content::rarity::RarityManager;
use crate::modifier;

#[derive(Clone, Debug, Serialize, Deserialize, Default, PartialEq)]
pub struct LuckySpark {
    pub charges: u32,
}

impl Modifier for LuckySpark {
    const ID: &'static str = "lucky_spark";
    const TRIGGERS: &'static [&'static str] = &["on_roll", "on_cat_equip"];
    const HOOKS: &'static [&'static str] = &["rarity_weights"];

    fn is_active(&self) -> bool {
        self.charges > 0
    }

    fn display(&self) -> String {
        "⁂".to_string()
    }

    fn apply_hook<'a>(
        &self,
        _session: &crate::session::UserSession,
        hook: HookArg<'a>,
    ) -> HookArg<'a> {
        let charges = self.charges;
        let rarity_mgr = RarityManager::global();

        hook.on::<HashMap<String, f32>>("rarity_weights", move |mut weights| {
            if charges == 0 {
                return weights;
            }

            for (id, weight) in weights.iter_mut() {
                if let Some(rarity) = rarity_mgr.get_by_id(id) {
                    let tier = rarity.tier();
                    if tier > 0.1 {
                        *weight *= 1.35;
                    }
                }
            }
            weights
        })
    }

    fn on_trigger(&mut self, name: &str, session: &mut crate::session::UserSession) {
        match name {
            "on_cat_equip" => {
                if let Some(equipped) = session.inventory.get_ref("equip", 0) {
                    if equipped.cat.loyalty_level() >= 0.85 {
                        let trait_bonus = equipped
                            .cat
                            .traits
                            .iter()
                            .filter(|t| t.id == "sparking")
                            .count() as u32
                            * 2;
                        self.charges = 4 + trait_bonus;
                    }
                }
            }
            "on_roll" => {
                self.charges = self.charges.saturating_sub(1);
            }
            _ => {}
        }
    }
}

modifier!(LuckySpark);
