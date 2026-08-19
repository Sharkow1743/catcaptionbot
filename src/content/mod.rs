use std::time::Instant;

use self::rarity::Rarity;

use crate::AppState;
use crate::text::TextComposer;
use image::edit::process_image;
use image::fetch_image;
pub use queue::spawn_queue;
use rarity::RarityManager;
use reqwest::Client;
use tokio::task::spawn_blocking;
pub use types::Content;

mod image;
mod queue;
pub mod rarity;
mod types;

async fn generate_rarity_text(
    rarity: &dyn Rarity,
    composer: TextComposer,
) -> anyhow::Result<String> {
    let template = rarity.template();
    composer.generate_and_harmonize(template).await
}

pub async fn generate_content(app_state: &AppState) -> anyhow::Result<Content> {
    let start = Instant::now();

    let rarity = RarityManager::global()
        .pick()
        .ok_or_else(|| anyhow::anyhow!("Could not generate rarity"))?;
    log::debug!("Picked rarity: {}", rarity.name());

    let fetch_start = Instant::now();
    let (image, text) = tokio::try_join!(
        fetch_image(&app_state.client),
        generate_rarity_text(rarity.as_ref(), app_state.composer.clone())
    )?;
    log::debug!(
        "Image fetch + text gen took {:?} (rarity={}, text={:?})",
        fetch_start.elapsed(),
        rarity.name(),
        text
    );

    let image = {
        let rarity = rarity.clone();
        let text = text.clone();
        let render_start = Instant::now();
        let img = spawn_blocking(move || process_image(rarity.as_ref(), image, text)).await??;
        log::debug!(
            "Image render (spawn_blocking) took {:?}",
            render_start.elapsed()
        );
        img
    };

    log::info!("generate_content finished in {:?}", start.elapsed());
    Ok(Content::new(image, rarity, text))
}
