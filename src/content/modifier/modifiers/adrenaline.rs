use serde::{Deserialize, Serialize};

use crate::content::modifier::Modifier;
use crate::{modifier, notice};

#[derive(Clone, Debug, Serialize, Deserialize, Default)]
pub struct Adrenaline {
    level: u32,
}

impl Modifier for Adrenaline {
    const ID: &'static str = "adrenaline";
    const TRIGGERS: &'static [&'static str] = &["on_generation_start", "on_roll"];
    const HOOKS: &'static [&'static str] = &[];

    fn is_active(&self) -> bool {
        true
    }

    fn display(&self) -> String {
        "asd".to_string()
    }

    fn on_trigger(&mut self, name: &str, _session: &mut crate::session::UserSession) {
        match name {
            "on_generation_start" => {
                notice!("on_generation_start")
            }
            "on_roll" => {
                notice!("on_roll")
            }
            _ => {}
        }
    }
}

modifier!(Adrenaline);
