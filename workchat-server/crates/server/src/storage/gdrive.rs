//! Google Drive backend (OAuth refresh token, `drive.file` scope). Objects live flat in one
//! application folder; the object key `a/b/c` is stored under the file name `a__b__c`.

use super::{ByteStream, ObjectData, Storage};
use crate::error::{AppError, AppResult};
use async_trait::async_trait;
use bytes::Bytes;
use futures_util::StreamExt;
use proto::StorageConfig;
use serde::Deserialize;
use std::path::Path;
use std::time::{Duration, Instant};
use tokio::sync::Mutex;

pub struct GDrive {
    http: reqwest::Client,
    client_id: String,
    client_secret: String,
    refresh_token: String,
    folder_name: String,
    api_base: String,
    token_url: String,
    token: Mutex<Option<(String, Instant)>>,
    folder: Mutex<Option<String>>,
}

#[derive(Deserialize)]
struct TokenResp {
    access_token: String,
    #[serde(default)]
    expires_in: u64,
}

#[derive(Deserialize)]
struct FileList {
    #[serde(default)]
    files: Vec<DriveFile>,
}

#[derive(Deserialize, Clone)]
struct DriveFile {
    id: String,
    #[serde(default)]
    size: Option<String>,
}

fn serr(e: impl std::fmt::Display) -> AppError {
    AppError::storage(format!("gdrive: {e}"))
}

fn name_of(key: &str) -> String {
    key.replace('/', "__")
}

impl GDrive {
    pub fn new(cfg: &StorageConfig) -> AppResult<Self> {
        let StorageConfig::Gdrive { client_id, client_secret, refresh_token, folder_name, api_base, token_url } = cfg
        else {
            return Err(AppError::validation("not a Google Drive config"));
        };
        Ok(GDrive {
            http: reqwest::Client::builder().timeout(Duration::from_secs(600)).build().map_err(serr)?,
            client_id: client_id.clone(),
            client_secret: client_secret.clone(),
            refresh_token: refresh_token.clone(),
            folder_name: folder_name.clone().filter(|s| !s.is_empty()).unwrap_or_else(|| "WorkChat".into()),
            api_base: api_base
                .clone()
                .filter(|s| !s.is_empty())
                .unwrap_or_else(|| "https://www.googleapis.com".into())
                .trim_end_matches('/')
                .to_string(),
            token_url: token_url
                .clone()
                .filter(|s| !s.is_empty())
                .unwrap_or_else(|| "https://oauth2.googleapis.com/token".into()),
            token: Mutex::new(None),
            folder: Mutex::new(None),
        })
    }

    async fn access_token(&self) -> AppResult<String> {
        let mut g = self.token.lock().await;
        if let Some((t, exp)) = g.as_ref() {
            if Instant::now() < *exp {
                return Ok(t.clone());
            }
        }
        let resp = self
            .http
            .post(&self.token_url)
            .form(&[
                ("client_id", self.client_id.as_str()),
                ("client_secret", self.client_secret.as_str()),
                ("refresh_token", self.refresh_token.as_str()),
                ("grant_type", "refresh_token"),
            ])
            .send()
            .await
            .map_err(serr)?;
        if !resp.status().is_success() {
            return Err(serr(format!("token refresh failed: HTTP {}", resp.status())));
        }
        let t: TokenResp = resp.json().await.map_err(serr)?;
        let ttl = t.expires_in.saturating_sub(60).max(30);
        *g = Some((t.access_token.clone(), Instant::now() + Duration::from_secs(ttl)));
        Ok(t.access_token)
    }

    async fn folder_id(&self) -> AppResult<String> {
        let mut g = self.folder.lock().await;
        if let Some(f) = g.as_ref() {
            return Ok(f.clone());
        }
        let tok = self.access_token().await?;
        let q = format!(
            "mimeType='application/vnd.google-apps.folder' and name='{}' and trashed=false",
            self.folder_name.replace('\'', "\\'")
        );
        let list: FileList = self
            .http
            .get(format!("{}/drive/v3/files", self.api_base))
            .bearer_auth(&tok)
            .query(&[("q", q.as_str()), ("fields", "files(id,name)"), ("spaces", "drive")])
            .send()
            .await
            .map_err(serr)?
            .error_for_status()
            .map_err(serr)?
            .json()
            .await
            .map_err(serr)?;
        let id = if let Some(f) = list.files.first() {
            f.id.clone()
        } else {
            let created: DriveFile = self
                .http
                .post(format!("{}/drive/v3/files", self.api_base))
                .bearer_auth(&tok)
                .query(&[("fields", "id")])
                .json(&serde_json::json!({
                    "name": self.folder_name,
                    "mimeType": "application/vnd.google-apps.folder"
                }))
                .send()
                .await
                .map_err(serr)?
                .error_for_status()
                .map_err(serr)?
                .json()
                .await
                .map_err(serr)?;
            created.id
        };
        *g = Some(id.clone());
        Ok(id)
    }

    async fn find(&self, key: &str) -> AppResult<Option<DriveFile>> {
        let tok = self.access_token().await?;
        let folder = self.folder_id().await?;
        let q = format!(
            "name='{}' and '{}' in parents and trashed=false",
            name_of(key).replace('\'', "\\'"),
            folder
        );
        let list: FileList = self
            .http
            .get(format!("{}/drive/v3/files", self.api_base))
            .bearer_auth(&tok)
            .query(&[("q", q.as_str()), ("fields", "files(id,name,size)"), ("spaces", "drive")])
            .send()
            .await
            .map_err(serr)?
            .error_for_status()
            .map_err(serr)?
            .json()
            .await
            .map_err(serr)?;
        Ok(list.files.into_iter().next())
    }
}

#[async_trait]
impl Storage for GDrive {
    fn kind(&self) -> String {
        "gdrive".into()
    }

    async fn put_file(&self, key: &str, path: &Path, ct: &str) -> AppResult<()> {
        let data = tokio::fs::read(path).await.map_err(serr)?;
        self.put_bytes(key, Bytes::from(data), ct).await
    }

    async fn put_bytes(&self, key: &str, data: Bytes, ct: &str) -> AppResult<()> {
        if self.find(key).await?.is_some() {
            self.delete(key).await?;
        }
        let tok = self.access_token().await?;
        let folder = self.folder_id().await?;
        let boundary = format!("workchat{}", crate::db::new_id());
        let meta = serde_json::json!({"name": name_of(key), "parents": [folder]});
        let mut body: Vec<u8> = Vec::with_capacity(data.len() + 512);
        body.extend_from_slice(
            format!(
                "--{boundary}\r\nContent-Type: application/json; charset=UTF-8\r\n\r\n{meta}\r\n--{boundary}\r\nContent-Type: {ct}\r\n\r\n"
            )
            .as_bytes(),
        );
        body.extend_from_slice(&data);
        body.extend_from_slice(format!("\r\n--{boundary}--").as_bytes());
        self.http
            .post(format!("{}/upload/drive/v3/files", self.api_base))
            .bearer_auth(&tok)
            .query(&[("uploadType", "multipart"), ("fields", "id")])
            .header("Content-Type", format!("multipart/related; boundary={boundary}"))
            .body(body)
            .send()
            .await
            .map_err(serr)?
            .error_for_status()
            .map_err(serr)?;
        Ok(())
    }

    async fn get(&self, key: &str, range: Option<(u64, u64)>) -> AppResult<ObjectData> {
        let f = self.find(key).await?.ok_or_else(|| AppError::not_found(format!("object {key} missing")))?;
        let size: u64 = f.size.as_deref().and_then(|s| s.parse().ok()).unwrap_or(0);
        let tok = self.access_token().await?;
        let mut req = self
            .http
            .get(format!("{}/drive/v3/files/{}", self.api_base, f.id))
            .bearer_auth(&tok)
            .query(&[("alt", "media")]);
        let (start, end) = match range {
            Some((s, e)) => {
                let e = e.min(size);
                req = req.header("Range", format!("bytes={}-{}", s, e.saturating_sub(1)));
                (s, e)
            }
            None => (0, size),
        };
        let resp = req.send().await.map_err(serr)?.error_for_status().map_err(serr)?;
        let stream: ByteStream = Box::pin(resp.bytes_stream().map(|r| r.map_err(std::io::Error::other)));
        Ok(ObjectData { stream, size, start, end })
    }

    async fn head(&self, key: &str) -> AppResult<Option<u64>> {
        Ok(self
            .find(key)
            .await?
            .map(|f| f.size.as_deref().and_then(|s| s.parse().ok()).unwrap_or(0)))
    }

    async fn delete(&self, key: &str) -> AppResult<()> {
        if let Some(f) = self.find(key).await? {
            let tok = self.access_token().await?;
            let r = self
                .http
                .delete(format!("{}/drive/v3/files/{}", self.api_base, f.id))
                .bearer_auth(&tok)
                .send()
                .await
                .map_err(serr)?;
            if !r.status().is_success() && r.status().as_u16() != 404 {
                return Err(serr(format!("delete failed: HTTP {}", r.status())));
            }
        }
        Ok(())
    }

    async fn rename(&self, from: &str, to: &str) -> AppResult<()> {
        let f = self.find(from).await?.ok_or_else(|| AppError::not_found(format!("object {from} missing")))?;
        if self.find(to).await?.is_some() {
            self.delete(to).await?;
        }
        let tok = self.access_token().await?;
        self.http
            .patch(format!("{}/drive/v3/files/{}", self.api_base, f.id))
            .bearer_auth(&tok)
            .json(&serde_json::json!({"name": name_of(to)}))
            .send()
            .await
            .map_err(serr)?
            .error_for_status()
            .map_err(serr)?;
        Ok(())
    }
}
