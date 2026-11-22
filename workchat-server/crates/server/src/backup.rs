//! Per-user backups, "Clear history", account deletion and date-range delete.
//!
//! Every destructive action first writes a per-user backup into the storage backend and
//! verifies it; if that fails the action is refused and nothing changes. Attachments that no
//! longer have any visible reference are *moved* into the backup folder, not erased.

use crate::db::{new_id, now_ms};
use crate::emit::Emitter;
use crate::error::{AppError, AppResult};
use crate::queries::*;
use crate::state::St;
use crate::storage::backup_attachment_key;
use bytes::Bytes;
use proto::*;
use rusqlite::{params, Connection};
use serde_json::{json, Value};

pub struct BackupResult {
    pub info: BackupInfo,
    pub prefix: String,
}

fn backup_err(m: impl std::fmt::Display) -> AppError {
    AppError::new(ErrorCode::BackupFailed, format!("backup failed, nothing was changed: {m}"))
}

/// Every saved item of the user, including ones whose message is not visible right now (so a restore brings them back).
fn saved_rows(conn: &Connection, user_id: &str) -> AppResult<Vec<Value>> {
    let mut st = conn.prepare("SELECT message_id,conv_id,saved_ms FROM saved_items WHERE user_id=?1 ORDER BY saved_ms")?;
    let rows = st.query_map([user_id], |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?, r.get::<_, i64>(2)?)))?.collect::<Result<Vec<_>, _>>()?;
    let mut out = vec![];
    for (mid, conv, ms) in rows {
        let mut l = conn.prepare("SELECT label_id FROM saved_labels WHERE user_id=?1 AND message_id=?2")?;
        let labels = l.query_map(params![user_id, mid], |r| r.get::<_, String>(0))?.collect::<Result<Vec<_>, _>>()?;
        out.push(json!({"message_id": mid, "conv_id": conv, "saved_ms": ms, "label_ids": labels}));
    }
    Ok(out)
}

/// Builds the JSON document for one user: account, memberships, conversations, visible messages.
pub fn build_user_doc(conn: &Connection, user_id: &str) -> AppResult<Value> {
    let u = get_user(conn, user_id)?;
    let pw: String = conn.query_row("SELECT password_hash FROM users WHERE id=?1", [user_id], |r| r.get(0))?;
    let mut convs = vec![];
    let mut messages = vec![];
    let mut atts: Vec<Value> = vec![];
    let mut seen_att = std::collections::HashSet::new();
    for cid in user_conversation_ids(conn, user_id)? {
        let c = load_conversation(conn, &cid)?;
        let (last_read, marked): (i64, i64) = conn
            .query_row(
                "SELECT last_read_seq, marked_unread FROM read_state WHERE user_id=?1 AND conv_id=?2",
                [user_id, &cid],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .unwrap_or((0, 0));
        let dm_key: Option<String> = conn.query_row("SELECT dm_key FROM conversations WHERE id=?1", [&cid], |r| r.get(0))?;
        convs.push(json!({"conversation": c, "dm_key": dm_key, "last_read_seq": last_read, "marked_unread": marked}));
        let mut after = 0;
        loop {
            let h = history(conn, &cid, user_id, &HistoryQuery { before_seq: None, after_seq: Some(after), around_ts: None, around_seq: None, limit: 200 })?;
            if h.messages.is_empty() {
                break;
            }
            after = h.messages.last().map(|m| m.seq).unwrap_or(after);
            for m in &h.messages {
                for a in &m.attachments {
                    if seen_att.insert(a.id.clone()) {
                        let (sha, key): (String, String) = conn.query_row(
                            "SELECT sha256, storage_key FROM attachments WHERE id=?1",
                            [&a.id],
                            |r| Ok((r.get(0)?, r.get(1)?)),
                        )?;
                        atts.push(json!({"id": a.id, "name": a.name, "content_type": a.content_type, "size": a.size, "sha256": sha, "storage_key": key, "removed": a.removed}));
                    }
                }
            }
            messages.extend(h.messages);
            if !h.has_more_after {
                break;
            }
        }
    }
    Ok(json!({
        "format": "workchat-user-backup",
        "version": 1,
        "created_ms": now_ms(),
        "user": {
            "id": u.id, "username": u.username, "display_name": u.display_name, "email": u.email,
            "role": u.role, "password_hash": pw, "password_set": u.password_set, "disabled": u.disabled,
            "privacy_controls_enabled": u.privacy_controls_enabled, "hide_last_active": u.hide_last_active,
            "created_ms": u.created_ms, "avatar": u.avatar
        },
        "conversations": convs,
        "labels": crate::saved::load_labels(conn, user_id)?,
        "saved": saved_rows(conn, user_id)?,
        // the private pictures this person chose for other users and conversations (ids of their own uploads).
        "avatar_overrides": crate::routes::avatars::load_overrides(conn, user_id)?,
        "messages": messages,
        "attachments": atts,
    }))
}

/// Writes and verifies a per-user backup. Errors map to `backup_failed`.
pub async fn backup_user(st: &St, user_id: &str, reason: &str) -> AppResult<BackupResult> {
    if st.migrating.load(std::sync::atomic::Ordering::SeqCst) {
        return Err(AppError::new(proto::ErrorCode::BackupFailed, "A storage move is running; the backup (and the action that needs it) is refused until it finishes."));
    }
    let uid = user_id.to_string();
    let doc = st.db.call(move |c| build_user_doc(c, &uid)).await.map_err(backup_err)?;
    let id = new_id();
    let prefix = format!("backups/users/{user_id}/{id}");
    // With a backup passphrase configured, per-user backups are encrypted like full backups.
    let pass = crate::fullbackup::passphrase(st);
    let plain = serde_json::to_vec_pretty(&doc).map_err(backup_err)?;
    let (key, bytes) = match &pass {
        Some(p) => {
            let (p2, wf) = (p.clone(), st.cfg.backup_work_factor);
            let enc = tokio::task::spawn_blocking(move || crate::crypto::encrypt_bytes(&plain, &p2, wf)).await.map_err(backup_err)?.map_err(backup_err)?;
            (format!("{prefix}/data.json.age"), Bytes::from(enc))
        }
        None => (format!("{prefix}/data.json"), Bytes::from(plain)),
    };
    let size = bytes.len() as u64;
    let n_att = doc["attachments"].as_array().map(|a| a.len()).unwrap_or(0) as i64;
    let storage = st.storage();
    storage.put_bytes(&key, bytes.clone(), "application/json").await.map_err(backup_err)?;
    match storage.head(&key).await {
        Ok(Some(s)) if s == size => {}
        Ok(other) => {
            let _ = storage.delete(&key).await;
            return Err(backup_err(format!("verification failed (size {other:?}, expected {size})")));
        }
        Err(e) => return Err(backup_err(e)),
    }
    let info = BackupInfo {
        id: id.clone(),
        kind: "user".into(),
        user_id: Some(user_id.to_string()),
        created_ms: now_ms(),
        reason: reason.to_string(),
        key: key.clone(),
        size: size as i64,
        attachments: n_att,
        encrypted: pass.is_some(),
    };
    let i2 = info.clone();
    st.db
        .call(move |c| {
            c.execute(
                "INSERT INTO backups(id,kind,user_id,created_ms,reason,key,size,attachments) VALUES(?1,?2,?3,?4,?5,?6,?7,?8)",
                params![i2.id, i2.kind, i2.user_id, i2.created_ms, i2.reason, i2.key, i2.size, i2.attachments],
            )?;
            Ok(())
        })
        .await
        .map_err(backup_err)?;
    Ok(BackupResult { info, prefix })
}

pub struct HideRequest {
    pub user_id: String,
    /// Restrict to a time range (inclusive); `None` means every message.
    pub range: Option<(i64, i64)>,
    pub conversation_id: Option<String>,
    pub also_others: bool,
    /// Restrict to these messages (admin viewer bulk delete).
    pub message_ids: Option<Vec<String>>,
}

pub struct HideOutcome {
    pub messages_hidden: i64,
    pub attachments_moved: i64,
}

/// Message ids visible to the user and matching the filter.
fn target_messages(conn: &Connection, req: &HideRequest) -> AppResult<Vec<String>> {
    let vis = visible_sql("?1");
    let mut sql = format!(
        "SELECT m.id FROM messages m JOIN members mem ON mem.conv_id=m.conv_id AND mem.user_id=?1 WHERE {vis}"
    );
    let mut args: Vec<Box<dyn rusqlite::ToSql>> = vec![Box::new(req.user_id.clone())];
    if let Some((a, b)) = req.range {
        sql.push_str(&format!(" AND m.ts>=?{} AND m.ts<=?{}", args.len() + 1, args.len() + 2));
        args.push(Box::new(a));
        args.push(Box::new(b));
    }
    if let Some(c) = &req.conversation_id {
        sql.push_str(&format!(" AND m.conv_id=?{}", args.len() + 1));
        args.push(Box::new(c.clone()));
    }
    if let Some(ids) = &req.message_ids {
        sql.push_str(&format!(" AND m.id IN (SELECT value FROM json_each(?{}))", args.len() + 1));
        args.push(Box::new(serde_json::to_string(ids)?));
    }
    let mut st = conn.prepare(&sql)?;
    let refs: Vec<&dyn rusqlite::ToSql> = args.iter().map(|b| b.as_ref()).collect();
    let ids = st.query_map(refs.as_slice(), |r| r.get::<_, String>(0))?.collect::<Result<Vec<_>, _>>()?;
    Ok(ids)
}

/// Attachments whose every remaining visible reference is among the messages about to be hidden.
fn attachments_to_move(conn: &Connection, req: &HideRequest, msg_ids: &[String]) -> AppResult<Vec<(String, String)>> {
    let json_ids = serde_json::to_string(msg_ids)?;
    let mut st = conn.prepare(
        "SELECT DISTINCT a.id, a.storage_key FROM message_attachments ma JOIN attachments a ON a.id=ma.attachment_id
         WHERE ma.message_id IN (SELECT value FROM json_each(?1)) AND a.backup_id IS NULL AND a.removed=0 AND a.purpose='file'",
    )?;
    let cands = st
        .query_map([&json_ids], |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?)))?
        .collect::<Result<Vec<_>, _>>()?;
    let mut out = vec![];
    for (aid, key) in cands {
        let remaining: i64 = conn.query_row(
            "SELECT COUNT(*) FROM message_attachments ma JOIN messages m2 ON m2.id=ma.message_id
             JOIN members mem ON mem.conv_id=m2.conv_id
             WHERE ma.attachment_id=?1 AND m2.deleted=0
             AND NOT EXISTS (SELECT 1 FROM message_hidden h WHERE h.user_id=mem.user_id AND h.message_id=m2.id)
             AND NOT (m2.id IN (SELECT value FROM json_each(?2)) AND (mem.user_id=?3 OR ?4=1))",
            params![aid, json_ids, req.user_id, req.also_others as i64],
            |r| r.get(0),
        )?;
        if remaining == 0 {
            out.push((aid, key));
        }
    }
    Ok(out)
}

/// Backs up, moves orphaned attachments into the backup folder, then hides the messages.
/// Refuses (changing nothing) when the backup cannot be written.
pub async fn hide_with_backup(st: &St, req: HideRequest, reason: &str) -> AppResult<(BackupResult, HideOutcome)> {
    let backup = backup_user(st, &req.user_id, reason).await?;
    let (msg_ids, moves) = {
        let r = HideRequest { user_id: req.user_id.clone(), range: req.range, conversation_id: req.conversation_id.clone(), also_others: req.also_others, message_ids: req.message_ids.clone() };
        st.db
            .call(move |c| {
                let ids = target_messages(c, &r)?;
                let mv = attachments_to_move(c, &r, &ids)?;
                Ok((ids, mv))
            })
            .await?
    };
    // Move objects first; roll back if any move fails.
    let storage = st.storage();
    let mut done: Vec<(String, String, String)> = vec![]; // (att id, old key, new key)
    for (aid, key) in &moves {
        let new_key = backup_attachment_key(&backup.prefix, aid);
        if let Err(e) = storage.rename(key, &new_key).await {
            for (_, old, new) in done.iter().rev() {
                let _ = storage.rename(new, old).await;
            }
            // The backup itself is harmless to keep; the action is refused.
            return Err(backup_err(format!("could not move attachment {aid} into the backup folder: {e}")));
        }
        done.push((aid.clone(), key.clone(), new_key));
    }
    let hub = st.hub.clone();
    let r2 = HideRequest { user_id: req.user_id.clone(), range: req.range, conversation_id: req.conversation_id.clone(), also_others: req.also_others, message_ids: req.message_ids.clone() };
    let backup_id = backup.info.id.clone();
    let done2 = done.clone();
    let ids2 = msg_ids.clone();
    let res = st
        .db
        .call(move |c| {
            let tx = c.transaction()?;
            let mut em = Emitter::new();
            // (user, conv) -> ids to announce
            let mut per_user: std::collections::BTreeMap<(String, String), Vec<String>> = Default::default();
            for mid in &ids2 {
                let conv: String = tx.query_row("SELECT conv_id FROM messages WHERE id=?1", [mid], |r| r.get(0))?;
                let targets: Vec<String> = if r2.also_others { member_ids(&tx, &conv)? } else { vec![r2.user_id.clone()] };
                for u in targets {
                    let n = tx.execute("INSERT OR IGNORE INTO message_hidden(user_id,message_id) VALUES(?1,?2)", params![u, mid])?;
                    if n > 0 {
                        per_user.entry((u, conv.clone())).or_default().push(mid.clone());
                    }
                }
            }
            for ((u, conv), ids) in per_user {
                em.emit(&tx, &u, EventData::MessageHide { conv_id: conv.clone(), message_ids: ids })?;
            }
            for (aid, _old, new_key) in &done2 {
                tx.execute("UPDATE attachments SET storage_key=?2, backup_id=?3 WHERE id=?1", params![aid, new_key, backup_id])?;
            }
            tx.commit()?;
            em.flush(&hub);
            Ok(())
        })
        .await;
    if let Err(e) = res {
        for (_, old, new) in done.iter().rev() {
            let _ = storage.rename(new, old).await;
        }
        return Err(e);
    }
    Ok((backup, HideOutcome { messages_hidden: msg_ids.len() as i64, attachments_moved: moves.len() as i64 }))
}

/// "Clear history": backup, hide everything for the user, move orphaned attachments, force re-login.
pub async fn clear_history(st: &St, user_id: &str) -> AppResult<ClearHistoryResponse> {
    let (backup, out) = hide_with_backup(
        st,
        HideRequest { user_id: user_id.to_string(), range: None, conversation_id: None, also_others: false, message_ids: None },
        "clear_history",
    )
    .await?;
    let (hub, uid) = (st.hub.clone(), user_id.to_string());
    let revoked = st
        .db
        .call(move |c| {
            // The user's server-side event log held message payloads; drop it.
            c.execute("DELETE FROM events WHERE user_id=?1", [&uid])?;
            c.execute("DELETE FROM read_state WHERE user_id=?1", [&uid])?;
            crate::users::revoke_sessions(c, &hub, &uid, "history_cleared", None)
        })
        .await?;
    Ok(ClearHistoryResponse {
        backup_id: backup.info.id.clone(),
        backup_key: backup.info.key.clone(),
        messages_hidden: out.messages_hidden,
        attachments_moved: out.attachments_moved,
        sessions_revoked: revoked,
    })
}

/// Deletes the account after the same backup + hide steps. The other side's copies stay.
pub async fn delete_user(st: &St, user_id: &str) -> AppResult<ClearHistoryResponse> {
    let resp = clear_history(st, user_id).await?;
    let (hub, uid) = (st.hub.clone(), user_id.to_string());
    st.db
        .call(move |c| {
            let tx = c.transaction()?;
            let junk = crate::auth::random_token("x", 24);
            let hash = crate::auth::hash_password(&junk)?;
            tx.execute(
                "UPDATE users SET deleted=1, disabled=1, username=?2, display_name='Deleted user', email=NULL, avatar_attachment=NULL, password_hash=?3 WHERE id=?1",
                params![uid, format!("deleted-{}", &uid[uid.len().saturating_sub(12)..]), hash],
            )?;
            // Everything that belongs to the person alone goes; messages stay (the other side keeps its copy, the author
            // shows as "Deleted user"). The unique placeholder name uses the random end of the id: the start of a v7 id
            // is a timestamp, which made the second deletion collide and answer 500.
            for sql in [
                "DELETE FROM login_links WHERE user_id=?1",
                "DELETE FROM push_registrations WHERE user_id=?1",
                "DELETE FROM drafts WHERE user_id=?1",
                "DELETE FROM conv_prefs WHERE user_id=?1",
                "DELETE FROM avatar_overrides WHERE viewer_id=?1 OR (kind='user' AND target=?1)",
                "DELETE FROM sections WHERE user_id=?1",
                "DELETE FROM thread_follows WHERE user_id=?1",
                "DELETE FROM saved_items WHERE user_id=?1",
                "DELETE FROM saved_labels WHERE user_id=?1",
                "DELETE FROM labels WHERE user_id=?1",
                "DELETE FROM activity_state WHERE user_id=?1",
                "DELETE FROM user_group_members WHERE user_id=?1",
                "DELETE FROM scheduled WHERE user_id=?1 AND state='pending'",
                "DELETE FROM bots WHERE user_id=?1",
            ] {
                tx.execute(sql, [&uid])?;
            }
            let mut em = Emitter::new();
            // Leave channels and group DMs; DMs stay for the other person.
            let convs: Vec<String> = {
                let mut st = tx.prepare(
                    "SELECT c.id FROM conversations c JOIN members m ON m.conv_id=c.id WHERE m.user_id=?1 AND c.kind IN ('channel','group_dm')",
                )?;
                let v = st.query_map([&uid], |r| r.get::<_, String>(0))?.collect::<Result<Vec<_>, _>>()?;
                v
            };
            for cid in convs {
                tx.execute("DELETE FROM members WHERE conv_id=?1 AND user_id=?2", params![cid, uid])?;
                crate::routes::chat::upsert_for_members(&tx, &mut em, &cid)?;
            }
            crate::users::announce_user(&tx, &hub, &uid, &mut em)?;
            tx.commit()?;
            em.flush(&hub);
            Ok(())
        })
        .await?;
    Ok(resp)
}

/// Reads a per-user backup document back from storage (decrypting when needed).
pub async fn read_user_backup(st: &St, key: &str) -> AppResult<Value> {
    let raw = st.storage().get_all(key).await.map_err(|_| AppError::not_found("the backup file is missing from storage"))?;
    let bytes = if key.ends_with(".age") {
        let pass = crate::fullbackup::passphrase(st).ok_or_else(|| AppError::validation("the backup passphrase is not set on this server"))?;
        tokio::task::spawn_blocking(move || crate::crypto::decrypt_bytes(&raw, &pass)).await.map_err(|e| AppError::internal(e.to_string()))??
    } else {
        raw.to_vec()
    };
    serde_json::from_slice(&bytes).map_err(|_| AppError::validation("the backup file is damaged"))
}
