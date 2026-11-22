use super::{ByteStream, ObjectData, Storage};
use crate::error::{AppError, AppResult};
use async_trait::async_trait;
use bytes::Bytes;
use futures_util::StreamExt;
use object_store::path::Path as OPath;
use object_store::{GetOptions, GetRange, ObjectStore, PutPayload, WriteMultipart};
use proto::StorageConfig;
use std::path::Path;
use std::sync::Arc;
use tokio::io::AsyncReadExt;

const CHUNK: usize = 8 * 1024 * 1024;

pub struct ObjStore {
    store: Arc<dyn ObjectStore>,
    label: String,
    prefix: String,
}

fn serr(e: impl std::fmt::Display) -> AppError {
    AppError::storage(e.to_string())
}

impl ObjStore {
    pub fn local(dir: &Path) -> AppResult<Self> {
        std::fs::create_dir_all(dir).map_err(serr)?;
        let fs = object_store::local::LocalFileSystem::new_with_prefix(dir).map_err(serr)?;
        Ok(ObjStore { store: Arc::new(fs), label: "local".into(), prefix: String::new() })
    }

    pub fn s3(cfg: &StorageConfig) -> AppResult<Self> {
        let StorageConfig::S3 {
            endpoint, region, bucket, access_key_id, secret_access_key, prefix, path_style, allow_http, ..
        } = cfg
        else {
            return Err(AppError::validation("not an S3 config"));
        };
        let mut b = object_store::aws::AmazonS3Builder::new()
            .with_region(region)
            .with_bucket_name(bucket)
            .with_access_key_id(access_key_id)
            .with_secret_access_key(secret_access_key)
            .with_virtual_hosted_style_request(!*path_style);
        if let Some(ep) = endpoint.as_ref().filter(|e| !e.is_empty()) {
            b = b.with_endpoint(ep.trim_end_matches('/'));
        }
        if *allow_http {
            b = b.with_allow_http(true);
        }
        let store = b.build().map_err(serr)?;
        let prefix = prefix.clone().unwrap_or_default().trim_matches('/').to_string();
        Ok(ObjStore { store: Arc::new(store), label: cfg.kind().to_string(), prefix })
    }

    fn path(&self, key: &str) -> OPath {
        if self.prefix.is_empty() {
            OPath::from(key)
        } else {
            OPath::from(format!("{}/{}", self.prefix, key))
        }
    }
}

#[async_trait]
impl Storage for ObjStore {
    fn kind(&self) -> String {
        self.label.clone()
    }

    async fn put_file(&self, key: &str, path: &Path, _ct: &str) -> AppResult<()> {
        let loc = self.path(key);
        let mut f = tokio::fs::File::open(path).await.map_err(serr)?;
        let len = f.metadata().await.map_err(serr)?.len() as usize;
        if len <= CHUNK {
            let mut buf = Vec::with_capacity(len);
            f.read_to_end(&mut buf).await.map_err(serr)?;
            self.store.put(&loc, PutPayload::from(Bytes::from(buf))).await.map_err(serr)?;
            return Ok(());
        }
        let upload = self.store.put_multipart(&loc).await.map_err(serr)?;
        let mut w = WriteMultipart::new_with_chunk_size(upload, CHUNK);
        let mut buf = vec![0u8; 1024 * 1024];
        loop {
            let n = f.read(&mut buf).await.map_err(serr)?;
            if n == 0 {
                break;
            }
            w.wait_for_capacity(4).await.map_err(serr)?;
            w.write(&buf[..n]);
        }
        w.finish().await.map_err(serr)?;
        Ok(())
    }

    async fn put_bytes(&self, key: &str, data: Bytes, _ct: &str) -> AppResult<()> {
        self.store.put(&self.path(key), PutPayload::from(data)).await.map_err(serr)?;
        Ok(())
    }

    async fn get(&self, key: &str, range: Option<(u64, u64)>) -> AppResult<ObjectData> {
        let loc = self.path(key);
        let opts = GetOptions {
            range: range.map(|(s, e)| GetRange::Bounded(s..e)),
            ..Default::default()
        };
        let res = self.store.get_opts(&loc, opts).await.map_err(|e| match e {
            object_store::Error::NotFound { .. } => AppError::not_found(format!("object {key} missing")),
            e => serr(e),
        })?;
        let size = res.meta.size;
        let (start, end) = (res.range.start, res.range.end);
        let stream: ByteStream = Box::pin(
            res.into_stream().map(|r| r.map_err(|e| std::io::Error::other(e.to_string()))),
        );
        Ok(ObjectData { stream, size, start, end })
    }

    async fn head(&self, key: &str) -> AppResult<Option<u64>> {
        match self.store.head(&self.path(key)).await {
            Ok(m) => Ok(Some(m.size)),
            Err(object_store::Error::NotFound { .. }) => Ok(None),
            Err(e) => Err(serr(e)),
        }
    }

    async fn delete(&self, key: &str) -> AppResult<()> {
        match self.store.delete(&self.path(key)).await {
            Ok(()) | Err(object_store::Error::NotFound { .. }) => Ok(()),
            Err(e) => Err(serr(e)),
        }
    }

    async fn rename(&self, from: &str, to: &str) -> AppResult<()> {
        self.store.copy(&self.path(from), &self.path(to)).await.map_err(serr)?;
        self.store.delete(&self.path(from)).await.map_err(serr)?;
        Ok(())
    }
}
