use std::sync::Arc;

use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::assets::CachedImage;
use crate::content::rarity::Rarity;

use super::Content;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Cat {
    content: Content,
    loyality: f32,
    uuid: Uuid,
}

impl Cat {
    pub fn new(content: Content) -> Self {
        Self {
            content,
            loyality: 0.0,
            uuid: Uuid::new_v4(),
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
}
