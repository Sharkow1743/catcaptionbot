use std::sync::Arc;
use std::time::Instant;

use self::rarity::Rarity;

use crate::AppState;
use crate::text::TextComposer;
use bytes::Bytes;
use flume::Receiver;
use image::edit::{preprocess_image, process_image};
use image::fetch_image;
pub use queue::{
    RequestError, RequestQueue, WorkerPoolConfig, spawn_consumer, spawn_dependent_request_queue,
    spawn_producer, spawn_transformer,
};
use rarity::RarityManager;
use skia_rs_codec::Image;
use tokio::task::spawn_blocking;
pub use types::{Content, UserRequest};

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

pub async fn content_queue(
    app_state: AppState,
) -> RequestQueue<UserRequest, anyhow::Result<Content>> {
    let app_state = Arc::new(app_state);
    let image_fetch_queue: Receiver<Bytes> = spawn_producer(
        app_state.clone(),
        WorkerPoolConfig::new(1, 16, 32),
        |state| async move {
            let fetch_time = Instant::now();
            let image = fetch_image(&state.client).await;
            log::info!("Fetched image in {:?}", fetch_time.elapsed());
            image
        },
    );

    let image_preprocess_queue: Receiver<Image> = spawn_transformer(
        app_state.clone(),
        image_fetch_queue,
        WorkerPoolConfig::new(1, 16, 32),
        |image_bytes, _| async move {
            let preprocess_time = Instant::now();
            let image = spawn_blocking(move || preprocess_image(&image_bytes)).await?;
            log::info!("Preprocessed image in {:?}", preprocess_time.elapsed());
            image
        },
    );

    spawn_dependent_request_queue(
        app_state.clone(),
        image_preprocess_queue,
        WorkerPoolConfig::new(1, 16, 32),
        process_content,
    )
}

async fn process_content(
    req: UserRequest,
    image: Image,
    app_state: Arc<AppState>,
) -> anyhow::Result<Content> {
    let rarity_time = Instant::now();
    let rarity = RarityManager::global()
        .pick()
        .ok_or_else(|| anyhow::anyhow!("Could not generate rarity"))?;
    let rarity_time = rarity_time.elapsed();

    let text_time = Instant::now();
    let text = generate_rarity_text(rarity.as_ref(), app_state.composer.clone()).await?;
    let text_time = text_time.elapsed();

    let image_time = Instant::now();
    let image = {
        let rarity = rarity.clone();
        let text = text.clone();
        spawn_blocking(move || process_image(rarity.as_ref(), image, text)).await??
    };
    let image_time = image_time.elapsed();

    log::info!(
        "Content generated in {:?}\nRarity selection: {:?}\nText generation:  {:?}\nImage rendering:  {:?}",
        rarity_time + text_time + image_time,
        rarity_time,
        text_time,
        image_time
    );

    Ok(Content::new(image, rarity, text))
}
