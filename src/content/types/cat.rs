use std::sync::Arc;

use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::assets::CachedImage;
use crate::content::domain::{Domain, DomainManager};
use crate::content::loyalty::Loyality;
use crate::content::rarity::Rarity;
use crate::state::AppState;

use super::Content;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Cat {
    content: Content,
    pub loyality: Loyality,
    quote: String,
    desc: String,
    uuid: Uuid,
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
            "".to_string()
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

        Self {
            content,
            loyality: Loyality::new(),
            quote,
            desc,
            uuid: Uuid::new_v4(),
        }
    }

    pub fn tick_loyalty(&mut self, is_equipped: bool, now: i64) {
        let tier = self.rarity().tier();
        let regen_mult = self.domain().passive_regen_multiplier();
        self.loyality.update(is_equipped, tier, now, regen_mult);
    }

    pub fn loyalty_level(&self) -> f32 {
        self.loyality.level()
    }

    pub fn loyalty_status(&self) -> (&'static str, String) {
        let loyalty = self.loyalty_level();

        let label = if loyalty >= 0.6 {
            "Преданный"
        } else if loyalty >= 0.2 {
            "Дружелюбный"
        } else if loyalty > -0.2 {
            "Спокойный"
        } else if loyalty > -0.6 {
            "Настороженный"
        } else {
            "Обиженный"
        };

        let norm = ((loyalty + 1.0) / 2.0).clamp(0.0, 1.0);
        let bar_len: usize = 6;
        let filled = (norm * bar_len as f32).round() as usize;
        let empty = bar_len.saturating_sub(filled);
        let bar = format!("[{}{}]", "■".repeat(filled), "□".repeat(empty));

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

    pub fn domain_stats(&self) -> String {
        let domain = self.domain();
        let domain_mgr = DomainManager::global();

        let mut parts = Vec::new();
        if let Some(res) = domain.primary_resonance() {
            let res_name = domain_mgr
                .get_by_id(res)
                .map(|d| format!("{} {}", d.icon(), d.name()))
                .unwrap_or_else(|| res.to_string());
            parts.push(format!("Резонанс: {res_name}"));
        }

        if let Some(opp) = domain.primary_opposing() {
            let opp_name = domain_mgr
                .get_by_id(opp)
                .map(|d| format!("{} {}", d.icon(), d.name()))
                .unwrap_or_else(|| opp.to_string());
            parts.push(format!("Конфликт: {opp_name}"));
        }

        if parts.is_empty() {
            format!("{} {}", domain.icon(), domain.name())
        } else {
            format!(
                "{} {} ({})",
                domain.icon(),
                domain.name(),
                parts.join(" • ")
            )
        }
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
