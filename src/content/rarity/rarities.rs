use skia_rs_core::Color;

use super::SimpleRarity;
use crate::register_rarity;
use crate::text::WordSpec;

// Common (Tier: -1.0)
register_rarity!(
    SimpleRarity::new("Обычный", "common")
        .tier(-1.0)
        .template(vec![WordSpec::new()])
        .weight(3.0)
);

// Rare (Tier: 1.0)
register_rarity!(
    SimpleRarity::new("Редкий", "rare")
        .tier(1.0)
        .template(vec![WordSpec::new().similar_to_q8_str("lKVMOxD39PT9AA/x+QIG/Q4L//YHA/YfB/gA8S4F9/kHCQMBDQPsBRYH3uP7ChL5MQsJ7fzzEgUC//IBDBH/BQ3zAwgNBAf1+gsO9vzq8esKAAgOC/f+Ge4ECQcJAxEUNfjvHSAK/ALw7AcC9uwM9Qr5EwJO/BoA3fH1/fbv/v3mAPUFCwIB8/EEDg/3AhQL8+z5DwXp8QAZAQcC6eoH9v8H3/H6FhD3Cf7r+wPvBgsQBwYK/ggT/gn3G/v2Aw358w/t+NELD6ELE/3+/vUA4EIAf/b0++3mDCf98hgC8QH1DA8FB+kABAYA7OQGBQcSG/MI7R358vcDBvzu/PTm/QQMCgL09f36DQcGA+Xv8RUJ+xQV9Bb6+wPr/wYcCPr39AjzARLp/wUJDuX96ff24fLzCAztC/8AAuYf7A=="), WordSpec::new()])
        .rarity_color(Color::BLUE)
        .text_color(Color::BLUE)
        .weight(3.5)
);
