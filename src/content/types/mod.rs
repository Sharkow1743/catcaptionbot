use super::rarity::Rarity;

mod cached_image;

pub use cached_image::CachedImage;

pub struct Content<R: Rarity> {
    image: CachedImage,
    rarity: R,
    image_text: String,
}
