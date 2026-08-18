use super::rarity::Rarity;
use std::sync::Arc;

mod cached_image;
pub use cached_image::CachedImage;

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

    pub fn image(&mut self) -> &mut CachedImage {
        &mut self.image
    }

    pub fn rarity(&self) -> &Arc<dyn Rarity> {
        &self.rarity
    }

    pub fn image_text(&self) -> String {
        self.image_text.to_string()
    }
}
