pub mod edit;

use std::path::Path;

use bytes::Bytes;
use rand::rng;
use rand::seq::IndexedRandom;
use reqwest::Client;
use serde_json::Value;
use sha2::{Digest, Sha256};
use tokio::{fs, spawn};

use crate::detail;

pub async fn fetch_image(client: &Client) -> anyhow::Result<Bytes> {
    // Try fetching from API; on ANY failure, fall back to cache
    let bytes = match fetch_from_api(client).await {
        Ok(b) => {
            log::debug!("Fetched image from API ({} bytes)", b.len());
            b
        }
        Err(e) => {
            log::warn!("API fetch failed ({e}), falling back to cached image");
            return load_random_cached().await;
        }
    };

    // Cache the successfully fetched image (non-fatal if this fails)
    spawn(cache_bytes(bytes.clone()));

    Ok(bytes)
}

async fn fetch_from_api(client: &Client) -> anyhow::Result<Bytes> {
    let t1 = std::time::Instant::now();
    let resp = client
        .get("https://api.thecatapi.com/v1/images/search")
        .send()
        .await?
        .json::<Value>()
        .await?;
    log::debug!("catapi metadata request took {:?}", t1.elapsed());

    let url = resp[0]["url"]
        .as_str()
        .ok_or_else(|| anyhow::anyhow!("Cannot parse url from the api"))?;

    let t2 = std::time::Instant::now();
    let bytes = client.get(url).send().await?.bytes().await?;
    log::debug!(
        "image download request took {:?} ({} bytes)",
        t2.elapsed(),
        bytes.len()
    );

    Ok(bytes)
}

async fn cache_bytes(bytes: Bytes) {
    let cache_dir = Path::new("./.image_cache");
    if let Err(e) = fs::create_dir_all(cache_dir).await {
        log::warn!("Cannot create image cache dir: {e}");
        return;
    }

    let mut hasher = Sha256::new();
    hasher.update(&bytes);
    let hash = hasher.finalize();
    let hash_hex: String = hash.iter().map(|b| format!("{b:02x}")).collect();

    let filename = format!("{}.jpg", &hash_hex[..16]);
    let filepath = cache_dir.join(filename);

    if !filepath.exists() {
        match fs::write(&filepath, bytes).await {
            Ok(()) => log::trace!("Cached image to {}", filepath.display()),
            Err(e) => log::warn!("Cannot write image to cache: {e}"),
        }
    } else {
        log::trace!("Image already cached at {}", filepath.display());
    }
}

async fn load_random_cached() -> anyhow::Result<Bytes> {
    let cache_dir = Path::new("./.image_cache");
    if !cache_dir.exists() {
        fs::create_dir_all(cache_dir).await?;
    }
    let mut entries = fs::read_dir(cache_dir).await?;

    let mut files = Vec::new();
    while let Some(entry) = entries.next_entry().await? {
        let path = entry.path();
        if path.is_file() {
            files.push(path);
        }
    }

    if files.is_empty() {
        log::error!("Fallback image cache is empty, no image can be served");
    }

    let chosen = files
        .choose(&mut rng())
        .ok_or_else(|| anyhow::anyhow!("No cached images available for fallback"))?;

    detail!("Serving fallback from cache: {}", chosen.display());
    Ok(fs::read(chosen).await.map(Bytes::from)?)
}
