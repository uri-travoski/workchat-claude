//! Selective restore (merge by default, "replace" optional) from per-user backups, full backups and
//! deleted-message backups. Attachments that were moved into a backup folder are moved back.
//!
//! Merge adds what is missing: the account if it was deleted, conversations and memberships, messages
//! that no longer exist, and un-hides messages that were hidden for the user. Replace also hides
//! everything the user currently sees that is not in the backup, and resets the profile to the
//! backup. Neither mode edits other people's data: shared messages keep their current content.

use crate::backup::read_user_backup;
use crate::db::now_ms;
use crate::emit::Emitter;
use crate::error::{AppError, AppResult};
use crate::hub::Hub;
use crate::queries::*;
use crate::state::St;
use crate::storage::attachment_key;
use proto::*;
use rusqlite::{params, Connection, OptionalExtension};
use serde_json::Value;
use std::collections::{BTreeSet, HashMap};

fn s(v: &Value, k: &str) -> Option<String> {
    v.get(k).and_then(|x| x.as_str()).map(str::to_string)
}
fn i(v: &Value, k: &str) -> Option<i64> {
    v.get(k).and_then(|x| x.as_i64())
}
fn b(v: &Value, k: &str) -> bool {
    v.get(k).and_then(|x| x.as_bool()).unwrap_or(false)
}

pub async fn restore(st: &St, req: &RestoreRequest) -> AppResult<RestoreResponse> {
    let mode = req.mode.clone().unwrap_or_else(|| "merge".into());
    if !matches!(mode.as_str(), "merge" | "replace") {
        return Err(AppError::validation("mode must be merge or replace"));
    }
    let replace = mode == "replace";
    let bid = req.backup_id.clone();
    let row: (String, String, Option<String>) = st
        .db
        .call(move |c| {
            c.query_row("SELECT kind,key,user_id FROM backups WHERE id=?1", [&bid], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))
                .optional()?
                .ok_or_else(|| AppError::not_found("backup not found"))
        })
        .await?;
    let (kind, key, owner) = row;
    if kind != "messages" && !req.all && req.user_ids.is_empty() {
        return Err(AppError::validation("choose at least one user, or restore all users"));
    }
    let mut resp = RestoreResponse::default();
    match kind.as_str() {
        "user" => {
            let doc = read_user_backup(st, &key).await?;
            let uid = owner.unwrap_or_default();
            if !req.all && !req.user_ids.contains(&uid) {
                return Err(AppError::validation("this backup belongs to a different user"));
            }
            apply_doc(st, doc, replace, &mut resp).await?;
        }
        "full" => {
            let snap = crate::fullbackup::open_full_backup(st, &key).await?;
            let r = docs_from_snapshot(&snap, req).await;
            let _ = tokio::fs::remove_file(&snap).await;
            let (docs, warnings) = r?;
            resp.warnings.extend(warnings);
            for d in docs {
                apply_doc(st, d, replace, &mut resp).await?;
            }
        }
        "messages" => {
            return restore_messages_backup(st, &key).await;
        }
        _ => return Err(AppError::validation("unknown backup kind")),
    }
    Ok(resp)
}

async fn docs_from_snapshot(snap: &std::path::Path, req: &RestoreRequest) -> AppResult<(Vec<Value>, Vec<String>)> {
    let (snap, ids, all) = (snap.to_path_buf(), req.user_ids.clone(), req.all);
    tokio::task::spawn_blocking(move || {
        let c = Connection::open_with_flags(&snap, rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY).map_err(|e| AppError::internal(e.to_string()))?;
        let wanted: Vec<String> = if all {
            let mut st = c.prepare("SELECT id FROM users WHERE deleted=0 AND is_bot=0")?;
            let v = st.query_map([], |r| r.get::<_, String>(0))?.collect::<Result<Vec<_>, _>>()?;
            v
        } else {
            ids
        };
        let mut docs = vec![];
        let mut warnings = vec![];
        for u in wanted {
            match crate::backup::build_user_doc(&c, &u) {
                Ok(d) => docs.push(d),
                Err(_) => warnings.push(format!("user {u} is not in that backup")),
            }
        }
        Ok((docs, warnings))
    })
    .await
    .map_err(|e| AppError::internal(e.to_string()))?
}

/// Applies one per-user document, then moves attachments back out of the backup folder.
async fn apply_doc(st: &St, doc: Value, replace: bool, resp: &mut RestoreResponse) -> AppResult<()> {
    let hub = st.hub.clone();
    let (r, att_ids) = st
        .db
        .call(move |c| {
            let mut r = RestoreResponse::default();
            let atts = apply_user_doc(c, &hub, &doc, replace, &mut r)?;
            Ok((r, atts))
        })
        .await?;
    resp.users_restored += r.users_restored;
    resp.conversations_created += r.conversations_created;
    resp.messages_added += r.messages_added;
    resp.messages_unhidden += r.messages_unhidden;
    resp.messages_hidden += r.messages_hidden;
    resp.warnings.extend(r.warnings);
    resp.attachments_moved_back += move_back(st, &att_ids, &mut resp.warnings).await?;
    Ok(())
}

/// Moves attachments from their backup folder back to `att/<id>` and re-activates them.
pub async fn move_back(st: &St, att_ids: &[String], warnings: &mut Vec<String>) -> AppResult<i64> {
    let storage = st.storage();
    let mut moved = 0;
    for id in att_ids {
        let id2 = id.clone();
        let row: Option<String> = st
            .db
            .call(move |c| Ok(c.query_row("SELECT storage_key FROM attachments WHERE id=?1 AND backup_id IS NOT NULL AND removed=0", [&id2], |r| r.get(0)).optional()?))
            .await?;
        let Some(from) = row else { continue };
        let to = attachment_key(id);
        match storage.rename(&from, &to).await {
            Ok(()) => {
                let (id3, to2) = (id.clone(), to.clone());
                st.db.call(move |c| Ok(c.execute("UPDATE attachments SET storage_key=?2, backup_id=NULL WHERE id=?1", params![id3, to2])?)).await?;
                moved += 1;
            }
            Err(e) => warnings.push(format!("could not move attachment {id} back: {}", e.message)),
        }
    }
    Ok(moved)
}

fn user_exists(conn: &Connection, id: &str) -> AppResult<Option<UserRow>> {
    Ok(conn
        .query_row(&format!("SELECT {USER_COLS} FROM users WHERE id=?1"), [id], user_row)
        .optional()?)
}

pub fn apply_user_doc(conn: &mut Connection, hub: &Hub, doc: &Value, replace: bool, resp: &mut RestoreResponse) -> AppResult<Vec<String>> {
    let u = doc.get("user").ok_or_else(|| AppError::validation("the backup has no user section"))?;
    let uid = s(u, "id").ok_or_else(|| AppError::validation("the backup has no user id"))?;
    let tx = conn.transaction()?;
    let mut em = Emitter::new();
    // ---------------------------------------------------------------- account
    let username_doc = s(u, "username").unwrap_or_else(|| uid.clone());
    let free_name = |tx: &Connection, want: &str| -> AppResult<String> {
        let taken: i64 = tx.query_row("SELECT COUNT(*) FROM users WHERE username=?1 AND id!=?2", params![want, uid], |r| r.get(0))?;
        Ok(if taken > 0 { format!("{want}-restored") } else { want.to_string() })
    };
    let role_doc = Role::parse(&s(u, "role").unwrap_or_default()).unwrap_or(Role::Member);
    let role = if role_doc == Role::Owner {
        let owners: i64 = tx.query_row("SELECT COUNT(*) FROM users WHERE role='owner' AND id!=?1", [&uid], |r| r.get(0))?;
        if owners > 0 { Role::Admin } else { Role::Owner }
    } else {
        role_doc
    };
    match user_exists(&tx, &uid)? {
        None => {
            let name = free_name(&tx, &username_doc)?;
            tx.execute(
                "INSERT INTO users(id,username,display_name,email,password_hash,password_set,role,disabled,deleted,privacy_controls_enabled,hide_last_active,avatar_attachment,created_ms)
                 VALUES(?1,?2,?3,?4,?5,?6,?7,?8,0,?9,?10,?11,?12)",
                params![
                    uid, name, s(u, "display_name").unwrap_or_else(|| name.clone()), s(u, "email"), s(u, "password_hash").unwrap_or_default(),
                    b(u, "password_set") as i64, role.as_str(), b(u, "disabled") as i64, b(u, "privacy_controls_enabled") as i64,
                    b(u, "hide_last_active") as i64, s(u, "avatar"), i(u, "created_ms").unwrap_or_else(now_ms)
                ],
            )?;
            resp.users_restored += 1;
            crate::users::ensure_self_conversation(&tx, &uid)?;
            crate::users::announce_user(&tx, hub, &uid, &mut em)?;
        }
        Some(row) if row.deleted => {
            let name = free_name(&tx, &username_doc)?;
            tx.execute(
                "UPDATE users SET deleted=0, disabled=?2, username=?3, display_name=?4, email=?5, password_hash=?6, password_set=?7, role=?8, avatar_attachment=?9 WHERE id=?1",
                params![uid, b(u, "disabled") as i64, name, s(u, "display_name").unwrap_or_else(|| name.clone()), s(u, "email"), s(u, "password_hash").unwrap_or_default(), b(u, "password_set") as i64, role.as_str(), s(u, "avatar")],
            )?;
            resp.users_restored += 1;
            crate::users::announce_user(&tx, hub, &uid, &mut em)?;
        }
        Some(_) => {
            resp.users_restored += 1;
            if replace {
                tx.execute(
                    "UPDATE users SET display_name=?2, email=?3, hide_last_active=?4, avatar_attachment=?5 WHERE id=?1",
                    params![uid, s(u, "display_name").unwrap_or_default(), s(u, "email"), b(u, "hide_last_active") as i64, s(u, "avatar")],
                )?;
                crate::users::announce_user(&tx, hub, &uid, &mut em)?;
            }
        }
    }
    // ---------------------------------------------------------------- conversations
    let mut map: HashMap<String, String> = HashMap::new();
    let mut touched: BTreeSet<String> = BTreeSet::new();
    let empty = vec![];
    for cv in doc["conversations"].as_array().unwrap_or(&empty) {
        let c = &cv["conversation"];
        let Some(doc_id) = s(c, "id") else { continue };
        let kind = s(c, "kind").unwrap_or_else(|| "channel".into());
        let dm_key = s(cv, "dm_key");
        let live: Option<String> = tx
            .query_row("SELECT id FROM conversations WHERE id=?1", [&doc_id], |r| r.get(0))
            .optional()?
            .or(match &dm_key {
                Some(k) => tx.query_row("SELECT id FROM conversations WHERE dm_key=?1", [k], |r| r.get(0)).optional()?,
                None => None,
            });
        let live_id = match live {
            Some(l) => l,
            None => {
                tx.execute(
                    "INSERT INTO conversations(id,kind,name,topic,archived,created_by,created_ms,dm_key,last_seq,last_ts) VALUES(?1,?2,?3,?4,?5,?6,?7,?8,0,NULL)",
                    params![doc_id, kind, s(c, "name"), s(c, "topic"), b(c, "archived") as i64, uid, i(c, "created_ms").unwrap_or_else(now_ms), dm_key],
                )?;
                resp.conversations_created += 1;
                // Members of a conversation we had to re-create come back too (if their accounts exist).
                for m in c["members"].as_array().unwrap_or(&empty) {
                    if let Some(mu) = s(m, "user_id") {
                        if let Some(r) = user_exists(&tx, &mu)? {
                            if !r.deleted && !r.disabled {
                                tx.execute("INSERT OR IGNORE INTO members(conv_id,user_id,role,joined_ms) VALUES(?1,?2,?3,?4)", params![doc_id, mu, s(m, "role").unwrap_or_else(|| "member".into()), now_ms()])?;
                            }
                        }
                    }
                }
                doc_id.clone()
            }
        };
        tx.execute("INSERT OR IGNORE INTO members(conv_id,user_id,role,joined_ms) VALUES(?1,?2,'member',?3)", params![live_id, uid, now_ms()])?;
        map.insert(doc_id, live_id.clone());
        touched.insert(live_id);
    }
    // ---------------------------------------------------------------- messages
    let doc_atts: HashMap<String, &Value> = doc["attachments"].as_array().unwrap_or(&empty).iter().filter_map(|a| s(a, "id").map(|id| (id, a))).collect();
    let mut need_move: BTreeSet<String> = BTreeSet::new();
    let mut doc_ids: HashMap<String, Vec<String>> = HashMap::new();
    let mut msgs: Vec<&Value> = doc["messages"].as_array().unwrap_or(&empty).iter().collect();
    msgs.sort_by_key(|m| (s(m, "conv_id").unwrap_or_default(), i(m, "seq").unwrap_or(0)));
    for m in msgs {
        let (Some(mid), Some(doc_conv)) = (s(m, "id"), s(m, "conv_id")) else { continue };
        let Some(conv) = map.get(&doc_conv).cloned() else { continue };
        doc_ids.entry(conv.clone()).or_default().push(mid.clone());
        let live: Option<(i64, String)> = tx.query_row("SELECT deleted, conv_id FROM messages WHERE id=?1", [&mid], |r| Ok((r.get(0)?, r.get(1)?))).optional()?;
        match live {
            Some((deleted, _)) => {
                let n = tx.execute("DELETE FROM message_hidden WHERE user_id=?1 AND message_id=?2", params![uid, mid])?;
                if n > 0 && deleted == 0 {
                    resp.messages_unhidden += 1;
                }
                if deleted == 0 {
                    for a in m["attachments"].as_array().unwrap_or(&empty) {
                        if let Some(aid) = s(a, "id") {
                            need_move.insert(aid);
                        }
                    }
                }
            }
            None => {
                let mut seq = i(m, "seq").unwrap_or(0);
                let taken: i64 = tx.query_row("SELECT COUNT(*) FROM messages WHERE conv_id=?1 AND seq=?2", params![conv, seq], |r| r.get(0))?;
                if taken > 0 || seq <= 0 {
                    seq = tx.query_row("SELECT last_seq+1 FROM conversations WHERE id=?1", [&conv], |r| r.get(0))?;
                }
                let ts = i(m, "ts").unwrap_or_else(now_ms);
                let fwd = m.get("forwarded").filter(|f| !f.is_null()).map(|f| f.to_string());
                tx.execute(
                    "INSERT INTO messages(id,conv_id,seq,sender_id,client_id,body,kind,ts,version,edited_ms,deleted,thread_root,reply_to,mentions,delivered_ms,fwd,form,also_channel)
                     VALUES(?1,?2,?3,?4,NULL,?5,?6,?7,?8,?9,?15,?10,?11,?12,?13,?14,?16,?17)",
                    params![
                        mid, conv, seq, s(m, "sender_id").unwrap_or_default(), s(m, "body").unwrap_or_default(),
                        match m["kind"].as_str() { Some("file") => "file", Some("system") => "system", _ => "text" },
                        ts, i(m, "version").unwrap_or(1), i(m, "edited_ms"), s(m, "thread_root"), s(m, "reply_to"),
                        m.get("mentions").map(|v| v.to_string()).unwrap_or_else(|| "[]".into()),
                        if b(m, "delivered") { Some(ts) } else { None }, fwd, b(m, "deleted") as i64,
                        m.get("form").filter(|f| !f.is_null()).map(|f| f.to_string()),
                        b(m, "also_send_to_channel") as i64
                    ],
                )?;
                tx.execute("UPDATE conversations SET last_seq=MAX(last_seq,?2), last_ts=MAX(COALESCE(last_ts,0),?3) WHERE id=?1", params![conv, seq, ts])?;
                for a in m["attachments"].as_array().unwrap_or(&empty) {
                    let Some(aid) = s(a, "id") else { continue };
                    let have: i64 = tx.query_row("SELECT COUNT(*) FROM attachments WHERE id=?1", [&aid], |r| r.get(0))?;
                    if have == 0 {
                        if let Some(d) = doc_atts.get(&aid) {
                            tx.execute(
                                "INSERT INTO attachments(id,uploader_id,name,content_type,size,sha256,storage_key,purpose,created_ms,removed) VALUES(?1,?2,?3,?4,?5,?6,?7,'file',?8,?9)",
                                params![aid, s(m, "sender_id").unwrap_or_default(), s(d, "name").unwrap_or_default(), s(d, "content_type").unwrap_or_default(), i(d, "size").unwrap_or(0), s(d, "sha256").unwrap_or_default(), s(d, "storage_key").unwrap_or_else(|| attachment_key(&aid)), ts, b(d, "removed") as i64],
                            )?;
                        } else {
                            resp.warnings.push(format!("attachment {aid} is not described in the backup"));
                            continue;
                        }
                    }
                    tx.execute("INSERT OR IGNORE INTO message_attachments(message_id,attachment_id) VALUES(?1,?2)", params![mid, aid])?;
                    need_move.insert(aid);
                }
                for rx in m["reactions"].as_array().unwrap_or(&empty) {
                    let emoji = s(rx, "emoji").unwrap_or_default();
                    for ru in rx["user_ids"].as_array().unwrap_or(&empty).iter().filter_map(|x| x.as_str()) {
                        if user_exists(&tx, ru)?.is_some() {
                            tx.execute("INSERT OR IGNORE INTO reactions(message_id,user_id,emoji,ts) VALUES(?1,?2,?3,?4)", params![mid, ru, emoji, ts])?;
                        }
                    }
                }
                tx.execute("DELETE FROM message_hidden WHERE user_id=?1 AND message_id=?2", params![uid, mid])?;
                resp.messages_added += 1;
            }
        }
    }
    // ---------------------------------------------------------------- read state and replace mode
    for cv in doc["conversations"].as_array().unwrap_or(&empty) {
        let Some(live) = s(&cv["conversation"], "id").and_then(|d| map.get(&d).cloned()) else { continue };
        let lr = i(cv, "last_read_seq").unwrap_or(0);
        let mu = i(cv, "marked_unread").unwrap_or(0);
        if replace {
            tx.execute(
                "INSERT INTO read_state(user_id,conv_id,last_read_seq,marked_unread) VALUES(?1,?2,?3,?4) ON CONFLICT(user_id,conv_id) DO UPDATE SET last_read_seq=excluded.last_read_seq, marked_unread=excluded.marked_unread",
                params![uid, live, lr, mu],
            )?;
        } else {
            tx.execute(
                "INSERT INTO read_state(user_id,conv_id,last_read_seq,marked_unread) VALUES(?1,?2,?3,0) ON CONFLICT(user_id,conv_id) DO UPDATE SET last_read_seq=MAX(last_read_seq,excluded.last_read_seq)",
                params![uid, live, lr],
            )?;
        }
        if replace {
            let keep = serde_json::to_string(doc_ids.get(&live).unwrap_or(&vec![]))?;
            let vis = visible_sql("?2");
            let extra: Vec<String> = {
                let mut st = tx.prepare(&format!("SELECT m.id FROM messages m WHERE m.conv_id=?1 AND {vis} AND m.id NOT IN (SELECT value FROM json_each(?3))"))?;
                let v = st.query_map(params![live, uid, keep], |r| r.get::<_, String>(0))?.collect::<Result<Vec<_>, _>>()?;
                v
            };
            for id in &extra {
                tx.execute("INSERT OR IGNORE INTO message_hidden(user_id,message_id) VALUES(?1,?2)", params![uid, id])?;
            }
            resp.messages_hidden += extra.len() as i64;
        }
    }
    // ---------------------------------------------------------------- labels and saved items
    if replace {
        tx.execute("DELETE FROM saved_labels WHERE user_id=?1", [&uid])?;
        tx.execute("DELETE FROM saved_items WHERE user_id=?1", [&uid])?;
        tx.execute("DELETE FROM labels WHERE user_id=?1", [&uid])?;
    }
    for l in doc["labels"].as_array().unwrap_or(&empty) {
        let (Some(id), Some(name)) = (s(l, "id"), s(l, "name")) else { continue };
        // A label id belongs to one user; a clash with someone else's label would be a corrupt file.
        let other: i64 = tx.query_row("SELECT COUNT(*) FROM labels WHERE id=?1 AND user_id!=?2", params![id, uid], |r| r.get(0))?;
        if other > 0 {
            continue;
        }
        let dup: i64 = tx.query_row("SELECT COUNT(*) FROM labels WHERE user_id=?1 AND name=?2 COLLATE NOCASE AND id!=?3", params![uid, name, id], |r| r.get(0))?;
        if dup > 0 {
            continue;
        }
        tx.execute(
            "INSERT OR IGNORE INTO labels(id,user_id,name,color,position) VALUES(?1,?2,?3,?4,?5)",
            params![id, uid, name, s(l, "color").unwrap_or_else(|| "#0dbd8b".into()), i(l, "position").unwrap_or(0)],
        )?;
    }
    for it in doc["saved"].as_array().unwrap_or(&empty) {
        let (Some(mid), Some(ms)) = (s(it, "message_id"), i(it, "saved_ms")) else { continue };
        let conv: Option<String> = tx.query_row("SELECT conv_id FROM messages WHERE id=?1", [&mid], |r| r.get(0)).optional()?;
        let Some(conv) = conv else { continue };
        tx.execute("INSERT OR IGNORE INTO saved_items(user_id,message_id,conv_id,saved_ms) VALUES(?1,?2,?3,?4)", params![uid, mid, conv, ms])?;
        for lid in it["label_ids"].as_array().unwrap_or(&empty) {
            if let Some(lid) = lid.as_str() {
                let ok: i64 = tx.query_row("SELECT COUNT(*) FROM labels WHERE id=?1 AND user_id=?2", params![lid, uid], |r| r.get(0))?;
                if ok > 0 {
                    tx.execute("INSERT OR IGNORE INTO saved_labels(user_id,message_id,label_id) VALUES(?1,?2,?3)", params![uid, mid, lid])?;
                }
            }
        }
    }
    // ---------------------------------------------------------------- private pictures
    if replace {
        tx.execute("DELETE FROM avatar_overrides WHERE viewer_id=?1", [&uid])?;
    }
    for o in doc["avatar_overrides"].as_array().unwrap_or(&empty) {
        let (Some(kind), Some(target), Some(att)) = (s(o, "kind"), s(o, "target"), s(o, "attachment")) else { continue };
        // Only pictures that still exist and belong to this person, for people and chats that still exist.
        let img: i64 = tx.query_row("SELECT COUNT(*) FROM attachments WHERE id=?1 AND uploader_id=?2 AND purpose='avatar_private' AND removed=0", params![att, uid], |r| r.get(0))?;
        let target_ok: i64 = match kind.as_str() {
            "user" => tx.query_row("SELECT COUNT(*) FROM users WHERE id=?1 AND deleted=0 AND id!=?2", params![target, uid], |r| r.get(0))?,
            "conv" => tx.query_row("SELECT COUNT(*) FROM members WHERE conv_id=?1 AND user_id=?2", params![target, uid], |r| r.get(0))?,
            _ => 0,
        };
        if img > 0 && target_ok > 0 {
            tx.execute(
                "INSERT OR REPLACE INTO avatar_overrides(viewer_id,kind,target,attachment_id,updated_ms) VALUES(?1,?2,?3,?4,?5)",
                params![uid, kind, target, att, crate::db::now_ms()],
            )?;
        }
    }
    // ---------------------------------------------------------------- announce
    for conv in &touched {
        crate::routes::chat::upsert_for_members(&tx, &mut em, conv)?;
    }
    em.emit(&tx, &uid, EventData::Resync {})?;
    tx.commit()?;
    em.flush(hub);
    Ok(need_move.into_iter().collect())
}

/// Restores messages that an admin deleted (kind `messages`): tombstones become messages again.
pub async fn restore_messages_backup(st: &St, key: &str) -> AppResult<RestoreResponse> {
    let doc = read_user_backup(st, key).await?;
    let hub = st.hub.clone();
    let (resp, att_ids) = st
        .db
        .call(move |c| {
            let mut r = RestoreResponse::default();
            let mut need_move = BTreeSet::new();
            let tx = c.transaction()?;
            let mut em = Emitter::new();
            let empty = vec![];
            let scope_user = s(&doc, "user_id").filter(|_| s(&doc, "scope").as_deref() == Some("user"));
            let doc_atts: HashMap<String, &Value> = doc["attachments"].as_array().unwrap_or(&empty).iter().filter_map(|a| s(a, "id").map(|id| (id, a))).collect();
            for m in doc["messages"].as_array().unwrap_or(&empty) {
                let Some(mid) = s(m, "id") else { continue };
                let live: Option<i64> = tx.query_row("SELECT deleted FROM messages WHERE id=?1", [&mid], |r| r.get(0)).optional()?;
                let Some(deleted) = live else {
                    r.warnings.push(format!("message {mid} no longer exists and was not re-created"));
                    continue;
                };
                if let Some(u) = &scope_user {
                    let n = tx.execute("DELETE FROM message_hidden WHERE user_id=?1 AND message_id=?2", params![u, mid])?;
                    r.messages_unhidden += n as i64;
                }
                if deleted != 0 {
                    tx.execute(
                        "UPDATE messages SET deleted=0, body=?2, version=?3, edited_ms=?4 WHERE id=?1",
                        params![mid, s(m, "body").unwrap_or_default(), i(m, "version").unwrap_or(1), i(m, "edited_ms")],
                    )?;
                    for a in m["attachments"].as_array().unwrap_or(&empty) {
                        let Some(aid) = s(a, "id") else { continue };
                        let have: i64 = tx.query_row("SELECT COUNT(*) FROM attachments WHERE id=?1", [&aid], |r| r.get(0))?;
                        if have == 0 {
                            if let Some(d) = doc_atts.get(&aid) {
                                tx.execute(
                                    "INSERT INTO attachments(id,uploader_id,name,content_type,size,sha256,storage_key,purpose,created_ms,removed) VALUES(?1,?2,?3,?4,?5,?6,?7,'file',?8,0)",
                                    params![aid, s(m, "sender_id").unwrap_or_default(), s(d, "name").unwrap_or_default(), s(d, "content_type").unwrap_or_default(), i(d, "size").unwrap_or(0), s(d, "sha256").unwrap_or_default(), s(d, "storage_key").unwrap_or_else(|| attachment_key(&aid)), i(m, "ts").unwrap_or(0)],
                                )?;
                            }
                        }
                        tx.execute("INSERT OR IGNORE INTO message_attachments(message_id,attachment_id) VALUES(?1,?2)", params![mid, aid])?;
                        need_move.insert(aid);
                    }
                    r.messages_added += 1;
                    let msg = load_message(&tx, &mid)?;
                    for u in recipients(&tx, &msg.conv_id, &mid)? {
                        em.emit(&tx, &u, EventData::MessageEdit { message: msg.clone() })?;
                    }
                } else {
                    for a in m["attachments"].as_array().unwrap_or(&empty) {
                        if let Some(aid) = s(a, "id") {
                            need_move.insert(aid);
                        }
                    }
                }
                if let Some(u) = &scope_user {
                    em.emit(&tx, u, EventData::Resync {})?;
                }
            }
            tx.commit()?;
            em.flush(&hub);
            Ok((r, need_move.into_iter().collect::<Vec<_>>()))
        })
        .await?;
    let mut resp = resp;
    resp.attachments_moved_back = move_back(st, &att_ids, &mut resp.warnings).await?;
    Ok(resp)
}

pub async fn restore_route(axum::extract::State(st): axum::extract::State<St>, crate::auth::Admin(_a): crate::auth::Admin, axum::Json(req): axum::Json<RestoreRequest>) -> AppResult<axum::Json<RestoreResponse>> {
    Ok(axum::Json(restore(&st, &req).await?))
}
