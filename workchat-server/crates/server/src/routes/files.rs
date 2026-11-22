//! Attachment upload (streamed) and download (members only, signed URLs, range support).

use crate::auth::*;
use crate::db::{new_id, now_ms};
use crate::error::{AppError, AppResult};
use crate::queries::*;
use crate::state::St;
use crate::storage::attachment_key;
use axum::body::Body;
use axum::extract::{Path, Query, State};
use axum::http::{header, HeaderMap, HeaderValue, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::Json;
use futures_util::StreamExt;
use hmac::{Hmac, Mac};
use proto::*;
use rusqlite::{params, Connection, OptionalExtension};
use serde::Deserialize;
use sha2::{Digest, Sha256};
use tokio::io::AsyncWriteExt;

type HmacSha256 = Hmac<Sha256>;

#[derive(Deserialize)]
pub struct UploadQuery {
    pub name: Option<String>,
    pub purpose: Option<String>,
}

pub fn safe_name(n: &str) -> String {
    let base = n.rsplit(['/', '\\']).next().unwrap_or("file");
    let cleaned: String = base.chars().filter(|c| !c.is_control()).take(200).collect();
    let cleaned = cleaned.trim().to_string();
    if cleaned.is_empty() || cleaned == "." || cleaned == ".." {
        "file".into()
    } else {
        cleaned
    }
}

pub async fn upload(
    State(st): State<St>,
    auth: Auth,
    Query(q): Query<UploadQuery>,
    headers: HeaderMap,
    body: Body,
) -> AppResult<(StatusCode, Json<UploadResponse>)> {
    upload_stream(&st, &auth.user_id, q, headers, body).await
}

pub async fn upload_stream(
    st: &St,
    uploader: &str,
    q: UploadQuery,
    headers: HeaderMap,
    body: Body,
) -> AppResult<(StatusCode, Json<UploadResponse>)> {
    if st.migrating.load(std::sync::atomic::Ordering::SeqCst) {
        return Err(AppError::conflict("Files are being moved to another storage. Uploads are paused; try again in a few minutes."));
    }
    let purpose = match q.purpose.as_deref() {
        Some("avatar") => "avatar",
        Some("avatar_private") => "avatar_private",
        _ => "file",
    };
    let is_avatar = purpose.starts_with("avatar");
    let max = if is_avatar { 8 * 1024 * 1024 } else { st.cfg.max_upload_bytes };
    if let Some(len) = headers.get(header::CONTENT_LENGTH).and_then(|v| v.to_str().ok()).and_then(|v| v.parse::<u64>().ok()) {
        if len > max {
            return Err(AppError::new(ErrorCode::FileTooLarge, format!("file exceeds the {} MB limit", max / 1024 / 1024)));
        }
    }
    let name = safe_name(q.name.as_deref().unwrap_or("file"));
    let ct = headers
        .get(header::CONTENT_TYPE)
        .and_then(|v| v.to_str().ok())
        .map(|s| s.split(';').next().unwrap_or("").trim().to_lowercase())
        .filter(|s| !s.is_empty() && s != "application/x-www-form-urlencoded" && s != "application/octet-stream")
        .unwrap_or_else(|| mime_guess::from_path(&name).first_or_octet_stream().to_string());
    if is_avatar && !ct.starts_with("image/") {
        return Err(AppError::validation("profile photos must be images"));
    }
    let tmp_dir = st.cfg.data_dir.join("tmp");
    tokio::fs::create_dir_all(&tmp_dir).await.map_err(|e| AppError::internal(e.to_string()))?;
    let id = new_id();
    let tmp = tmp_dir.join(&id);
    let mut file = tokio::fs::File::create(&tmp).await.map_err(|e| AppError::internal(e.to_string()))?;
    let mut hasher = Sha256::new();
    let mut size: u64 = 0;
    let mut stream = body.into_data_stream();
    let result: AppResult<()> = async {
        while let Some(chunk) = stream.next().await {
            let chunk = chunk.map_err(|e| AppError::validation(format!("upload interrupted: {e}")))?;
            size += chunk.len() as u64;
            if size > max {
                return Err(AppError::new(ErrorCode::FileTooLarge, format!("file exceeds the {} MB limit", max / 1024 / 1024)));
            }
            hasher.update(&chunk);
            file.write_all(&chunk).await.map_err(|e| AppError::internal(e.to_string()))?;
        }
        file.flush().await.map_err(|e| AppError::internal(e.to_string()))?;
        Ok(())
    }
    .await;
    drop(file);
    if let Err(e) = result {
        let _ = tokio::fs::remove_file(&tmp).await;
        return Err(e);
    }
    if size == 0 {
        let _ = tokio::fs::remove_file(&tmp).await;
        return Err(AppError::validation("empty file"));
    }
    let sha = hex::encode(hasher.finalize());
    let att = finalize_upload(st, uploader, &id, &name, &ct, purpose, &tmp, size, &sha).await?;
    Ok((StatusCode::CREATED, Json(UploadResponse { attachment: att })))
}

/// Moves a finished temp file into the storage backend and records the attachment.
#[allow(clippy::too_many_arguments)]
pub async fn finalize_upload(
    st: &St,
    uploader: &str,
    id: &str,
    name: &str,
    ct: &str,
    purpose: &str,
    tmp: &std::path::Path,
    size: u64,
    sha: &str,
) -> AppResult<Attachment> {
    let key = attachment_key(id);
    let put = st.storage().put_file(&key, tmp, ct).await;
    let _ = tokio::fs::remove_file(tmp).await;
    put?;
    let att = Attachment { id: id.to_string(), name: name.to_string(), content_type: ct.to_string(), size: size as i64, removed: false, meta: None };
    let (a2, uid, sha, purpose) = (att.clone(), uploader.to_string(), sha.to_string(), purpose.to_string());
    st.db
        .call(move |c| {
            c.execute(
                "INSERT INTO attachments(id,uploader_id,name,content_type,size,sha256,storage_key,purpose,created_ms) VALUES(?1,?2,?3,?4,?5,?6,?7,?8,?9)",
                params![a2.id, uid, a2.name, a2.content_type, a2.size, sha, key, purpose, now_ms()],
            )?;
            Ok(())
        })
        .await?;
    Ok(att)
}

/// M-30/M-31: the uploader attaches media details (duration, waveform peaks, poster frame) to a file of theirs.
pub async fn set_meta(State(st): State<St>, auth: Auth, Path(id): Path<String>, body: axum::body::Bytes) -> AppResult<Json<serde_json::Value>> {
    if body.len() > proto::AttachmentMeta::MAX_JSON_BYTES {
        return Err(AppError::validation("attachment metadata is too large"));
    }
    let meta: proto::AttachmentMeta = serde_json::from_slice(&body).map_err(|e| AppError::validation(format!("bad metadata: {e}")))?;
    if !meta.valid() {
        return Err(AppError::validation("attachment metadata is out of range"));
    }
    let json = serde_json::to_string(&meta)?;
    st.db
        .call(move |c| {
            let row = att_row(c, &id)?;
            if row.uploader != auth.user_id {
                return Err(AppError::forbidden("only the uploader can change a file's details"));
            }
            c.execute("UPDATE attachments SET meta=?2 WHERE id=?1", params![id, json])?;
            Ok(())
        })
        .await?;
    Ok(Json(serde_json::json!({"ok": true})))
}

/// F-03: the first page of a PDF as a PNG. Drawn once on first request and kept next to the file; a PDF that cannot be
/// drawn leaves a marker so it is not tried again.
pub async fn thumbnail(State(st): State<St>, auth: Auth, Path(id): Path<String>) -> AppResult<Response> {
    let (uid, admin, id2) = (auth.user_id.clone(), auth.is_admin(), id.clone());
    let row = st
        .db
        .call(move |c| {
            let row = att_row(c, &id2)?;
            if !can_access(c, &id2, &row, &uid, admin)? {
                return Err(AppError::not_found("file not found"));
            }
            Ok(row)
        })
        .await?;
    if row.removed || row.content_type != "application/pdf" {
        return Err(AppError::not_found("no preview"));
    }
    let (key, none_key) = (format!("thumb/{id}.png"), format!("thumb/{id}.none"));
    let png = if st.storage().head(&key).await?.is_some() {
        st.storage().get_all(&key).await?
    } else if st.storage().head(&none_key).await?.is_some() || row.size as u64 > crate::pdfthumb::MAX_PDF_BYTES {
        return Err(AppError::not_found("no preview"));
    } else {
        static GATE: tokio::sync::Semaphore = tokio::sync::Semaphore::const_new(2);
        let _permit = GATE.acquire().await.map_err(|_| AppError::internal("busy"))?;
        let pdf = st.storage().get_all(&row.key).await?;
        let rendered = tokio::task::spawn_blocking(move || crate::pdfthumb::first_page_png(pdf.to_vec())).await.ok().flatten();
        match rendered {
            Some(p) => {
                let b = bytes::Bytes::from(p);
                let _ = st.storage().put_bytes(&key, b.clone(), "image/png").await;
                b
            }
            None => {
                let _ = st.storage().put_bytes(&none_key, bytes::Bytes::from_static(b"x"), "text/plain").await;
                return Err(AppError::not_found("no preview"));
            }
        }
    };
    let mut resp = Response::new(Body::from(png));
    let h = resp.headers_mut();
    h.insert(header::CONTENT_TYPE, HeaderValue::from_static("image/png"));
    h.insert("x-content-type-options", HeaderValue::from_static("nosniff"));
    h.insert(header::CACHE_CONTROL, HeaderValue::from_static("private, max-age=86400"));
    Ok(resp)
}

pub struct AttRow {
    pub key: String,
    pub content_type: String,
    pub name: String,
    pub size: i64,
    pub removed: bool,
    pub uploader: String,
    pub purpose: String,
}

pub fn att_row(conn: &Connection, id: &str) -> AppResult<AttRow> {
    conn.query_row(
        "SELECT storage_key, content_type, name, size, removed, uploader_id, purpose FROM attachments WHERE id=?1",
        [id],
        |r| {
            Ok(AttRow {
                key: r.get(0)?,
                content_type: r.get(1)?,
                name: r.get(2)?,
                size: r.get(3)?,
                removed: r.get::<_, i64>(4)? != 0,
                uploader: r.get(5)?,
                purpose: r.get(6)?,
            })
        },
    )
    .optional()?
    .ok_or_else(|| AppError::not_found("file not found"))
}

/// Members of a conversation with a visible message that references the file, the uploader
/// (before attaching), anyone for avatars, and admins.
pub fn can_access(conn: &Connection, id: &str, row: &AttRow, user_id: &str, admin: bool) -> AppResult<bool> {
    // a private picture (chosen by one person for another user or a chat) is readable only by the person who chose it.
    if row.purpose == "avatar_private" {
        return Ok(row.uploader == user_id);
    }
    if admin || row.purpose == "avatar" {
        return Ok(true);
    }
    if row.uploader == user_id {
        // The uploader may fetch the file only while it is not attached to any message yet.
        let attached: i64 = conn.query_row("SELECT COUNT(*) FROM message_attachments WHERE attachment_id=?1", [id], |r| r.get(0))?;
        if attached == 0 {
            return Ok(true);
        }
    }
    let vis = visible_sql("?2");
    let n: i64 = conn.query_row(
        &format!(
            "SELECT COUNT(*) FROM message_attachments ma JOIN messages m ON m.id=ma.message_id
             JOIN members mem ON mem.conv_id=m.conv_id AND mem.user_id=?2
             WHERE ma.attachment_id=?1 AND {vis}"
        ),
        params![id, user_id],
        |r| r.get(0),
    )?;
    Ok(n > 0)
}

fn sign(secret: &[u8], id: &str, user: &str, exp: i64) -> String {
    let mut mac = HmacSha256::new_from_slice(secret).expect("hmac key");
    mac.update(format!("file|{id}|{user}|{exp}").as_bytes());
    hex::encode(mac.finalize().into_bytes())
}

#[derive(serde::Serialize)]
pub struct SignedUrl {
    pub url: String,
    pub expires_ms: i64,
}

pub async fn signed_url(State(st): State<St>, auth: Auth, Path(id): Path<String>) -> AppResult<Json<SignedUrl>> {
    let (uid, admin) = (auth.user_id.clone(), auth.is_admin());
    let id2 = id.clone();
    st.db
        .call(move |c| {
            let row = att_row(c, &id2)?;
            if !can_access(c, &id2, &row, &uid, admin)? {
                return Err(AppError::not_found("file not found"));
            }
            Ok(())
        })
        .await?;
    let exp = now_ms() + 10 * 60 * 1000;
    let sig = sign(&st.secret, &id, &auth.user_id, exp);
    Ok(Json(SignedUrl {
        url: format!("{}/files/{}?u={}&exp={}&sig={}", API_PREFIX, id, auth.user_id, exp, sig),
        expires_ms: exp,
    }))
}

#[derive(Deserialize)]
pub struct DownloadQuery {
    u: Option<String>,
    exp: Option<i64>,
    sig: Option<String>,
    /// `1` forces a download instead of inline display.
    dl: Option<String>,
}

fn parse_range(h: &str, size: u64) -> Option<(u64, u64)> {
    let spec = h.strip_prefix("bytes=")?;
    let (a, b) = spec.split_once('-')?;
    if size == 0 {
        return None;
    }
    if a.is_empty() {
        let n: u64 = b.parse().ok()?;
        let n = n.min(size);
        return Some((size - n, size));
    }
    let start: u64 = a.parse().ok()?;
    let end: u64 = if b.is_empty() { size } else { b.parse::<u64>().ok()?.saturating_add(1).min(size) };
    if start >= size || end <= start {
        return None;
    }
    Some((start, end))
}

pub async fn download(
    State(st): State<St>,
    Path(id): Path<String>,
    Query(q): Query<DownloadQuery>,
    headers: HeaderMap,
) -> AppResult<Response> {
    // Authenticate by bearer token or by signed URL.
    let (user_id, admin) = if let (Some(u), Some(exp), Some(sig)) = (&q.u, q.exp, &q.sig) {
        if exp < now_ms() || sign(&st.secret, &id, u, exp) != *sig {
            return Err(AppError::new(ErrorCode::Unauthorized, "link expired or invalid"));
        }
        let u2 = u.clone();
        let (disabled, admin) = st
            .db
            .call(move |c| {
                let r = get_user(c, &u2)?;
                Ok((r.disabled || r.deleted, r.role.is_admin()))
            })
            .await?;
        if disabled {
            return Err(AppError::new(ErrorCode::AccountDisabled, "account disabled"));
        }
        (u.clone(), admin)
    } else {
        let token = headers
            .get(header::AUTHORIZATION)
            .and_then(|v| v.to_str().ok())
            .and_then(|v| v.strip_prefix("Bearer "))
            .map(str::trim)
            .map(str::to_string)
            .ok_or_else(|| AppError::new(ErrorCode::Unauthorized, "missing bearer token"))?;
        let a = st.db.call(move |c| authenticate(c, &token)).await?;
        if !st.limiter.check(&a.user_id) {
            return Err(AppError::new(ErrorCode::RateLimited, "too many requests"));
        }
        (a.user_id.clone(), a.is_admin())
    };
    serve_attachment(&st, &id, &user_id, admin, &headers, q.dl.as_deref() == Some("1")).await
}

/// Streams an attachment (range aware) after the access check.
pub async fn serve_attachment(st: &St, id: &str, user_id: &str, admin: bool, headers: &HeaderMap, force_download: bool) -> AppResult<Response> {
    let (id2, uid2) = (id.to_string(), user_id.to_string());
    let row = st
        .db
        .call(move |c| {
            let row = att_row(c, &id2)?;
            if !can_access(c, &id2, &row, &uid2, admin)? {
                return Err(AppError::not_found("file not found"));
            }
            Ok(row)
        })
        .await?;
    if row.removed {
        return Err(AppError::not_found("File removed"));
    }
    let range = headers
        .get(header::RANGE)
        .and_then(|v| v.to_str().ok())
        .and_then(|h| parse_range(h, row.size as u64));
    let data = st.storage().get(&row.key, range).await?;
    let partial = range.is_some();
    let mut resp = Response::new(Body::from_stream(data.stream));
    *resp.status_mut() = if partial { StatusCode::PARTIAL_CONTENT } else { StatusCode::OK };
    let h = resp.headers_mut();
    let ct = &row.content_type;
    // Never let uploaded active content run in the origin's context.
    let inline_ok = (ct.starts_with("image/") && !ct.contains("svg"))
        || ct.starts_with("video/")
        || ct.starts_with("audio/")
        || ct == "application/pdf";
    let served_ct = if inline_ok { ct.as_str() } else { "application/octet-stream" };
    h.insert(header::CONTENT_TYPE, HeaderValue::from_str(served_ct).unwrap_or(HeaderValue::from_static("application/octet-stream")));
    let disp = if inline_ok && !force_download { "inline" } else { "attachment" };
    let fname: String = row.name.chars().map(|c| if c == '"' || c == '\\' || c.is_control() { '_' } else { c }).collect();
    let encoded: String = row.name.bytes().map(|b| if b.is_ascii_alphanumeric() || b"-._~".contains(&b) { (b as char).to_string() } else { format!("%{b:02X}") }).collect();
    if let Ok(v) = HeaderValue::from_str(&format!("{disp}; filename=\"{fname}\"; filename*=UTF-8''{encoded}")) {
        h.insert(header::CONTENT_DISPOSITION, v);
    }
    h.insert("x-content-type-options", HeaderValue::from_static("nosniff"));
    h.insert("content-security-policy", HeaderValue::from_static("sandbox; default-src 'none'"));
    h.insert(header::ACCEPT_RANGES, HeaderValue::from_static("bytes"));
    h.insert(header::CACHE_CONTROL, HeaderValue::from_static("private, max-age=3600"));
    h.insert(header::CONTENT_LENGTH, HeaderValue::from(data.end - data.start));
    if partial {
        if let Ok(v) = HeaderValue::from_str(&format!("bytes {}-{}/{}", data.start, data.end.saturating_sub(1), data.size)) {
            h.insert(header::CONTENT_RANGE, v);
        }
    }
    Ok(resp.into_response())
}
