use cached_image::CachedImage;

use super::rarity::Rarity;

mod cached_image;

struct Content<R: Rarity> {
    image: CachedImage,
    rarity: R,
    image_text: String,
}
