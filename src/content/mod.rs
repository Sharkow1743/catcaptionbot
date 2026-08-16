use self::rarity::Rarity;

use crate::text::TextComposer;
use image::fetch_image;
use rarity::RarityManager;
use types::Content;

mod image;
pub mod rarity;
mod types;

async fn generate_rarity_text(
    rarity_manager: RarityManager,
    composer: TextComposer,
) -> anyhow::Result<String> {
    let rarity = rarity_manager
        .pick()
        .ok_or_else(|| anyhow::anyhow!("Could not generate rarity"))?;
    let template = rarity.template();

    composer.generate_and_harmonize(template).await
}

pub async fn generate_content(
    rarity_manager: RarityManager,
    composer: TextComposer,
) -> anyhow::Result<Content> {
    let (image, text) = tokio::try_join!(
        fetch_image(),
        generate_rarity_text(rarity_manager, composer)
    )?;
}
