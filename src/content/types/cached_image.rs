use std::sync::{Arc, OnceLock};
use teloxide::types::{FileId, InputFile};

#[derive(Debug, Clone)]
pub struct CachedImage {
    state: CachedImageState,
}

#[derive(Debug, Clone)]
enum CachedImageState {
    Pending {
        bytes: bytes::Bytes,
        file_name: Option<String>,
    },
    Ready(FileId),
}

impl CachedImage {
    pub fn from_bytes(bytes: impl Into<bytes::Bytes>) -> Self {
        Self {
            state: CachedImageState::Pending {
                bytes: bytes.into(),
                file_name: None,
            },
        }
    }

    pub fn from_bytes_named(bytes: impl Into<bytes::Bytes>, name: impl Into<String>) -> Self {
        Self {
            state: CachedImageState::Pending {
                bytes: bytes.into(),
                file_name: Some(name.into()),
            },
        }
    }

    pub fn from_file_id(id: impl Into<FileId>) -> Self {
        Self {
            state: CachedImageState::Ready(id.into()),
        }
    }

    pub fn set_file_id(&mut self, id: impl Into<FileId>) {
        self.state = CachedImageState::Ready(id.into());
    }

    pub fn is_ready(&self) -> bool {
        matches!(self.state, CachedImageState::Ready(_))
    }
}

impl From<CachedImage> for InputFile {
    fn from(img: CachedImage) -> Self {
        match img.state {
            CachedImageState::Ready(file_id) => InputFile::file_id(file_id),
            CachedImageState::Pending { bytes, file_name } => {
                let mut input = InputFile::memory(bytes);
                if let Some(name) = file_name {
                    input = input.file_name(name);
                }
                input
            }
        }
    }
}

impl From<&CachedImage> for InputFile {
    fn from(img: &CachedImage) -> Self {
        match &img.state {
            CachedImageState::Ready(file_id) => InputFile::file_id(file_id.clone()),
            CachedImageState::Pending { bytes, file_name } => {
                let mut input = InputFile::memory(bytes.clone());
                if let Some(name) = file_name {
                    input = input.file_name(name.clone());
                }
                input
            }
        }
    }
}
