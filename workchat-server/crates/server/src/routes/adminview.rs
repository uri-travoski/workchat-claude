//! Admin chat viewer API: read any conversation (optionally "as" a user, applying that user's
//! visibility), search across conversations, and delete single or bulk messages with a mandatory
//! backup first. Writing as the admin uses the normal user endpoints in conversations they belong to.

use crate::auth::Admin;
use crate::backup::{self, HideRequest};
use crate::db::{new_id, now_ms};
use crate::emit::Emitter;
use crate::error::{AppError, AppResult};
use crate::queries::*;
use crate::routes::chat::tombstone_message;
use crate::routes::search::{fts_query, run_search, SearchParams};
use crate::state::St;
use crate::storage::backup_attachment_key;
use axum::extract::{Path, Query, State};
use axum::Json;
use bytes::Bytes;
use proto::*;
use rusqlite::{params, Connection, OptionalExtension};
use serde::Deserialize;
use serde_json::json;
use std::collections::BTreeMap;

type J<T> = AppResult<Json<T>>;

#[derive(Deserialize, Default)]
pub struct ConvParams {
    /// Show this person's conversations (including DMs) from their point of view.
    user_id: Option<String>,
    /// "channel", "group_dm", "dm" or "self" when no user is chosen.
    kind: Option<String>,
    q: Option<String>,
}

fn title_for(conn: &Connection, c: &Conversation, perspective: Option<&str>) -> AppResult<String> {
    let name_of = |id: &str| -> AppResult<String> { Ok(get_user(conn, id).map(|u| u.display_name).unwrap_or_else(|_| "Unknown".into())) };
    Ok(match c.kind {
        ConversationKind::Channel | ConversationKind::GroupDm => c.name.clone().unwrap_or_else(|| "Conversation".into()),
        ConversationKind::SelfNote => match c.members.first() {
            Some(m) => format!("Notes to self ({})", name_of(&m.user_id)?),
            None => "Notes to self".into(),
        },
        ConversationKind::Dm => {
            let others: Vec<String> = c.members.iter().filter(|m| Some(m.user_id.as_str()) != perspective).map(|m| name_of(&m.user_id)).collect::<AppResult<_>>()?;
            if perspective.is_some() {
                others.first().cloned().unwrap_or_else(|| "Direct message".into())
            } else {
                others.join(" & ")
            }
        }
    })
}

fn last_message(conn: &Connection, conv_id: &str, as_user: Option<&str>) -> AppResult<Option<Message>> {
    let vis = match as_user {
        Some(_) => format!("AND {}", visible_sql("?2")),
        None => "AND ?2 IS NULL".to_string(),
    };
    let m = conn
        .query_row(
            &format!("SELECT {MSG_COLS} FROM messages m WHERE m.conv_id=?1 {vis} AND m.deleted=0 ORDER BY m.seq DESC LIMIT 1"),
            params![conv_id, as_user],
            msg_base,
        )
        .optional()?;
    m.map(|m| hydrate(conn, vec![m]).map(|mut v| v.remove(0))).transpose()
}

pub async fn viewer_conversations(State(st): State<St>, Admin(_a): Admin, Query(p): Query<ConvParams>) -> J<Vec<ViewerConversation>> {
    Ok(Json(
        st.db
            .call(move |c| {
                let ids: Vec<String> = match (&p.user_id, &p.kind) {
                    (Some(u), _) => user_conversation_ids(c, u)?,
                    (None, Some(k)) => {
                        let mut s = c.prepare("SELECT id FROM conversations WHERE kind=?1 ORDER BY COALESCE(last_ts, created_ms) DESC")?;
                        let v = s.query_map([k], |r| r.get::<_, String>(0))?.collect::<Result<Vec<_>, _>>()?;
                        v
                    }
                    (None, None) => {
                        let mut s = c.prepare("SELECT id FROM conversations WHERE kind IN ('channel','group_dm') ORDER BY COALESCE(last_ts, created_ms) DESC")?;
                        let v = s.query_map([], |r| r.get::<_, String>(0))?.collect::<Result<Vec<_>, _>>()?;
                        v
                    }
                };
                let needle = p.q.as_ref().map(|q| q.trim().to_lowercase()).filter(|q| !q.is_empty());
                let mut out = vec![];
                for id in ids {
                    let conversation = load_conversation(c, &id)?;
                    let title = title_for(c, &conversation, p.user_id.as_deref())?;
                    if let Some(n) = &needle {
                        if !title.to_lowercase().contains(n) {
                            continue;
                        }
                    }
                    let message_count: i64 = c.query_row("SELECT COUNT(*) FROM messages WHERE conv_id=?1", [&id], |r| r.get(0))?;
                    let last = last_message(c, &id, p.user_id.as_deref())?;
                    out.push(ViewerConversation { conversation, title, message_count, last_message: last });
                }
                Ok(out)
            })
            .await?,
    ))
}

#[derive(Deserialize, Default)]
pub struct ViewerMsgParams {
    as_user: Option<String>,
    before: Option<i64>,
    after: Option<i64>,
    around_id: Option<String>,
    around_ts: Option<i64>,
    from_ms: Option<i64>,
    to_ms: Option<i64>,
    /// Only messages of this thread (replies).
    thread_root: Option<String>,
    /// `true`: hide thread replies (the channel timeline).
    top_level: Option<bool>,
    limit: Option<i64>,
}

pub async fn viewer_messages(State(st): State<St>, Admin(_a): Admin, Path(conv): Path<String>, Query(p): Query<ViewerMsgParams>) -> J<ViewerHistory> {
    Ok(Json(st.db.call(move |c| viewer_history(c, &conv, &p)).await?))
}

fn viewer_history(c: &Connection, conv: &str, p: &ViewerMsgParams) -> AppResult<ViewerHistory> {
    conv_row(c, conv)?;
    let limit = p.limit.unwrap_or(50).clamp(1, 200);
    // Shared WHERE (?1 = conversation, ?2 = as_user or NULL).
    let mut cond = String::from("m.conv_id=?1");
    if p.as_user.is_some() {
        cond.push_str(&format!(" AND {}", visible_sql("?2")));
    } else {
        cond.push_str(" AND (?2 IS NULL)");
    }
    let mut extra: Vec<Box<dyn rusqlite::ToSql>> = vec![];
    let mut add = |sql: &str, v: Box<dyn rusqlite::ToSql>, cond: &mut String| {
        extra.push(v);
        cond.push_str(&format!(" AND {}", sql.replace("?N", &format!("?{}", extra.len() + 2))));
    };
    if let Some(f) = p.from_ms {
        add("m.ts>=?N", Box::new(f), &mut cond);
    }
    if let Some(t) = p.to_ms {
        add("m.ts<=?N", Box::new(t), &mut cond);
    }
    if let Some(t) = &p.thread_root {
        add("m.thread_root=?N", Box::new(t.clone()), &mut cond);
    } else if p.top_level.unwrap_or(false) {
        cond.push_str(" AND m.thread_root IS NULL");
    }
    let run = |extra_cond: &str, order: &str, lim: i64, more: &[&dyn rusqlite::ToSql]| -> AppResult<Vec<Message>> {
        let n0 = extra.len() + 3;
        let ec = extra_cond.replace("?A", &format!("?{n0}"));
        let sql = format!("SELECT {MSG_COLS} FROM messages m WHERE {cond} {ec} ORDER BY m.seq {order} LIMIT {lim}");
        let mut st = c.prepare(&sql)?;
        let mut args: Vec<&dyn rusqlite::ToSql> = vec![&conv, &p.as_user];
        for e in &extra {
            args.push(e.as_ref());
        }
        args.extend_from_slice(more);
        let v = st.query_map(args.as_slice(), msg_base)?.collect::<Result<Vec<_>, _>>()?;
        Ok(v)
    };
    let around_seq: Option<i64> = match (&p.around_id, p.around_ts) {
        (Some(id), _) => c.query_row("SELECT seq FROM messages WHERE id=?1 AND conv_id=?2", params![id, conv], |r| r.get(0)).optional()?,
        (None, Some(ts)) => run("AND m.ts>=?A", "ASC", 1, &[&ts])?.first().map(|m| m.seq),
        _ => None,
    };
    let mut msgs = if let Some(pivot) = around_seq {
        let half = (limit / 2).max(1);
        let mut before = run("AND m.seq<?A", "DESC", half, &[&pivot])?;
        before.reverse();
        before.extend(run("AND m.seq>=?A", "ASC", limit - half, &[&pivot])?);
        before
    } else if let Some(a) = p.after {
        run("AND m.seq>?A", "ASC", limit, &[&a])?
    } else if let Some(b) = p.before {
        let mut v = run("AND m.seq<?A", "DESC", limit, &[&b])?;
        v.reverse();
        v
    } else {
        let mut v = run("", "DESC", limit, &[])?;
        v.reverse();
        v
    };
    let has_before = match msgs.first() {
        Some(m) => !run("AND m.seq<?A", "DESC", 1, &[&m.seq])?.is_empty(),
        None => false,
    };
    let has_after = match msgs.last() {
        Some(m) => !run("AND m.seq>?A", "ASC", 1, &[&m.seq])?.is_empty(),
        None => false,
    };
    msgs = hydrate(c, msgs)?;
    let mut reply_counts = BTreeMap::new();
    for m in &msgs {
        let n: i64 = c.query_row("SELECT COUNT(*) FROM messages WHERE thread_root=?1 AND deleted=0", [&m.id], |r| r.get(0))?;
        if n > 0 {
            reply_counts.insert(m.id.clone(), n);
        }
    }
    Ok(ViewerHistory { messages: msgs, has_more_before: has_before, has_more_after: has_after, reply_counts })
}

#[derive(Deserialize, Default)]
pub struct ViewerSearchParams {
    q: Option<String>,
    /// Search as this user (their conversations and visibility); default: everything.
    user_id: Option<String>,
    conv_id: Option<String>,
    sender: Option<String>,
    from_ms: Option<i64>,
    to_ms: Option<i64>,
    has: Option<String>,
    limit: Option<i64>,
}

pub async fn viewer_search(State(st): State<St>, Admin(_a): Admin, Query(p): Query<ViewerSearchParams>) -> J<Vec<SearchHit>> {
    let hub = st.hub.clone();
    Ok(Json(
        st.db
            .call(move |c| {
                if let Some(u) = &p.user_id {
                    let sp = SearchParams {
                        q: p.q.clone().unwrap_or_default(),
                        from: p.sender.clone(),
                        in_conv: p.conv_id.clone(),
                        before: p.to_ms.map(|t| t + 1),
                        after: p.from_ms.map(|t| t - 1),
                        has: p.has.clone(),
                        kind: None,
                        limit: p.limit,
                    };
                    return Ok(run_search(c, &hub, u, &sp)?.hits);
                }
                let limit = p.limit.unwrap_or(50).clamp(1, 200);
                let fts = p.q.as_deref().and_then(fts_query);
                let mut sql = format!(
                    "SELECT {MSG_COLS}, {} FROM messages m ",
                    if fts.is_some() { "snippet(messages_fts,0,'[[',']]','…',12)" } else { "m.body" }
                );
                let mut args: Vec<Box<dyn rusqlite::ToSql>> = vec![];
                if let Some(f) = &fts {
                    sql.push_str("JOIN messages_fts ON messages_fts.rowid=m.rowid ");
                    args.push(Box::new(f.clone()));
                    sql.push_str("WHERE messages_fts MATCH ?1 ");
                } else {
                    sql.push_str("WHERE 1=1 ");
                }
                sql.push_str("AND m.deleted=0 ");
                if let Some(cv) = &p.conv_id {
                    args.push(Box::new(cv.clone()));
                    sql.push_str(&format!("AND m.conv_id=?{} ", args.len()));
                }
                if let Some(s) = &p.sender {
                    args.push(Box::new(s.trim_start_matches('@').to_string()));
                    sql.push_str(&format!("AND m.sender_id IN (SELECT id FROM users WHERE id=?{n} OR username=?{n} COLLATE NOCASE OR display_name=?{n} COLLATE NOCASE) ", n = args.len()));
                }
                if let Some(f) = p.from_ms {
                    args.push(Box::new(f));
                    sql.push_str(&format!("AND m.ts>=?{} ", args.len()));
                }
                if let Some(t) = p.to_ms {
                    args.push(Box::new(t));
                    sql.push_str(&format!("AND m.ts<=?{} ", args.len()));
                }
                for h in p.has.as_deref().unwrap_or("").split(',').map(str::trim) {
                    match h {
                        "file" => sql.push_str("AND EXISTS (SELECT 1 FROM message_attachments x WHERE x.message_id=m.id) "),
                        "link" => sql.push_str("AND (m.body LIKE '%http://%' OR m.body LIKE '%https://%') "),
                        "reaction" => sql.push_str("AND EXISTS (SELECT 1 FROM reactions x WHERE x.message_id=m.id) "),
                        _ => {}
                    }
                }
                sql.push_str(&format!("ORDER BY m.ts DESC LIMIT {limit}"));
                let mut stmt = c.prepare(&sql)?;
                let refs: Vec<&dyn rusqlite::ToSql> = args.iter().map(|b| b.as_ref()).collect();
                let rows = stmt.query_map(refs.as_slice(), |r| Ok((msg_base(r)?, r.get::<_, String>(18)?)))?.collect::<Result<Vec<_>, _>>()?;
                let snippets: Vec<String> = rows.iter().map(|r| r.1.clone()).collect();
                let msgs = hydrate(c, rows.into_iter().map(|r| r.0).collect())?;
                Ok(msgs.into_iter().zip(snippets).map(|(message, snippet)| SearchHit { message, snippet }).collect())
            })
            .await?,
    ))
}

// ---------------------------------------------------------------- deleting messages

fn target_ids(c: &Connection, req: &AdminDeleteMessagesRequest) -> AppResult<Vec<String>> {
    if !req.message_ids.is_empty() {
        let json = serde_json::to_string(&req.message_ids)?;
        let mut s = c.prepare("SELECT id FROM messages WHERE id IN (SELECT value FROM json_each(?1)) AND deleted=0")?;
        let v = s.query_map([json], |r| r.get::<_, String>(0))?.collect::<Result<Vec<_>, _>>()?;
        return Ok(v);
    }
    let conv = req.conv_id.clone().ok_or_else(|| AppError::validation("choose messages, or a conversation with a date range"))?;
    let (from, to) = match (req.before_ms, req.from_ms, req.to_ms) {
        (Some(b), _, _) => (0, b - 1),
        (None, Some(f), Some(t)) => (f, t),
        _ => return Err(AppError::validation("give a date range or a \"before\" date")),
    };
    if to < from {
        return Err(AppError::validation("the end date is before the start date"));
    }
    let mut s = c.prepare("SELECT id FROM messages WHERE conv_id=?1 AND ts>=?2 AND ts<=?3 AND deleted=0 ORDER BY seq")?;
    let v = s.query_map(params![conv, from, to], |r| r.get::<_, String>(0))?.collect::<Result<Vec<_>, _>>()?;
    Ok(v)
}

/// Writes (and verifies) a backup of the given messages. Errors are `backup_failed`.
async fn backup_messages(st: &St, ids: &[String], scope: &str, user_id: Option<&str>) -> AppResult<(BackupInfo, String)> {
    let fail = |m: String| AppError::new(ErrorCode::BackupFailed, format!("backup failed, nothing was changed: {m}"));
    if st.migrating.load(std::sync::atomic::Ordering::SeqCst) {
        return Err(fail("a storage move is running".into()));
    }
    let (ids2, scope2, uid2) = (ids.to_vec(), scope.to_string(), user_id.map(str::to_string));
    let doc = st
        .db
        .call(move |c| {
            let mut msgs = vec![];
            let mut atts = vec![];
            let mut seen = std::collections::HashSet::new();
            for id in &ids2 {
                let m = load_message(c, id)?;
                for a in &m.attachments {
                    if seen.insert(a.id.clone()) {
                        let (sha, key): (String, String) = c.query_row("SELECT sha256, storage_key FROM attachments WHERE id=?1", [&a.id], |r| Ok((r.get(0)?, r.get(1)?)))?;
                        atts.push(json!({"id": a.id, "name": a.name, "content_type": a.content_type, "size": a.size, "sha256": sha, "storage_key": key, "removed": a.removed}));
                    }
                }
                msgs.push(m);
            }
            Ok(json!({"format": "workchat-messages-backup", "version": 1, "created_ms": now_ms(), "scope": scope2, "user_id": uid2, "messages": msgs, "attachments": atts}))
        })
        .await
        .map_err(|e| fail(e.message))?;
    let id = new_id();
    let prefix = format!("backups/messages/{id}");
    let plain = serde_json::to_vec_pretty(&doc).map_err(|e| fail(e.to_string()))?;
    let pass = crate::fullbackup::passphrase(st);
    let (key, bytes) = match &pass {
        Some(p) => {
            let (p2, wf) = (p.clone(), st.cfg.backup_work_factor);
            let enc = tokio::task::spawn_blocking(move || crate::crypto::encrypt_bytes(&plain, &p2, wf)).await.map_err(|e| fail(e.to_string()))?.map_err(|e| fail(e.message))?;
            (format!("{prefix}/data.json.age"), Bytes::from(enc))
        }
        None => (format!("{prefix}/data.json"), Bytes::from(plain)),
    };
    let size = bytes.len() as u64;
    let storage = st.storage();
    storage.put_bytes(&key, bytes, "application/json").await.map_err(|e| fail(e.message))?;
    match storage.head(&key).await {
        Ok(Some(s)) if s == size => {}
        other => {
            let _ = storage.delete(&key).await;
            return Err(fail(format!("verification failed ({other:?})")));
        }
    }
    let info = BackupInfo { id: id.clone(), kind: "messages".into(), user_id: user_id.map(str::to_string), created_ms: now_ms(), reason: "admin_delete_messages".into(), key, size: size as i64, attachments: 0, encrypted: pass.is_some() };
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
        .map_err(|e| fail(e.message))?;
    Ok((info, prefix))
}

pub async fn delete_messages(State(st): State<St>, Admin(_a): Admin, Json(req): Json<AdminDeleteMessagesRequest>) -> J<AdminDeleteMessagesResponse> {
    match req.scope.as_str() {
        "user" => {
            let uid = req.user_id.clone().ok_or_else(|| AppError::validation("scope \"user\" needs user_id"))?;
            let range = match (req.before_ms, req.from_ms, req.to_ms) {
                (Some(b), _, _) => Some((0, b - 1)),
                (None, Some(f), Some(t)) => Some((f, t)),
                _ => None,
            };
            if req.message_ids.is_empty() && (range.is_none() || req.conv_id.is_none()) {
                return Err(AppError::validation("choose messages, or a conversation with a date range"));
            }
            let (b, out) = backup::hide_with_backup(
                &st,
                HideRequest {
                    user_id: uid,
                    range: if req.message_ids.is_empty() { range } else { None },
                    conversation_id: req.conv_id.clone(),
                    also_others: false,
                    message_ids: if req.message_ids.is_empty() { None } else { Some(req.message_ids.clone()) },
                },
                "admin_delete_for_user",
            )
            .await?;
            Ok(Json(AdminDeleteMessagesResponse { backup_id: b.info.id, messages_deleted: out.messages_hidden, attachments_moved: out.attachments_moved }))
        }
        "everyone" => {
            let req2 = req.clone();
            let ids = st.db.call(move |c| target_ids(c, &req2)).await?;
            if ids.is_empty() {
                return Err(AppError::validation("no messages match"));
            }
            let (info, prefix) = backup_messages(&st, &ids, "everyone", None).await?;
            // Attachments that only these messages reference move into the backup folder (not erased).
            let ids_json = serde_json::to_string(&ids)?;
            let movable: Vec<(String, String)> = st
                .db
                .call(move |c| {
                    let mut s = c.prepare(
                        "SELECT DISTINCT a.id, a.storage_key FROM message_attachments ma JOIN attachments a ON a.id=ma.attachment_id
                         WHERE ma.message_id IN (SELECT value FROM json_each(?1)) AND a.backup_id IS NULL AND a.removed=0 AND a.purpose='file'
                         AND NOT EXISTS (SELECT 1 FROM message_attachments m2 JOIN messages mm ON mm.id=m2.message_id
                                         WHERE m2.attachment_id=a.id AND mm.deleted=0 AND m2.message_id NOT IN (SELECT value FROM json_each(?1)))",
                    )?;
                    let v = s.query_map([ids_json], |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?)))?.collect::<Result<Vec<_>, _>>()?;
                    Ok(v)
                })
                .await?;
            let storage = st.storage();
            let mut done: Vec<(String, String, String)> = vec![];
            for (aid, key) in &movable {
                let new_key = backup_attachment_key(&prefix, aid);
                if let Err(e) = storage.rename(key, &new_key).await {
                    for (_, old, new) in done.iter().rev() {
                        let _ = storage.rename(new, old).await;
                    }
                    return Err(AppError::new(ErrorCode::BackupFailed, format!("backup failed, nothing was changed: could not move attachment {aid} into the backup folder: {}", e.message)));
                }
                done.push((aid.clone(), key.clone(), new_key));
            }
            let hub = st.hub.clone();
            let (ids2, bid, done2) = (ids.clone(), info.id.clone(), done.clone());
            let res = st
                .db
                .call(move |c| {
                    let tx = c.transaction()?;
                    for (aid, _old, new) in &done2 {
                        tx.execute("UPDATE attachments SET storage_key=?2, backup_id=?3 WHERE id=?1", params![aid, new, bid])?;
                    }
                    let mut em = Emitter::new();
                    let mut keys = vec![];
                    for id in &ids2 {
                        keys.extend(tombstone_message(&tx, &mut em, id)?);
                    }
                    tx.commit()?;
                    em.flush(&hub);
                    Ok(keys)
                })
                .await;
            let keys = match res {
                Ok(k) => k,
                Err(e) => {
                    for (_, old, new) in done.iter().rev() {
                        let _ = storage.rename(new, old).await;
                    }
                    return Err(e);
                }
            };
            for k in keys {
                let _ = storage.delete(&k).await;
            }
            Ok(Json(AdminDeleteMessagesResponse { backup_id: info.id, messages_deleted: ids.len() as i64, attachments_moved: done.len() as i64 }))
        }
        _ => Err(AppError::validation("scope must be \"everyone\" or \"user\"")),
    }
}

/// `PUT /admin/messages/{id}/ts` (owner or admin): changes the time a message shows, for importing history and for the
/// seed tool. The time must not be in the future and must stay between the times of the neighbouring messages of the
/// conversation (so order by `seq` and order by time always agree). Connected members receive a `message_edit` event
/// carrying the message with its new `ts`; the message is not marked edited.
pub async fn set_message_ts(State(st): State<St>, Admin(_a): Admin, Path(id): Path<String>, Json(req): Json<AdminSetMessageTs>) -> J<Message> {
    let hub = st.hub.clone();
    Ok(Json(
        st.db
            .call(move |c| {
                let (conv, seq): (String, i64) = c
                    .query_row("SELECT conv_id, seq FROM messages WHERE id=?1", [&id], |r| Ok((r.get(0)?, r.get(1)?)))
                    .optional()?
                    .ok_or_else(|| AppError::not_found("message not found"))?;
                if req.ts > now_ms() {
                    return Err(AppError::validation("ts is in the future"));
                }
                let prev: Option<i64> = c.query_row("SELECT MAX(ts) FROM messages WHERE conv_id=?1 AND seq<?2", params![conv, seq], |r| r.get(0))?;
                let next: Option<i64> = c.query_row("SELECT MIN(ts) FROM messages WHERE conv_id=?1 AND seq>?2", params![conv, seq], |r| r.get(0))?;
                if prev.map(|p| req.ts < p).unwrap_or(false) || next.map(|n| req.ts > n).unwrap_or(false) {
                    return Err(AppError::validation("ts must stay between the times of the neighbouring messages"));
                }
                let tx = c.transaction()?;
                tx.execute("UPDATE messages SET ts=?2 WHERE id=?1", params![id, req.ts])?;
                tx.execute("UPDATE conversations SET last_ts=(SELECT MAX(ts) FROM messages WHERE conv_id=?1) WHERE id=?1", [&conv])?;
                let message = load_message(&tx, &id)?;
                let mut em = Emitter::new();
                for u in recipients(&tx, &conv, &id)? {
                    em.emit(&tx, &u, EventData::MessageEdit { message: message.clone() })?;
                }
                tx.commit()?;
                em.flush(&hub);
                Ok(message)
            })
            .await?,
    ))
}
