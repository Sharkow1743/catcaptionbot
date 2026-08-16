mod edit;

use std::path::Path;

use bytes::Bytes;
use rand::rng;
use rand::seq::IndexedRandom;
use serde_json::Value;
use sha2::{Digest, Sha256};
use tokio::fs;

pub async fn fetch_image() -> anyhow::Result<Bytes> {
    // Try fetching from API; on ANY failure, fall back to cache
    let bytes = match fetch_from_api().await {
        Ok(b) => b,
        Err(e) => {
            log::warn!("API fetch failed ({e}), falling back to cached image");
            return load_random_cached().await;
        }
    };

    // Cache the successfully fetched image (non-fatal if this fails)
    cache_bytes(&bytes).await;

    Ok(bytes)
}

async fn fetch_from_api() -> anyhow::Result<Bytes> {
    let resp = reqwest::get("https://api.thecatapi.com/v1/images/search")
        .await?
        .json::<Value>()
        .await?;

    let url = resp[0]["url"]
        .as_str()
        .ok_or_else(|| anyhow::anyhow!("Cannot parse url from the api"))?;

    Ok(reqwest::get(url).await?.bytes().await?)
}

async fn cache_bytes(bytes: &Bytes) {
    let cache_dir = Path::new("./.image_cache");
    if let Err(e) = fs::create_dir_all(cache_dir).await {
        log::warn!("Cannot create image cache dir: {e}");
        return;
    }

    let mut hasher = Sha256::new();
    hasher.update(bytes);
    let hash = hex::encode(hasher.finalize());

    let filename = format!("{}.{}", &hash[..16], "jpg");
    let filepath = cache_dir.join(filename);

    if !filepath.exists() {
        if let Err(e) = fs::write(&filepath, bytes).await {
            log::warn!("Cannot write image to cache: {e}");
        }
    }
}

async fn load_random_cached() -> anyhow::Result<Bytes> {
    let cache_dir = Path::new("./.image_cache");
    let mut entries = fs::read_dir(cache_dir).await?;

    let mut files = Vec::new();
    while let Some(entry) = entries.next_entry().await? {
        let path = entry.path();
        if path.is_file() {
            files.push(path);
        }
    }

    let chosen = files
        .choose(&mut rng())
        .ok_or_else(|| anyhow::anyhow!("No cached images available for fallback"))?;

    log::info!("Serving fallback from cache: {}", chosen.display());
    Ok(fs::read(chosen).await.map(Bytes::from)?)
}
