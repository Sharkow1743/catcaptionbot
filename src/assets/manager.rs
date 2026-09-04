use std::collections::HashMap;
use std::sync::{Arc, RwLock};

use bytes::Bytes;
use teloxide::types::FileId;

use super::cached_image::CachedImage;

#[derive(Debug, Clone)]
struct AssetEntry {
    bytes: Bytes,
    file_name: Option<String>,
    file_id: Option<FileId>,
}

/// Generic, thread-safe manager for static assets and their cached remote FileIds.
#[derive(Debug, Clone, Default)]
pub struct AssetManager {
    assets: Arc<RwLock<HashMap<String, AssetEntry>>>,
}

impl AssetManager {
    pub fn new() -> Self {
        Self {
            assets: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    /// Registers an asset with memory bytes and optional filename.
    pub fn register(
        &self,
        key: impl Into<String>,
        bytes: impl Into<Bytes>,
        file_name: impl Into<Option<String>>,
    ) -> &Self {
        let mut guard = self.assets.write().unwrap();
        guard.insert(
            key.into(),
            AssetEntry {
                bytes: bytes.into(),
                file_name: file_name.into(),
                file_id: None,
            },
        );
        self
    }

    /// Helper for static byte slices (`include_bytes!`).
    pub fn register_static(
        &self,
        key: impl Into<String>,
        bytes: &'static [u8],
        file_name: impl Into<String>,
    ) -> &Self {
        self.register(key, Bytes::from_static(bytes), Some(file_name.into()))
    }

    /// Retrieves an asset. Returns a `CachedImage` backed by `FileId` if cached,
    /// or raw bytes if not yet uploaded.
    pub fn get(&self, key: &str) -> Option<CachedImage> {
        let guard = self.assets.read().unwrap();
        let asset = guard.get(key)?;
        if let Some(ref file_id) = asset.file_id {
            Some(CachedImage::from_file_id(file_id.clone()))
        } else if let Some(ref name) = asset.file_name {
            Some(CachedImage::from_bytes_named(
                asset.bytes.clone(),
                name.clone(),
            ))
        } else {
            Some(CachedImage::from_bytes(asset.bytes.clone()))
        }
    }

    /// Asserts the existence of an asset and returns its `CachedImage`.
    pub fn require(&self, key: &str) -> CachedImage {
        self.get(key)
            .unwrap_or_else(|| panic!("Asset with key '{key}' is not registered in AssetManager"))
    }

    /// Caches the remote Telegram `FileId` for the given asset.
    pub fn update_file_id(&self, key: &str, file_id: FileId) {
        let mut guard = self.assets.write().unwrap();
        if let Some(asset) = guard.get_mut(key) {
            asset.file_id = Some(file_id);
        }
    }
}
