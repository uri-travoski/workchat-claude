//! Storage management shared by setup and the admin API.

use crate::db::{get_setting, set_setting, now_ms, new_id};
use crate::error::{AppError, AppResult};
use crate::state::St;
use crate::storage::{build_storage, Storage};
use proto::*;
use sha2::{Digest, Sha256};
use std::sync::Arc;

pub const STORAGE_KEY: &str = "storage_config";

pub fn load_storage_config(conn: &rusqlite::Connection) -> AppResult<Option<StorageConfig>> {
    Ok(get_setting(conn, STORAGE_KEY)?.and_then(|s| serde_json::from_str(&s).ok()))
}

/// Persists the config and swaps the live backend.
pub async fn set_storage(st: &St, cfg: StorageConfig, storage: Arc<dyn Storage>) -> AppResult<()> {
    let json = serde_json::to_string(&cfg)?;
    st.db.call(move |c| set_setting(c, STORAGE_KEY, &json)).await?;
    *st.storage.write().unwrap_or_else(|e| e.into_inner()) = storage;
    *st.storage_cfg.write().unwrap_or_else(|e| e.into_inner()) = cfg;
    Ok(())
}

fn update_job(st: &St, id: &str, f: impl FnOnce(&mut JobStatus)) {
    if let Some(j) = st.jobs.lock().unwrap_or_else(|e| e.into_inner()).get_mut(id) {
        f(j);
    }
}

pub fn new_job(st: &St, kind: &str, total: i64) -> String {
    let id = new_id();
    let job = JobStatus {
        id: id.clone(),
        kind: kind.into(),
        state: "running".into(),
        total,
        done: 0,
        failed: 0,
        message: "starting".into(),
        errors: vec![],
        started_ms: now_ms(),
        finished_ms: None,
    };
    st.jobs.lock().unwrap_or_else(|e| e.into_inner()).insert(id.clone(), job);
    id
}

fn fail_job(st: &St, id: &str, msg: String) {
    update_job(st, id, |j| {
        j.state = "failed".into();
        j.message = msg;
        j.finished_ms = Some(now_ms());
    });
}

struct ObjRef {
    key: String,
    size: i64,
    sha256: Option<String>,
}

fn list_objects(conn: &rusqlite::Connection) -> AppResult<Vec<ObjRef>> {
    let mut v = vec![];
    let mut st = conn.prepare("SELECT storage_key, size, sha256 FROM attachments WHERE removed=0")?;
    for r in st.query_map([], |r| Ok(ObjRef { key: r.get(0)?, size: r.get(1)?, sha256: Some(r.get(2)?) }))? {
        v.push(r?);
    }
    let mut st = conn.prepare("SELECT key, size FROM backups")?;
    for r in st.query_map([], |r| Ok(ObjRef { key: r.get(0)?, size: r.get(1)?, sha256: None }))? {
        v.push(r?);
    }
    Ok(v)
}

/// Copies one object to a temp file (hashing it) and uploads it to `target`.
async fn copy_object(st: &St, src: &Arc<dyn Storage>, dst: &Arc<dyn Storage>, o: &ObjRef) -> AppResult<()> {
    use futures_util::StreamExt;
    use tokio::io::AsyncWriteExt;
    let tmp = st.cfg.data_dir.join("tmp").join(new_id());
    tokio::fs::create_dir_all(tmp.parent().unwrap()).await.map_err(|e| AppError::internal(e.to_string()))?;
    let result: AppResult<()> = async {
        let mut data = src.get(&o.key, None).await?;
        let mut f = tokio::fs::File::create(&tmp).await.map_err(|e| AppError::internal(e.to_string()))?;
        let mut h = Sha256::new();
        let mut n: i64 = 0;
        while let Some(chunk) = data.stream.next().await {
            let chunk = chunk.map_err(|e| AppError::storage(format!("read {}: {e}", o.key)))?;
            h.update(&chunk);
            n += chunk.len() as i64;
            f.write_all(&chunk).await.map_err(|e| AppError::internal(e.to_string()))?;
        }
        f.flush().await.map_err(|e| AppError::internal(e.to_string()))?;
        drop(f);
        if n != o.size {
            return Err(AppError::storage(format!("{}: size {} on source, expected {}", o.key, n, o.size)));
        }
        if let Some(want) = &o.sha256 {
            if hex::encode(h.finalize()) != *want {
                return Err(AppError::storage(format!("{}: checksum mismatch on source", o.key)));
            }
        }
        dst.put_file(&o.key, &tmp, "application/octet-stream").await?;
        match dst.head(&o.key).await? {
            Some(s) if s as i64 == o.size => Ok(()),
            other => Err(AppError::storage(format!("{}: target size {:?}, expected {}", o.key, other, o.size))),
        }
    }
    .await;
    let _ = tokio::fs::remove_file(&tmp).await;
    result
}

/// Starts a background job that copies + verifies every object to the target backend, switches
/// the active backend, then optionally deletes the source objects.
pub async fn start_migration(st: &St, target: StorageConfig, delete_source: bool) -> AppResult<String> {
    if st.migrating.swap(true, std::sync::atomic::Ordering::SeqCst) {
        return Err(AppError::conflict("a storage migration is already running"));
    }
    let prep: AppResult<(Arc<dyn Storage>, Vec<ObjRef>)> = async {
        let dst = build_storage(&target, &st.cfg.data_dir)?;
        dst.check().await.map_err(|e| AppError::storage(format!("target storage test failed: {}", e.message)))?;
        let objs = st.db.call(|c| list_objects(c)).await?;
        Ok((dst, objs))
    }
    .await;
    let (dst, objs) = match prep {
        Ok(v) => v,
        Err(e) => {
            st.migrating.store(false, std::sync::atomic::Ordering::SeqCst);
            return Err(e);
        }
    };
    let job_id = new_job(st, "storage_migrate", objs.len() as i64);
    let st2 = st.clone();
    let jid = job_id.clone();
    tokio::spawn(async move {
        let src = st2.storage();
        let mut copied: Vec<String> = vec![];
        let mut failed = false;
        for o in &objs {
            match copy_object(&st2, &src, &dst, o).await {
                Ok(()) => {
                    copied.push(o.key.clone());
                    update_job(&st2, &jid, |j| {
                        j.done += 1;
                        j.message = format!("copied {} of {}", j.done, j.total);
                    });
                }
                Err(e) => {
                    failed = true;
                    update_job(&st2, &jid, |j| {
                        j.failed += 1;
                        j.errors.push(e.message.clone());
                    });
                    break;
                }
            }
        }
        if failed {
            for k in &copied {
                let _ = dst.delete(k).await;
            }
            fail_job(&st2, &jid, "migration failed; the old storage is still active and unchanged".into());
            st2.migrating.store(false, std::sync::atomic::Ordering::SeqCst);
            return;
        }
        // Verification pass against the target before switching.
        for o in &objs {
            match dst.head(&o.key).await {
                Ok(Some(s)) if s as i64 == o.size => {}
                other => {
                    for k in &copied {
                        let _ = dst.delete(k).await;
                    }
                    update_job(&st2, &jid, |j| j.errors.push(format!("verification failed for {}: {:?}", o.key, other.map_err(|e| e.message))));
                    fail_job(&st2, &jid, "verification failed; the old storage is still active".into());
                    st2.migrating.store(false, std::sync::atomic::Ordering::SeqCst);
                    return;
                }
            }
        }
        if let Err(e) = set_storage(&st2, target.clone(), dst.clone()).await {
            fail_job(&st2, &jid, format!("could not switch storage: {}", e.message));
            st2.migrating.store(false, std::sync::atomic::Ordering::SeqCst);
            return;
        }
        let mut deleted = 0;
        if delete_source {
            for o in &objs {
                if src.delete(&o.key).await.is_ok() {
                    deleted += 1;
                }
            }
        }
        update_job(&st2, &jid, |j| {
            j.state = "done".into();
            j.finished_ms = Some(now_ms());
            j.message = format!("moved and verified {} files to {}; {} deleted from the old storage", j.total, target.kind(), deleted);
        });
        st2.migrating.store(false, std::sync::atomic::Ordering::SeqCst);
    });
    Ok(job_id)
}

/// Background pass that proves every attachment is still resolvable in the active backend.
pub async fn start_verify(st: &St, deep: bool) -> AppResult<String> {
    let objs = st.db.call(|c| list_objects(c)).await?;
    let job_id = new_job(st, "storage_verify", objs.len() as i64);
    let st2 = st.clone();
    let jid = job_id.clone();
    tokio::spawn(async move {
        let storage = st2.storage();
        for o in &objs {
            let res: AppResult<()> = async {
                match storage.head(&o.key).await? {
                    Some(s) if s as i64 == o.size => {}
                    Some(s) => return Err(AppError::storage(format!("{}: size {} != {}", o.key, s, o.size))),
                    None => return Err(AppError::storage(format!("{}: missing", o.key))),
                }
                if deep {
                    if let Some(want) = &o.sha256 {
                        let bytes = storage.get_all(&o.key).await?;
                        if hex::encode(Sha256::digest(&bytes)) != *want {
                            return Err(AppError::storage(format!("{}: checksum mismatch", o.key)));
                        }
                    }
                }
                Ok(())
            }
            .await;
            update_job(&st2, &jid, |j| match &res {
                Ok(()) => j.done += 1,
                Err(e) => {
                    j.failed += 1;
                    if j.errors.len() < 50 {
                        j.errors.push(e.message.clone());
                    }
                }
            });
        }
        update_job(&st2, &jid, |j| {
            j.state = if j.failed == 0 { "done".into() } else { "failed".into() };
            j.finished_ms = Some(now_ms());
            j.message = format!("{} of {} files resolvable, {} problems", j.done, j.total, j.failed);
        });
    });
    Ok(job_id)
}
