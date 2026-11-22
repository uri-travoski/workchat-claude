//! Attachment storage behind one trait: local disk, S3-compatible (AWS S3, Cloudflare R2,
//! Backblaze B2, MinIO) and Google Drive.

mod gdrive;
mod objstore;

use crate::error::{AppError, AppResult};
use async_trait::async_trait;
use bytes::Bytes;
use futures_util::Stream;
use proto::StorageConfig;
use std::path::{Path, PathBuf};
use std::pin::Pin;
use std::sync::Arc;

pub type ByteStream = Pin<Box<dyn Stream<Item = std::io::Result<Bytes>> + Send>>;

pub struct ObjectData {
    pub stream: ByteStream,
    /// Total object size.
    pub size: u64,
    /// Returned byte range `[start, end)`.
    pub start: u64,
    pub end: u64,
}

#[async_trait]
pub trait Storage: Send + Sync {
    fn kind(&self) -> String;
    async fn put_file(&self, key: &str, path: &Path, content_type: &str) -> AppResult<()>;
    async fn put_bytes(&self, key: &str, data: Bytes, content_type: &str) -> AppResult<()>;
    /// `range` is `[start, end)` in bytes.
    async fn get(&self, key: &str, range: Option<(u64, u64)>) -> AppResult<ObjectData>;
    /// Size of the object, or `None` if it does not exist.
    async fn head(&self, key: &str) -> AppResult<Option<u64>>;
    async fn delete(&self, key: &str) -> AppResult<()>;
    /// Moves an object inside the same backend (used for the backup folder).
    async fn rename(&self, from: &str, to: &str) -> AppResult<()>;

    async fn get_all(&self, key: &str) -> AppResult<Bytes> {
        use futures_util::StreamExt;
        let mut data = self.get(key, None).await?;
        let mut out = Vec::with_capacity(data.size as usize);
        while let Some(chunk) = data.stream.next().await {
            out.extend_from_slice(&chunk.map_err(|e| AppError::storage(format!("read {key}: {e}")))?);
        }
        Ok(Bytes::from(out))
    }

    /// Writes, reads back and deletes a small probe object.
    async fn check(&self) -> AppResult<String> {
        let key = format!("probe/{}", crate::db::new_id());
        let payload = Bytes::from(format!("workchat probe {}", crate::db::now_ms()));
        self.put_bytes(&key, payload.clone(), "text/plain").await?;
        let back = self.get_all(&key).await?;
        let size = self.head(&key).await?;
        self.delete(&key).await?;
        if back != payload {
            return Err(AppError::storage("probe read back different bytes"));
        }
        if size != Some(payload.len() as u64) {
            return Err(AppError::storage("probe size mismatch"));
        }
        if self.head(&key).await?.is_some() {
            return Err(AppError::storage("probe object was not deleted"));
        }
        Ok(format!("write, read, size and delete OK on {}", self.kind()))
    }
}

pub fn build_storage(cfg: &StorageConfig, data_dir: &Path) -> AppResult<Arc<dyn Storage>> {
    match cfg {
        StorageConfig::Local { path } => {
            let dir: PathBuf = path
                .as_ref()
                .filter(|p| !p.trim().is_empty())
                .map(PathBuf::from)
                .unwrap_or_else(|| data_dir.join("files"));
            Ok(Arc::new(objstore::ObjStore::local(&dir)?))
        }
        StorageConfig::S3 { .. } => Ok(Arc::new(objstore::ObjStore::s3(cfg)?)),
        StorageConfig::Gdrive { .. } => Ok(Arc::new(gdrive::GDrive::new(cfg)?)),
    }
}

/// Object key of a live attachment.
pub fn attachment_key(id: &str) -> String {
    format!("att/{id}")
}

/// Object key of an attachment that was moved into a backup folder.
pub fn backup_attachment_key(backup_prefix: &str, id: &str) -> String {
    format!("{backup_prefix}/att/{id}")
}
