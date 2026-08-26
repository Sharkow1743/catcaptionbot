use std::sync::Arc;
use std::time::{Duration, Instant};

use self::rarity::Rarity;

use crate::session::{self, UserSession};
use crate::text::TextComposer;
use crate::{AppState, emit_collect, stats};
use bytes::Bytes;
use flume::Receiver;
use image::edit::{preprocess_image, process_image};
use image::fetch_image;
use modifier::ModifierRegistry;
pub use queue::{
    KeyedBufferConfig, KeyedBufferedQueue, RequestError, RequestQueue, WorkerPoolConfig,
    spawn_consumer, spawn_dependent_request_queue, spawn_keyed_buffered_queue, spawn_producer,
    spawn_transformer,
};
use rarity::RarityManager;
use skia_rs_codec::Image;
use tokio::task::spawn_blocking;
pub use types::Content;

mod image;
pub mod modifier;
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
) -> KeyedBufferedQueue<u64, UserSession, anyhow::Result<Content>> {
    let app_state = Arc::new(app_state);

    let image_fetch_queue: Receiver<Bytes> = spawn_producer(
        app_state.clone(),
        WorkerPoolConfig::new(1, 16, 32),
        |state| async move {
            let fetch_time = Instant::now();
            let image = fetch_image(&state.client).await;
            stats!("Fetched image in {:?}", fetch_time.elapsed());
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
            stats!("Preprocessed image in {:?}", preprocess_time.elapsed());
            image
        },
    );

    // Buffers 2 ready images per active user, evicts users after 60s idle
    let buffer_config = KeyedBufferConfig::new(2, Duration::from_secs(30));

    spawn_keyed_buffered_queue(
        app_state.clone(),
        image_preprocess_queue,
        buffer_config,
        process_content,
    )
}

async fn process_content(
    mut session: UserSession,
    image: Image,
    app_state: Arc<AppState>,
) -> anyhow::Result<Content> {
    session.fire_trigger("on_generation_start");
    let modifiers = session.modifiers.display_active("|");

    app_state.session_manager.save_session(&session).await?;

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
        spawn_blocking(move || process_image(rarity.as_ref(), image, text, modifiers)).await??
    };
    let image_time = image_time.elapsed();

    stats!(
        "Content generated in {:?}\nRarity selection: {:?}\nText generation:  {:?}\nImage rendering:  {:?}",
        rarity_time + text_time + image_time,
        rarity_time,
        text_time,
        image_time
    );

    Ok(Content::new(image, rarity, text))
}
