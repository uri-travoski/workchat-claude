//! Attachment bytes for the webview. The UI uses plain `<img src="wcmedia://localhost/<id>">`; this module answers those
//! requests from the on-disk cache, downloading with the account's token when needed. Videos and audio get HTTP range
//! support so seeking works, and an uncached range request is proxied from the server without downloading the whole file.

use crate::engine::Engine;
use futures_util::StreamExt;
use reqwest::Method;
use std::collections::HashMap;
use std::sync::{Arc, Mutex, OnceLock};
use tokio::io::{AsyncReadExt, AsyncSeekExt, AsyncWriteExt};

pub struct MediaResp {
    pub status: u16,
    pub content_type: String,
    pub content_range: Option<String>,
    pub body: Vec<u8>,
}

impl MediaResp {
    fn err(status: u16, msg: &str) -> MediaResp {
        MediaResp { status, content_type: "text/plain".into(), content_range: None, body: msg.as_bytes().to_vec() }
    }
}

/// Largest slice answered for one range request that is not in the cache.
const PROXY_CHUNK: u64 = 4 * 1024 * 1024;

fn locks() -> &'static Mutex<HashMap<String, Arc<tokio::sync::Mutex<()>>>> {
    static L: OnceLock<Mutex<HashMap<String, Arc<tokio::sync::Mutex<()>>>>> = OnceLock::new();
    L.get_or_init(|| Mutex::new(HashMap::new()))
}

/// `bytes=a-b`, `bytes=a-`, `bytes=-n` against a known size. Returns inclusive `(start, end)`.
pub fn parse_range(h: &str, total: u64) -> Option<(u64, u64)> {
    let spec = h.trim().strip_prefix("bytes=")?;
    let spec = spec.split(',').next()?.trim();
    let (a, b) = spec.split_once('-')?;
    if total == 0 {
        return None;
    }
    if a.is_empty() {
        let n: u64 = b.parse().ok()?;
        if n == 0 {
            return None;
        }
        return Some((total.saturating_sub(n), total - 1));
    }
    let start: u64 = a.parse().ok()?;
    let end: u64 = if b.is_empty() { total - 1 } else { b.parse::<u64>().ok()?.min(total - 1) };
    if start > end || start >= total {
        return None;
    }
    Some((start, end))
}

fn valid_id(id: &str) -> bool {
    !id.is_empty() && id.len() <= 128 && id.chars().all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
}

pub async fn serve(engine: &Engine, id: &str, thumb: bool, range: Option<&str>) -> MediaResp {
    if !valid_id(id) {
        return MediaResp::err(400, "bad id");
    }
    // Files that only exist on this device: staged picks, files of unsent messages, local avatars.
    if let Some((ct, path)) = engine.local_path(id) {
        return serve_file(&path.display().to_string(), &ct, 0, range).await.unwrap_or_else(|| MediaResp::err(404, "file not found"));
    }
    let key = if thumb { format!("{id}:thumb") } else { id.to_string() };
    if let Some((ct, size, path)) = engine.store.media_get(&key) {
        if let Some(r) = serve_file(&path, &ct, size as u64, range).await {
            return r;
        }
    }
    if !thumb {
        if let Some(r) = range {
            return proxy_range(engine, id, r).await;
        }
    }
    let lock = locks().lock().unwrap_or_else(|e| e.into_inner()).entry(key.clone()).or_default().clone();
    let _guard = lock.lock().await;
    // Another request may have finished the download while this one waited.
    if let Some((ct, size, path)) = engine.store.media_get(&key) {
        if let Some(r) = serve_file(&path, &ct, size as u64, range).await {
            return r;
        }
    }
    match download(engine, id, thumb, &key).await {
        Ok((ct, size, path)) => serve_file(&path, &ct, size, range).await.unwrap_or_else(|| MediaResp::err(500, "cache read failed")),
        Err(e) => {
            let code = match &e {
                crate::error::CoreError::Api { code, .. } => code.http_status(),
                crate::error::CoreError::NotSignedIn => 401,
                _ => 502,
            };
            MediaResp::err(code, &e.to_string())
        }
    }
}

/// Makes sure the attachment is in the on-disk cache (downloading it if needed) and returns `(content type, size, path)`.
pub async fn ensure_cached(engine: &Engine, id: &str) -> crate::error::Result<(String, u64, String)> {
    if !valid_id(id) {
        return Err(crate::error::CoreError::local("Bad file id."));
    }
    if let Some((ct, size, path)) = engine.store.media_get(id) {
        if std::fs::metadata(&path).map(|m| m.len() == size as u64 || size == 0).unwrap_or(false) {
            return Ok((ct, size as u64, path));
        }
    }
    let lock = locks().lock().unwrap_or_else(|e| e.into_inner()).entry(id.to_string()).or_default().clone();
    let _guard = lock.lock().await;
    if let Some((ct, size, path)) = engine.store.media_get(id) {
        if std::fs::metadata(&path).map(|m| m.len() == size as u64 || size == 0).unwrap_or(false) {
            return Ok((ct, size as u64, path));
        }
    }
    download(engine, id, false, id).await
}

async fn download(engine: &Engine, id: &str, thumb: bool, key: &str) -> crate::error::Result<(String, u64, String)> {
    let api = engine.api()?;
    let path = if thumb { format!("/files/{id}/thumb") } else { format!("/files/{id}") };
    let resp = api.send(Method::GET, &path, &[], None, &[]).await?;
    let ct = resp.headers().get("content-type").and_then(|v| v.to_str().ok()).unwrap_or("application/octet-stream").to_string();
    let dir = engine.media_dir();
    tokio::fs::create_dir_all(&dir).await?;
    let fname = key.replace(':', ".");
    let final_path = dir.join(&fname);
    let part = dir.join(format!("{fname}.part"));
    let mut f = tokio::fs::File::create(&part).await?;
    let mut stream = resp.bytes_stream();
    let mut size = 0u64;
    while let Some(chunk) = stream.next().await {
        let c = chunk.map_err(|e| crate::error::CoreError::Network(e.to_string()))?;
        size += c.len() as u64;
        f.write_all(&c).await?;
    }
    f.flush().await?;
    drop(f);
    tokio::fs::rename(&part, &final_path).await?;
    let p = final_path.display().to_string();
    engine.store.media_put(key, &ct, size as i64, &p)?;
    Ok((ct, size, p))
}

async fn serve_file(path: &str, ct: &str, size: u64, range: Option<&str>) -> Option<MediaResp> {
    let mut f = tokio::fs::File::open(path).await.ok()?;
    let real = f.metadata().await.ok()?.len();
    if real != size && size != 0 {
        return None;
    }
    let (start, end, partial) = match range.and_then(|r| parse_range(r, real)) {
        Some((s, e)) => (s, e, true),
        None => (0, real.saturating_sub(1), false),
    };
    let len = if real == 0 { 0 } else { end - start + 1 };
    let mut body = vec![0u8; len as usize];
    if len > 0 {
        f.seek(std::io::SeekFrom::Start(start)).await.ok()?;
        f.read_exact(&mut body).await.ok()?;
    }
    Some(MediaResp { status: if partial { 206 } else { 200 }, content_type: ct.to_string(), content_range: partial.then(|| format!("bytes {start}-{end}/{real}")), body })
}

async fn proxy_range(engine: &Engine, id: &str, range: &str) -> MediaResp {
    let Some(spec) = range.trim().strip_prefix("bytes=") else { return MediaResp::err(416, "bad range") };
    let Some((a, b)) = spec.split(',').next().unwrap_or("").trim().split_once('-') else { return MediaResp::err(416, "bad range") };
    // Cap open-ended and oversized ranges so one request never pulls a huge file into memory.
    let forwarded = match (a.parse::<u64>(), b.parse::<u64>()) {
        (Ok(s), Ok(e)) => format!("bytes={s}-{}", e.min(s + PROXY_CHUNK - 1)),
        (Ok(s), Err(_)) if b.is_empty() => format!("bytes={s}-{}", s + PROXY_CHUNK - 1),
        (Err(_), Ok(n)) if a.is_empty() => format!("bytes=-{}", n.min(PROXY_CHUNK)),
        _ => return MediaResp::err(416, "bad range"),
    };
    let api = match engine.api() {
        Ok(a) => a,
        Err(_) => return MediaResp::err(401, "not signed in"),
    };
    match api.send(Method::GET, &format!("/files/{id}"), &[], None, &[("Range", forwarded)]).await {
        Ok(resp) => {
            let status = resp.status().as_u16();
            let ct = resp.headers().get("content-type").and_then(|v| v.to_str().ok()).unwrap_or("application/octet-stream").to_string();
            let cr = resp.headers().get("content-range").and_then(|v| v.to_str().ok()).map(str::to_string);
            match resp.bytes().await {
                Ok(b) => MediaResp { status, content_type: ct, content_range: cr, body: b.to_vec() },
                Err(e) => MediaResp::err(502, &e.to_string()),
            }
        }
        Err(e) => MediaResp::err(502, &e.to_string()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ranges() {
        assert_eq!(parse_range("bytes=0-9", 100), Some((0, 9)));
        assert_eq!(parse_range("bytes=10-", 100), Some((10, 99)));
        assert_eq!(parse_range("bytes=-5", 100), Some((95, 99)));
        assert_eq!(parse_range("bytes=90-500", 100), Some((90, 99)));
        assert_eq!(parse_range("bytes=100-", 100), None);
        assert_eq!(parse_range("bytes=5-2", 100), None);
        assert_eq!(parse_range("items=1-2", 100), None);
        assert_eq!(parse_range("bytes=0-1", 0), None);
    }

    #[test]
    fn ids() {
        assert!(valid_id("01a1fdaeab3e70bf"));
        assert!(!valid_id("../etc/passwd"));
        assert!(!valid_id(""));
    }

    #[tokio::test]
    async fn serves_ranges_from_disk() {
        let d = tempfile::tempdir().unwrap();
        let p = d.path().join("f");
        std::fs::write(&p, b"0123456789").unwrap();
        let r = serve_file(p.to_str().unwrap(), "text/plain", 10, Some("bytes=2-4")).await.unwrap();
        assert_eq!((r.status, r.body.as_slice(), r.content_range.as_deref()), (206, &b"234"[..], Some("bytes 2-4/10")));
        let r = serve_file(p.to_str().unwrap(), "text/plain", 10, None).await.unwrap();
        assert_eq!((r.status, r.body.len()), (200, 10));
    }
}
