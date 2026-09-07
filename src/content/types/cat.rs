use std::sync::Arc;

use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::assets::CachedImage;
use crate::content::domain::{Domain, DomainManager};
use crate::content::loyalty::Loyality;
use crate::content::rarity::Rarity;
use crate::text::QuoteBank;
use crate::utils::unix_timestamp;

use super::Content;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Cat {
    content: Content,
    pub loyality: Loyality,
    quote: String,
    uuid: Uuid,
}

impl Cat {
    pub async fn new(content: Content, quote_bank: &QuoteBank) -> Self {
        let quote = if let Ok(Some(quote)) = quote_bank
            .random_quote_containing(&content.image_text())
            .await
        {
            quote.text
        } else {
            "".to_string()
        };
        Self {
            content,
            loyality: Loyality::new(),
            quote,
            uuid: Uuid::new_v4(),
        }
    }

    pub fn tick_loyalty(&mut self, is_equipped: bool, now: i64) {
        let tier = self.rarity().tier();
        self.loyality.update(is_equipped, tier, now);
    }

    pub fn loyalty_level(&self) -> f32 {
        self.loyality.level()
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

    /// Formats detailed domain resonance stats for text inspection/stats views
    pub fn domain_stats(&self) -> String {
        let domain = self.domain();
        let loyalty = self.loyalty_level();

        let resonant = domain.resonant_domains();
        let opposing = domain.opposing_domains();

        let resonant_str = if resonant.is_empty() {
            "нет".to_string()
        } else {
            resonant
                .iter()
                .filter_map(|&id| DomainManager::global().get_by_id(id))
                .map(|d| d.name().to_string())
                .collect::<Vec<_>>()
                .join(", ")
        };

        let opposing_str = if opposing.is_empty() {
            "нет".to_string()
        } else {
            opposing
                .iter()
                .filter_map(|&id| DomainManager::global().get_by_id(id))
                .map(|d| d.name().to_string())
                .collect::<Vec<_>>()
                .join(", ")
        };

        let pos_l = loyalty.max(0.0);
        let boost_pct = (0.80 * pos_l * 100.0).round() as i32;
        let suppress_pct = (0.70 * pos_l * 100.0).round() as i32;

        format!(
            "Домен: {}\nСвязь: {:+.2}\nРезонанс (+{}%): {}\nПодавление (-{}%): {}",
            domain.name(),
            loyalty,
            boost_pct,
            resonant_str,
            suppress_pct,
            opposing_str
        )
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
}
