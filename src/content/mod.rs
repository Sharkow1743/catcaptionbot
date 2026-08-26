use std::sync::Arc;
use std::time::{Duration, Instant};

use bytes::Bytes;
use flume::Receiver;
use skia_rs_codec::Image;

use crate::queue::{
    ComputeLimiter, KeyedBufferConfig, KeyedBufferedQueue, WorkerPoolConfig, run_cpu,
    spawn_keyed_buffered_queue, spawn_producer, spawn_transformer,
};
use crate::session::UserSession;
use crate::state::AppState;
use crate::stats;
use crate::text::TextComposer;
use image::edit::{preprocess_image, process_image};
use image::fetch_image;
use rarity::{Rarity, RarityManager};

pub use types::Content;

mod image;
pub mod modifier;
pub mod rarity;
mod types;

async fn generate_rarity_text(
    rarity: &dyn Rarity,
    composer: TextComposer,
) -> anyhow::Result<String> {
    let template = rarity.template();
    composer.generate_and_harmonize(template).await
}

/// State struct paired with the ComputeLimiter
pub struct PipelineContext {
    pub app_state: Arc<AppState>,
    pub limiter: ComputeLimiter,
}

pub async fn content_queue(
    app_state: AppState,
) -> KeyedBufferedQueue<u64, UserSession, anyhow::Result<Content>> {
    let ctx = Arc::new(PipelineContext {
        app_state: Arc::new(app_state),
        limiter: ComputeLimiter::new(),
    });

    // 1. Image Fetcher: bounded worker count to prevent network/memory saturation
    let image_fetch_queue: Receiver<Bytes> = spawn_producer(
        ctx.clone(),
        WorkerPoolConfig::new(1, 4, 16),
        |ctx| async move {
            let fetch_time = Instant::now();
            let image = fetch_image(&ctx.app_state.client).await;
            stats!("Fetched image in {:?}", fetch_time.elapsed());
            image
        },
    );

    // 2. Image Preprocessor (Background):
    //    Uses `acquire_background` (max 50% CPU quota) and `run_cpu` (Rayon pool)
    let image_preprocess_queue: Receiver<Image> = spawn_transformer(
        ctx.clone(),
        image_fetch_queue,
        WorkerPoolConfig::new(1, 4, 16),
        |image_bytes, ctx| async move {
            let preprocess_time = Instant::now();

            // Low-priority compute permit (yields if foreground needs CPU)
            let _permit = ctx.limiter.acquire_background().await;

            // Offload CPU-heavy Skia decode to Rayon pool
            let image = run_cpu(move || preprocess_image(&image_bytes)).await?;
            stats!("Preprocessed image in {:?}", preprocess_time.elapsed());
            anyhow::Ok(image)
        },
    );

    // 3. Keyed Pre-buffering per user session (2 items ready in buffer, 30s idle TTL)
    let buffer_config = KeyedBufferConfig::new(2, Duration::from_secs(30));

    spawn_keyed_buffered_queue(ctx, image_preprocess_queue, buffer_config, process_content)
}

async fn process_content(
    mut session: UserSession,
    image: Image,
    ctx: Arc<PipelineContext>,
) -> anyhow::Result<Content> {
    session.fire_trigger("on_generation_start");
    let modifiers = session.modifiers.display_active("|");

    // Async DB save (does not block CPU)
    ctx.app_state.session_manager.save_session(&session).await?;

    let rarity_time = Instant::now();
    let rarity = RarityManager::global()
        .pick_for_session(&session)
        .ok_or_else(|| anyhow::anyhow!("Could not generate rarity"))?;
    let rarity_time = rarity_time.elapsed();

    let text_time = Instant::now();
    let text = generate_rarity_text(rarity.as_ref(), ctx.app_state.composer.clone()).await?;
    let text_time = text_time.elapsed();

    let image_time = Instant::now();
    let image = {
        let rarity = rarity.clone();
        let text = text.clone();

        // High-priority compute permit: can use any available core immediately
        let _permit = ctx.limiter.acquire_foreground().await;

        // Render composite on Rayon pool (bypasses Tokio's runtime threads)
        run_cpu(move || process_image(rarity.as_ref(), image, text, modifiers)).await?
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
