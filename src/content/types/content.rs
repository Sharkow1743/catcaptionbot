use std::sync::Arc;

use serde::{Deserialize, Deserializer, Serialize, Serializer};

use super::super::rarity::{Rarity, RarityManager};
use crate::assets::CachedImage;

#[derive(Clone)]
pub struct Content {
    image: CachedImage,
    rarity: Arc<dyn Rarity>,
    image_text: String,
}

impl Content {
    pub fn new(image: CachedImage, rarity: Arc<dyn Rarity>, image_text: String) -> Self {
        Self {
            image,
            rarity,
            image_text,
        }
    }

    pub fn image_mut(&mut self) -> &mut CachedImage {
        &mut self.image
    }

    pub fn image(&self) -> &CachedImage {
        &self.image
    }

    pub fn rarity(&self) -> &Arc<dyn Rarity> {
        &self.rarity
    }

    pub fn image_text(&self) -> String {
        self.image_text.to_string()
    }
}

#[derive(Serialize, Deserialize)]
struct ContentData<'a> {
    image: std::borrow::Cow<'a, CachedImage>,
    rarity_id: String,
    image_text: std::borrow::Cow<'a, String>,
}

impl Serialize for Content {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        let data = ContentData {
            image: std::borrow::Cow::Borrowed(&self.image),
            rarity_id: self.rarity.id(),
            image_text: std::borrow::Cow::Borrowed(&self.image_text),
        };
        data.serialize(serializer)
    }
}

impl<'de> Deserialize<'de> for Content {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let data = ContentData::deserialize(deserializer)?;

        let rarity = RarityManager::global()
            .get_by_id(&data.rarity_id)
            .unwrap_or_else(|| RarityManager::global().pick().unwrap());

        Ok(Content {
            image: data.image.into_owned(),
            rarity,
            image_text: data.image_text.into_owned(),
        })
    }
}

impl std::fmt::Debug for Content {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Content")
            .field("image", &self.image)
            .field("rarity_id", &self.rarity.id())
            .field("image_text", &self.image_text)
            .finish()
    }
}
