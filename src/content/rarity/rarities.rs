use skia_rs_core::Color;

use super::{Rarity, SimpleRarity};
use crate::register_rarity;
use crate::text::WordSpec;

// Auto-registers this into RarityManager::global() — no need to list it
// anywhere else.
register_rarity!(
    SimpleRarity::new("Обычный", "common")
        .template(vec![WordSpec::new()])
        .weight(3.0)
);
register_rarity!(
    SimpleRarity::new("Редкий", "rare")
        .template(vec![WordSpec::new(), WordSpec::new()])
        .rarity_color(Color::BLUE)
        .text_color(Color::BLUE)
);

// Example of a non-simple rarity defined with a struct + custom logic,
// also auto-registered:
//
// crate::rarity_struct!(pub Legendary, id = "legendary", name = "Легендарный",
//     fn weight(&self) -> f32 { 0.01 }
//     fn is_secret(&self) -> bool { true }
//     fn text_color(&self) -> skia_rs_core::Color {
//         skia_rs_core::Color::from_rgb(255, 215, 0)
//     }
// );
