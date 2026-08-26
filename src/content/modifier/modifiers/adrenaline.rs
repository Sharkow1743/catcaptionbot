use serde::{Deserialize, Serialize};
use std::collections::HashMap;

use crate::content::modifier::{HookArg, Modifier};
use crate::{modifier, notice};

#[derive(Clone, Debug, Serialize, Deserialize, Default, PartialEq, Eq)]
pub struct Adrenaline {
    pub level: u32,
}

impl Modifier for Adrenaline {
    const ID: &'static str = "adrenaline";
    const TRIGGERS: &'static [&'static str] = &["on_roll", "on_catch", "on_escape"];
    const HOOKS: &'static [&'static str] = &["rarity_weights"];

    fn is_active(&self) -> bool {
        self.level > 0
    }

    fn display(&self) -> String {
        if self.level > 0 {
            format!("⚡{}", self.level)
        } else {
            String::new()
        }
    }

    fn on_trigger(&mut self, name: &str, _session: &mut crate::session::UserSession) {
        match name {
            "on_roll" => {
                self.level = self.level.saturating_add(1);
                notice!("Adrenaline increased to {}", self.level);
            }
            "on_catch" => {
                self.level /= 2;
                notice!("Adrenaline partially reset to {}", self.level);
            }
            "on_escape" => {
                notice!("Adrenaline maintained at {}", self.level);
            }
            _ => {}
        }
    }

    fn apply_hook(&self, _session: &crate::session::UserSession, hook: &mut HookArg) {
        if self.level > 0 {
            hook.on::<HashMap<String, f32>>("rarity_weights", |weights| {
                // Per design pillar: Adrenaline increases variance (boosts rare cat odds)
                let boost = 1.0 + (self.level as f32 * 0.20);
                if let Some(rare_weight) = weights.get_mut("rare") {
                    *rare_weight *= boost;
                }
            });
        }
    }
}

modifier!(Adrenaline);
