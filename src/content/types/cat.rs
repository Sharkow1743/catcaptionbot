use std::sync::Arc;

use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::assets::CachedImage;
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
