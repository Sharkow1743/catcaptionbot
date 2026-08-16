use self::rarity::Rarity;

use crate::text::TextComposer;

mod image;
pub mod rarity;
mod types;

pub async fn generate_rarity_text<R: Rarity>(
    rarity: R,
    composer: TextComposer,
) -> anyhow::Result<String> {
    let template = rarity.template();

    composer.generate_and_harmonize(template).await
}
