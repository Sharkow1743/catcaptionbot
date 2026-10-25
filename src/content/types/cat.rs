use rand::RngExt;
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use uuid::Uuid;

use crate::assets::CachedImage;
use crate::content::domain::{Domain, DomainManager};
use crate::content::loyalty::Loyality;
use crate::content::rarity::Rarity;
use crate::state::AppState;

use super::Content;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct CatStats {
    pub reach_bonus: f32,
    pub stamina_mult: f32,
    pub catch_bonus: f32,
    pub panic_resistance: f32,
}

impl Default for CatStats {
    fn default() -> Self {
        Self {
            reach_bonus: 0.0,
            stamina_mult: 1.0,
            catch_bonus: 0.0,
            panic_resistance: 0.0,
        }
    }
}

impl CatStats {
    pub fn roll_random(tier: f32) -> Self {
        let mut rng = rand::rng();
        let base_quality = (tier + 1.0) / 2.0;

        let reach_bonus =
            ((rng.random_range(-0.05..0.20) + base_quality * 0.10) * 100.0).round() / 100.0;
        let stamina_mult =
            ((rng.random_range(0.85..1.45) + base_quality * 0.15) * 100.0).round() / 100.0;
        let catch_bonus =
            ((rng.random_range(-0.06..0.12) + base_quality * 0.05) * 100.0).round() / 100.0;
        let panic_resistance =
            ((rng.random_range(0.0..0.35) + base_quality * 0.15).clamp(0.0, 0.60) * 100.0).round()
                / 100.0;

        Self {
            reach_bonus,
            stamina_mult,
            catch_bonus,
            panic_resistance,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct CatTrait {
    pub id: String,
    pub name: String,
    pub desc: String,
    pub is_positive: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct Cat {
    content: Content,
    pub loyalty: Loyality,
    pub quote: String,
    pub desc: String,
    pub uuid: Uuid,
    pub rolls_count: u32,
    pub reroll_count: u32,
    pub stats: CatStats,
    pub traits: Vec<CatTrait>,
}

impl Default for Cat {
    fn default() -> Self {
        Self {
            content: Content::default(),
            loyalty: Loyality::default(),
            quote: String::new(),
            desc: String::new(),
            uuid: Uuid::new_v4(),
            rolls_count: 0,
            reroll_count: 0,
            stats: CatStats::default(),
            traits: Vec::new(),
        }
    }
}

impl Cat {
    pub async fn new(content: Content, app_state: &AppState) -> Self {
        let quote = if let Ok(Some(quote)) = app_state
            .composer
            .quotes
            .random_quote_containing(&content.image_text())
            .await
        {
            quote.text
        } else {
            String::new()
        };

        let domain = DomainManager::global()
            .get_by_id(content.rarity().domain_id())
            .unwrap_or_else(DomainManager::default_domain);

        let desc = capitalize_first(
            app_state
                .composer
                .harmonize_words(&[
                    domain.name().to_string(),
                    content.rarity().name(),
                    "кот".to_string(),
                    content.image_text(),
                ])
                .await,
        );

        let tier = content.rarity().tier();
        let stats = CatStats::roll_random(tier);

        Self {
            content,
            loyalty: Loyality::new(),
            quote,
            desc,
            uuid: Uuid::new_v4(),
            rolls_count: 0,
            reroll_count: 0,
            stats,
            traits: Vec::new(),
        }
    }

    pub fn reroll_shard_cost(&self) -> u32 {
        let tier = self.rarity().tier();
        let base_cost = match tier {
            t if t < 0.0 => 4,
            t if t < 0.3 => 8,
            t if t < 0.6 => 14,
            t if t < 0.85 => 22,
            _ => 35,
        };
        base_cost + (self.reroll_count * 4)
    }

    pub fn release_shard_reward(&self) -> u32 {
        let tier = self.rarity().tier();
        let base = match tier {
            t if t < 0.0 => 1,
            t if t < 0.3 => 3,
            t if t < 0.6 => 6,
            t if t < 0.85 => 12,
            _ => 20,
        };
        let veteran_bonus = self.rolls_count / 10;
        base + veteran_bonus
    }

    pub fn reroll_all_stats(&mut self) {
        self.reroll_count += 1;
        self.stats = CatStats::roll_random(self.rarity().tier());

        let mut rng = rand::rng();
        let roll = rng.random_range(0..100);
        let mut new_traits = Vec::new();

        if roll < 40 {
            let pool = [
                (
                    "night_hunter",
                    "Ночной охотник",
                    "Обостряет ночные броски",
                    true,
                ),
                (
                    "tireless",
                    "Неутомимый",
                    "Быстрее восстанавливает силы",
                    true,
                ),
                (
                    "sparking",
                    "Искрящийся",
                    "+2 заряда Lucky Spark при экипировке",
                    true,
                ),
                (
                    "duelist",
                    "Дуэлянт",
                    "Увеличенная отдача очков в дуэлях",
                    true,
                ),
            ];
            let pick = pool[rng.random_range(0..pool.len())];
            new_traits.push(CatTrait {
                id: pick.0.to_string(),
                name: pick.1.to_string(),
                desc: pick.2.to_string(),
                is_positive: pick.3,
            });
        } else if roll < 65 {
            let pool = [
                ("sleepy", "Соня", "Слегка быстрее устает при бросках", false),
                (
                    "frightened",
                    "Пугливый",
                    "Усиленная реакция на панику",
                    false,
                ),
            ];
            let pick = pool[rng.random_range(0..pool.len())];
            new_traits.push(CatTrait {
                id: pick.0.to_string(),
                name: pick.1.to_string(),
                desc: pick.2.to_string(),
                is_positive: pick.3,
            });
        }

        self.traits = new_traits;
    }

    pub fn tick_loyalty(&mut self, is_equipped: bool, now: i64) {
        let tier = self.rarity().tier();
        let regen_mult = self.domain().passive_regen_multiplier();
        self.loyalty.update(is_equipped, tier, now, regen_mult);
    }

    pub fn loyalty_level(&self) -> f32 {
        self.loyalty.level()
    }

    pub fn loyalty_status(&self) -> (&'static str, String) {
        let loyalty = self.loyalty_level();

        let label = if loyalty >= 0.75 {
            "Преданный"
        } else if loyalty >= 0.35 {
            "Спокойный"
        } else {
            "Отдыхает"
        };

        let bar = crate::utils::render_progress_bar(loyalty, 6);

        (label, format!("{label} {bar}"))
    }

    pub fn domain(&self) -> Arc<dyn Domain> {
        let domain_id = self.rarity().domain_id();
        DomainManager::global()
            .get_by_id(domain_id)
            .unwrap_or_else(DomainManager::default_domain)
    }

    pub fn domain_id(&self) -> &'static str {
        self.rarity().domain_id()
    }

    pub fn image(&self) -> &CachedImage {
        self.content.image()
    }

    pub fn image_mut(&mut self) -> &mut CachedImage {
        self.content.image_mut()
    }

    pub fn rarity(&self) -> &Arc<dyn Rarity> {
        self.content.rarity()
    }

    pub fn image_text(&self) -> String {
        self.content.image_text()
    }

    pub fn uuid(&self) -> &Uuid {
        &self.uuid
    }

    pub fn quote(&self) -> &str {
        &self.quote
    }

    pub fn desc(&self) -> &str {
        &self.desc
    }
}

fn capitalize_first(s: String) -> String {
    let mut chars = s.chars();
    match chars.next() {
        None => String::new(),
        Some(first) => {
            let mut capitalized = first.to_uppercase().to_string();
            capitalized.extend(chars);
            capitalized
        }
    }
}
