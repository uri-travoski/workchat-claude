//! SV-20: encrypted full backups (database snapshot, age passphrase encryption) to local disk,
//! Google Drive, S3, R2 or B2; daily schedule with retention; "back up now"; and restore onto a
//! fresh server that reconnects to the existing attachment storage.
//!
//! What a full backup contains: a consistent SQLite snapshot (`VACUUM INTO`) with accounts,
//! sessions, conversations, messages, settings (including the storage configuration) and the
//! event log. Attachment files are NOT copied: they stay in the attachment storage and the restored
//! database points at them. The passphrase is never stored in the database or in a backup.

use crate::admin::{load_storage_config, STORAGE_KEY};
use crate::auth::Admin;
use crate::crypto;
use crate::db::{get_setting, new_id, now_ms, set_setting};
use crate::error::{AppError, AppResult};
use crate::state::St;
use crate::storage::{build_storage, Storage};
use axum::body::Body;
use axum::extract::{Path, State};
use axum::http::{header, HeaderMap, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::Json;
use bytes::Bytes;
use futures_util::StreamExt;
use proto::*;
use rusqlite::{params, Connection};
use std::path::PathBuf;
use std::sync::Arc;
use tokio::io::AsyncWriteExt;

pub const SETTINGS_KEY: &str = "backup_settings";
const PASS_FILE: &str = "backup.key";

pub fn default_settings() -> BackupSettings {
    BackupSettings { enabled: false, interval_hours: 24, retention: 7, target: None, passphrase_set: false, last_run_ms: None, last_status: String::new() }
}

pub fn load_settings(conn: &Connection) -> AppResult<BackupSettings> {
    Ok(get_setting(conn, SETTINGS_KEY)?.and_then(|s| serde_json::from_str(&s).ok()).unwrap_or_else(default_settings))
}

fn save_settings(conn: &Connection, s: &BackupSettings) -> AppResult<()> {
    let mut s = s.clone();
    s.passphrase_set = false;
    set_setting(conn, SETTINGS_KEY, &serde_json::to_string(&s)?)
}

/// The backup passphrase: `WORKCHAT_BACKUP_PASSPHRASE`, else the 0600 file `<data>/backup.key`.
pub fn passphrase(st: &St) -> Option<String> {
    if let Some(p) = std::env::var("WORKCHAT_BACKUP_PASSPHRASE").ok().filter(|p| !p.is_empty()) {
        return Some(p);
    }
    std::fs::read_to_string(st.cfg.data_dir.join(PASS_FILE)).ok().map(|s| s.trim().to_string()).filter(|s| !s.is_empty())
}

pub fn set_passphrase(st: &St, pass: &str) -> AppResult<()> {
    let p = st.cfg.data_dir.join(PASS_FILE);
    std::fs::write(&p, pass).map_err(|e| AppError::internal(e.to_string()))?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let _ = std::fs::set_permissions(&p, std::fs::Permissions::from_mode(0o600));
    }
    Ok(())
}

pub fn target_storage(st: &St, s: &BackupSettings) -> AppResult<Arc<dyn Storage>> {
    match &s.target {
        Some(cfg) => build_storage(cfg, &st.cfg.data_dir),
        None => Ok(st.storage()),
    }
}

fn bfail(m: impl std::fmt::Display) -> AppError {
    AppError::new(ErrorCode::BackupFailed, format!("backup failed: {m}"))
}

async fn tmp_path(st: &St, suffix: &str) -> AppResult<PathBuf> {
    let dir = st.cfg.data_dir.join("tmp");
    tokio::fs::create_dir_all(&dir).await.map_err(|e| AppError::internal(e.to_string()))?;
    Ok(dir.join(format!("{}{}", new_id(), suffix)))
}

#[derive(serde::Serialize, serde::Deserialize, Clone)]
struct IndexEntry {
    id: String,
    key: String,
    created_ms: i64,
    size: i64,
}

/// Creates one full backup now.
pub async fn run_full_backup(st: &St, reason: &str) -> AppResult<BackupInfo> {
    let _g = st.backup_lock.lock().await;
    if st.migrating.load(std::sync::atomic::Ordering::SeqCst) {
        return Err(bfail("a storage move is running; try again when it has finished"));
    }
    let pass = passphrase(st).ok_or_else(|| AppError::validation("set a backup passphrase first (Backups > Settings)"))?;
    let settings = st.db.call(|c| load_settings(c)).await?;
    let storage = target_storage(st, &settings)?;
    let snap = tmp_path(st, ".db").await?;
    let enc = tmp_path(st, ".db.age").await?;
    let result: AppResult<BackupInfo> = async {
        let snap2 = snap.clone();
        let n_att: i64 = st
            .db
            .call(move |c| {
                c.execute("VACUUM INTO ?1", [snap2.to_string_lossy().to_string()])?;
                Ok(c.query_row("SELECT COUNT(*) FROM attachments WHERE removed=0", [], |r| r.get(0))?)
            })
            .await
            .map_err(bfail)?;
        let (s2, e2, wf) = (snap.clone(), enc.clone(), st.cfg.backup_work_factor);
        tokio::task::spawn_blocking(move || crypto::encrypt_file(&s2, &e2, &pass, wf)).await.map_err(bfail)??;
        let size = tokio::fs::metadata(&enc).await.map_err(bfail)?.len();
        let id = new_id();
        let created = now_ms();
        let key = format!("backups/full/{created}-{id}.db.age");
        storage.put_file(&key, &enc, "application/octet-stream").await.map_err(bfail)?;
        match storage.head(&key).await {
            Ok(Some(s)) if s == size => {}
            Ok(other) => {
                let _ = storage.delete(&key).await;
                return Err(bfail(format!("verification failed (size {other:?}, expected {size})")));
            }
            Err(e) => return Err(bfail(e)),
        }
        let info = BackupInfo { id: id.clone(), kind: "full".into(), user_id: None, created_ms: created, reason: reason.to_string(), key: key.clone(), size: size as i64, attachments: n_att, encrypted: true };
        let i2 = info.clone();
        st.db
            .call(move |c| {
                c.execute(
                    "INSERT INTO backups(id,kind,user_id,created_ms,reason,key,size,attachments) VALUES(?1,?2,?3,?4,?5,?6,?7,?8)",
                    params![i2.id, i2.kind, i2.user_id, i2.created_ms, i2.reason, i2.key, i2.size, i2.attachments],
                )?;
                Ok(())
            })
            .await?;
        apply_retention(st, &storage, settings.retention.max(1)).await?;
        write_index(st, &storage).await?;
        Ok(info)
    }
    .await;
    let _ = tokio::fs::remove_file(&snap).await;
    let _ = tokio::fs::remove_file(&enc).await;
    record_result(st, &result).await;
    result
}

async fn record_result(st: &St, r: &AppResult<BackupInfo>) {
    let msg = match r {
        Ok(i) => format!("ok: {} ({} bytes)", i.key, i.size),
        Err(e) => format!("failed: {}", e.message),
    };
    let ok = r.is_ok();
    let _ = st
        .db
        .call(move |c| {
            let mut s = load_settings(c)?;
            if ok {
                s.last_run_ms = Some(now_ms());
            }
            s.last_status = msg;
            save_settings(c, &s)?;
            set_setting(c, "backup_last_attempt_ms", &now_ms().to_string())
        })
        .await;
}

/// Deletes full backups beyond the retention count from the target and the table.
async fn apply_retention(st: &St, storage: &Arc<dyn Storage>, keep: u32) -> AppResult<()> {
    let old: Vec<(String, String)> = st
        .db
        .call(move |c| {
            let mut s = c.prepare("SELECT id,key FROM backups WHERE kind='full' ORDER BY created_ms DESC, rowid DESC LIMIT -1 OFFSET ?1")?;
            let v = s.query_map([keep as i64], |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?)))?.collect::<Result<Vec<_>, _>>()?;
            Ok(v)
        })
        .await?;
    for (id, key) in old {
        let _ = storage.delete(&key).await;
        st.db.call(move |c| Ok(c.execute("DELETE FROM backups WHERE id=?1", [&id])?)).await?;
    }
    Ok(())
}

/// `backups/full/index.json` lists the remaining full backups; `latest.json` points at the newest.
/// A fresh server uses `latest.json` to find what to restore without listing the bucket.
async fn write_index(st: &St, storage: &Arc<dyn Storage>) -> AppResult<()> {
    let entries: Vec<IndexEntry> = st
        .db
        .call(|c| {
            let mut s = c.prepare("SELECT id,key,created_ms,size FROM backups WHERE kind='full' ORDER BY created_ms DESC, rowid DESC")?;
            let v = s
                .query_map([], |r| Ok(IndexEntry { id: r.get(0)?, key: r.get(1)?, created_ms: r.get(2)?, size: r.get(3)? }))?
                .collect::<Result<Vec<_>, _>>()?;
            Ok(v)
        })
        .await?;
    storage.put_bytes("backups/full/index.json", Bytes::from(serde_json::to_vec_pretty(&entries)?), "application/json").await.map_err(bfail)?;
    if let Some(l) = entries.first() {
        storage.put_bytes("backups/full/latest.json", Bytes::from(serde_json::to_vec_pretty(l)?), "application/json").await.map_err(bfail)?;
    }
    Ok(())
}

/// Runs a scheduled backup when one is due. Returns true when a backup was attempted.
pub async fn scheduled_tick(st: &St, now: i64) -> AppResult<bool> {
    let (s, last_attempt) = st
        .db
        .call(|c| Ok((load_settings(c)?, get_setting(c, "backup_last_attempt_ms")?.and_then(|v| v.parse::<i64>().ok()))))
        .await?;
    if !s.enabled || passphrase(st).is_none() {
        return Ok(false);
    }
    let due = s.last_run_ms.map(|l| now - l >= s.interval_hours.max(1) as i64 * 3_600_000).unwrap_or(true);
    // After a failure wait 15 minutes before trying again.
    let cooled = last_attempt.map(|a| now - a >= 15 * 60_000).unwrap_or(true);
    if !due || !cooled {
        return Ok(false);
    }
    let _ = run_full_backup(st, "scheduled").await;
    Ok(true)
}

// ---------------------------------------------------------------- routes (admin)

pub async fn get_backup_settings(State(st): State<St>, Admin(_a): Admin) -> AppResult<Json<BackupSettings>> {
    let mut s = st.db.call(|c| load_settings(c)).await?;
    s.passphrase_set = passphrase(&st).is_some();
    s.target = s.target.map(|t| t.masked());
    Ok(Json(s))
}

pub async fn put_backup_settings(State(st): State<St>, Admin(a): Admin, Json(req): Json<BackupSettingsUpdate>) -> AppResult<Json<BackupSettings>> {
    if a.role != Role::Owner {
        return Err(AppError::forbidden("only the owner can change server-wide settings"));
    }
    if let Some(p) = &req.passphrase {
        if p.chars().count() < 8 {
            return Err(AppError::validation("the backup passphrase must be at least 8 characters"));
        }
        set_passphrase(&st, p)?;
    }
    let st2 = st.clone();
    st.db
        .call(move |c| {
            let mut s = load_settings(c)?;
            if let Some(e) = req.enabled {
                s.enabled = e;
            }
            if let Some(h) = req.interval_hours {
                if h == 0 || h > 24 * 31 {
                    return Err(AppError::validation("interval must be 1 to 744 hours"));
                }
                s.interval_hours = h;
            }
            if let Some(r) = req.retention {
                if r == 0 || r > 365 {
                    return Err(AppError::validation("retention must be 1 to 365 backups"));
                }
                s.retention = r;
            }
            if req.clear_target {
                s.target = None;
            }
            if let Some(t) = &req.target {
                let t = match &s.target {
                    Some(prev) => t.clone().unmask_from(prev),
                    None => t.clone(),
                };
                s.target = Some(t);
            }
            if s.enabled && passphrase(&st2).is_none() {
                return Err(AppError::validation("set a backup passphrase before enabling scheduled backups"));
            }
            save_settings(c, &s)
        })
        .await?;
    get_backup_settings(State(st), Admin(a)).await
}

pub async fn test_backup_target(State(st): State<St>, Admin(_a): Admin) -> AppResult<Json<StorageTestResponse>> {
    let s = st.db.call(|c| load_settings(c)).await?;
    let res = match target_storage(&st, &s) {
        Ok(t) => t.check().await,
        Err(e) => Err(e),
    };
    Ok(Json(match res {
        Ok(m) => StorageTestResponse { ok: true, message: m },
        Err(e) => StorageTestResponse { ok: false, message: e.message },
    }))
}

pub async fn backup_now(State(st): State<St>, Admin(_a): Admin) -> AppResult<Json<BackupInfo>> {
    Ok(Json(run_full_backup(&st, "manual").await?))
}

pub async fn download_backup(State(st): State<St>, Admin(_a): Admin, Path(id): Path<String>) -> AppResult<Response> {
    let (key, kind): (String, String) = st
        .db
        .call(move |c| {
            c.query_row("SELECT key, kind FROM backups WHERE id=?1", [&id], |r| Ok((r.get(0)?, r.get(1)?)))
                .map_err(|_| AppError::not_found("backup not found"))
        })
        .await?;
    let s = st.db.call(|c| load_settings(c)).await?;
    let storage = if kind == "full" { target_storage(&st, &s)? } else { st.storage() };
    let data = storage.get(&key, None).await?;
    let name = key.rsplit('/').next().unwrap_or("backup").to_string();
    let mut resp = Response::new(Body::from_stream(data.stream));
    resp.headers_mut().insert(header::CONTENT_TYPE, "application/octet-stream".parse().unwrap());
    if let Ok(v) = format!("attachment; filename=\"{name}\"").parse() {
        resp.headers_mut().insert(header::CONTENT_DISPOSITION, v);
    }
    Ok(resp)
}

// ---------------------------------------------------------------- restore onto a fresh server

async fn require_fresh(st: &St) -> AppResult<()> {
    let n: i64 = st.db.call(|c| Ok(c.query_row("SELECT COUNT(*) FROM users", [], |r| r.get(0))?)).await?;
    if n > 0 {
        return Err(AppError::new(ErrorCode::SetupDone, "this server already has accounts; restore is only possible on a fresh server"));
    }
    Ok(())
}

pub async fn restore_from_target(State(st): State<St>, Json(req): Json<FreshRestoreRequest>) -> AppResult<Json<FreshRestoreResponse>> {
    require_fresh(&st).await?;
    let cfg = req.target.clone().ok_or_else(|| AppError::validation("choose where the backup is stored"))?;
    let storage = build_storage(&cfg, &st.cfg.data_dir)?;
    let key = match &req.key {
        Some(k) if !k.trim().is_empty() => k.trim().to_string(),
        _ => {
            let raw = storage.get_all("backups/full/latest.json").await.map_err(|_| AppError::not_found("no backup found in that storage (backups/full/latest.json is missing)"))?;
            let e: IndexEntry = serde_json::from_slice(&raw).map_err(|_| AppError::validation("backups/full/latest.json is damaged"))?;
            e.key
        }
    };
    let enc = tmp_path(&st, ".db.age").await?;
    let r: AppResult<FreshRestoreResponse> = async {
        let data = storage.get(&key, None).await.map_err(|_| AppError::not_found("that backup file was not found"))?;
        let mut f = tokio::fs::File::create(&enc).await.map_err(|e| AppError::internal(e.to_string()))?;
        let mut stream = data.stream;
        while let Some(chunk) = stream.next().await {
            f.write_all(&chunk.map_err(|e| AppError::storage(e.to_string()))?).await.map_err(|e| AppError::internal(e.to_string()))?;
        }
        f.flush().await.map_err(|e| AppError::internal(e.to_string()))?;
        drop(f);
        restore_encrypted_file(&st, &enc, &req.passphrase).await
    }
    .await;
    let _ = tokio::fs::remove_file(&enc).await;
    Ok(Json(r?))
}

/// Same, but the encrypted backup file is uploaded (for backups kept on the admin's own computer).
pub async fn restore_from_upload(State(st): State<St>, headers: HeaderMap, body: Body) -> AppResult<Json<FreshRestoreResponse>> {
    require_fresh(&st).await?;
    let pass = headers.get("x-backup-passphrase").and_then(|v| v.to_str().ok()).unwrap_or("").to_string();
    if pass.is_empty() {
        return Err(AppError::validation("send the passphrase in the X-Backup-Passphrase header"));
    }
    let enc = tmp_path(&st, ".db.age").await?;
    let r: AppResult<FreshRestoreResponse> = async {
        let mut f = tokio::fs::File::create(&enc).await.map_err(|e| AppError::internal(e.to_string()))?;
        let mut stream = body.into_data_stream();
        while let Some(chunk) = stream.next().await {
            f.write_all(&chunk.map_err(|e| AppError::validation(e.to_string()))?).await.map_err(|e| AppError::internal(e.to_string()))?;
        }
        f.flush().await.map_err(|e| AppError::internal(e.to_string()))?;
        drop(f);
        restore_encrypted_file(&st, &enc, &pass).await
    }
    .await;
    let _ = tokio::fs::remove_file(&enc).await;
    Ok(Json(r?))
}

async fn restore_encrypted_file(st: &St, enc: &std::path::Path, pass: &str) -> AppResult<FreshRestoreResponse> {
    let snap = tmp_path(st, ".db").await?;
    let (e2, s2, p2) = (enc.to_path_buf(), snap.clone(), pass.to_string());
    let dec = tokio::task::spawn_blocking(move || crypto::decrypt_file(&e2, &s2, &p2)).await.map_err(|e| AppError::internal(e.to_string()))?;
    if let Err(e) = dec {
        let _ = tokio::fs::remove_file(&snap).await;
        return Err(e);
    }
    let r = import_snapshot(st, &snap).await;
    let _ = tokio::fs::remove_file(&snap).await;
    let out = r?;
    // The passphrase that opened the backup protects the new server's backups too.
    set_passphrase(st, pass)?;
    Ok(out)
}

fn table_cols(conn: &Connection, schema: &str, table: &str) -> AppResult<Vec<String>> {
    let mut st = conn.prepare(&format!("PRAGMA {schema}.table_info({table})"))?;
    let v = st.query_map([], |r| r.get::<_, String>(1))?.collect::<Result<Vec<_>, _>>()?;
    Ok(v)
}

/// Replaces the database content with the snapshot, reloads the storage backend and forces every
/// client to re-bootstrap (event ids jump ahead of anything an old client could hold).
pub async fn import_snapshot(st: &St, snap: &std::path::Path) -> AppResult<FreshRestoreResponse> {
    let path = snap.to_string_lossy().to_string();
    let (counts, storage_cfg) = st
        .db
        .call(move |c| {
            // Sanity check the snapshot first.
            {
                let probe = Connection::open_with_flags(&path, rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY).map_err(|_| AppError::validation("the backup is not a valid database"))?;
                let ok: String = probe.query_row("PRAGMA integrity_check", [], |r| r.get(0)).map_err(|_| AppError::validation("the backup database is damaged"))?;
                if ok != "ok" {
                    return Err(AppError::validation("the backup database failed its integrity check"));
                }
                let has_users: i64 = probe
                    .query_row("SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name IN ('users','messages','conversations')", [], |r| r.get(0))
                    .map_err(|_| AppError::validation("the backup is not a WorkChat backup"))?;
                if has_users != 3 {
                    return Err(AppError::validation("the backup is not a WorkChat backup"));
                }
            }
            c.execute("ATTACH DATABASE ?1 AS snap", [&path])?;
            let res: AppResult<()> = (|| {
                let tables: Vec<String> = {
                    let mut s = c.prepare("SELECT name FROM snap.sqlite_master WHERE type='table' AND name NOT LIKE 'sqlite_%' AND name NOT LIKE 'messages_fts%'")?;
                    let v = s.query_map([], |r| r.get::<_, String>(0))?.collect::<Result<Vec<_>, _>>()?;
                    v
                };
                let tx = c.unchecked_transaction()?;
                for t in tables {
                    let main = table_cols(&tx, "main", &t)?;
                    if main.is_empty() {
                        continue;
                    }
                    let snapc = table_cols(&tx, "snap", &t)?;
                    let cols: Vec<String> = main.into_iter().filter(|c| snapc.contains(c)).collect();
                    if cols.is_empty() {
                        continue;
                    }
                    let list = cols.iter().map(|c| format!("\"{c}\"")).collect::<Vec<_>>().join(",");
                    tx.execute(&format!("DELETE FROM main.\"{t}\""), [])?;
                    tx.execute(&format!("INSERT INTO main.\"{t}\"({list}) SELECT {list} FROM snap.\"{t}\""), [])?;
                }
                tx.execute("INSERT INTO messages_fts(messages_fts) VALUES('rebuild')", [])?;
                // Event ids jump ahead and the retained floor moves with them: every client re-bootstraps.
                let max_ev: i64 = tx.query_row("SELECT COALESCE(MAX(id),0) FROM events", [], |r| r.get(0))?;
                let new_seq = max_ev + 1_000_000;
                let upd = tx.execute("UPDATE sqlite_sequence SET seq=?1 WHERE name='events'", [new_seq])?;
                if upd == 0 {
                    tx.execute("INSERT INTO sqlite_sequence(name,seq) VALUES('events',?1)", [new_seq])?;
                }
                set_setting(&tx, "events_floor", &new_seq.to_string())?;
                tx.commit()?;
                Ok(())
            })();
            let _ = c.execute("DETACH DATABASE snap", []);
            res?;
            let q = |sql: &str| -> AppResult<i64> { Ok(c.query_row(sql, [], |r| r.get(0))?) };
            let counts = (
                q("SELECT COUNT(*) FROM users WHERE deleted=0")?,
                q("SELECT COUNT(*) FROM conversations")?,
                q("SELECT COUNT(*) FROM messages")?,
                q("SELECT COUNT(*) FROM attachments WHERE removed=0")?,
            );
            let cfg = load_storage_config(c)?.unwrap_or(StorageConfig::Local { path: None });
            Ok((counts, cfg))
        })
        .await?;
    // Reconnect to the attachment storage named in the restored settings.
    let storage = build_storage(&storage_cfg, &st.cfg.data_dir)?;
    *st.storage.write().unwrap_or_else(|e| e.into_inner()) = storage.clone();
    *st.storage_cfg.write().unwrap_or_else(|e| e.into_inner()) = storage_cfg.clone();
    let rows: Vec<(String, i64)> = st
        .db
        .call(|c| {
            let mut s = c.prepare("SELECT storage_key,size FROM attachments WHERE removed=0")?;
            let v = s.query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, i64>(1)?)))?.collect::<Result<Vec<_>, _>>()?;
            Ok(v)
        })
        .await?;
    let mut ok = 0;
    for (k, size) in rows {
        if let Ok(Some(s)) = storage.head(&k).await {
            if s as i64 == size {
                ok += 1;
            }
        }
    }
    let _ = STORAGE_KEY;
    Ok(FreshRestoreResponse { users: counts.0, conversations: counts.1, messages: counts.2, attachments: counts.3, attachments_resolvable: ok, storage: storage_cfg.kind().into() })
}

/// Decrypts a full backup (found by id) into a temporary snapshot database for selective restore.
pub async fn open_full_backup(st: &St, backup_key: &str) -> AppResult<PathBuf> {
    let s = st.db.call(|c| load_settings(c)).await?;
    let storage = target_storage(st, &s)?;
    let pass = passphrase(st).ok_or_else(|| AppError::validation("the backup passphrase is not set on this server"))?;
    let enc = tmp_path(st, ".db.age").await?;
    let snap = tmp_path(st, ".db").await?;
    let data = storage.get(backup_key, None).await?;
    let mut f = tokio::fs::File::create(&enc).await.map_err(|e| AppError::internal(e.to_string()))?;
    let mut stream = data.stream;
    while let Some(chunk) = stream.next().await {
        f.write_all(&chunk.map_err(|e| AppError::storage(e.to_string()))?).await.map_err(|e| AppError::internal(e.to_string()))?;
    }
    f.flush().await.map_err(|e| AppError::internal(e.to_string()))?;
    drop(f);
    let (e2, s2) = (enc.clone(), snap.clone());
    let r = tokio::task::spawn_blocking(move || crypto::decrypt_file(&e2, &s2, &pass)).await.map_err(|e| AppError::internal(e.to_string()))?;
    let _ = tokio::fs::remove_file(&enc).await;
    r?;
    Ok(snap)
}

pub fn spawn_scheduler(st: St) {
    tokio::spawn(async move {
        loop {
            tokio::time::sleep(std::time::Duration::from_secs(60)).await;
            if let Err(e) = scheduled_tick(&st, now_ms()).await {
                tracing::warn!("scheduled backup check failed: {e}");
            }
        }
    });
}

#[allow(dead_code)]
fn _unused(_: StatusCode) -> impl IntoResponse {}
