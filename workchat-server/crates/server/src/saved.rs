//! Save for later and labels: per-user, private, synced through events.

use crate::db::{new_id, now_ms};
use crate::emit::Emitter;
use crate::error::{AppError, AppResult};
use crate::queries::*;
use proto::*;
use rusqlite::{params, Connection, OptionalExtension};

pub const MAX_LABELS: i64 = 30;
pub const MAX_SAVED: i64 = 2000;

pub fn load_labels(conn: &Connection, user_id: &str) -> AppResult<Vec<Label>> {
    let mut st = conn.prepare("SELECT id,name,color,position FROM labels WHERE user_id=?1 ORDER BY position, rowid")?;
    let v = st
        .query_map([user_id], |r| Ok(Label { id: r.get(0)?, name: r.get(1)?, color: r.get(2)?, position: r.get(3)? }))?
        .collect::<Result<Vec<_>, _>>()?;
    Ok(v)
}

fn label_ids_of(conn: &Connection, user_id: &str, message_id: &str) -> AppResult<Vec<String>> {
    let mut st = conn.prepare(
        "SELECT sl.label_id FROM saved_labels sl JOIN labels l ON l.id=sl.label_id WHERE sl.user_id=?1 AND sl.message_id=?2 ORDER BY l.position, l.rowid",
    )?;
    let v = st.query_map(params![user_id, message_id], |r| r.get::<_, String>(0))?.collect::<Result<Vec<_>, _>>()?;
    Ok(v)
}

pub fn load_item(conn: &Connection, user_id: &str, message_id: &str) -> AppResult<Option<SavedItem>> {
    let row: Option<(String, i64)> =
        conn.query_row("SELECT conv_id,saved_ms FROM saved_items WHERE user_id=?1 AND message_id=?2", params![user_id, message_id], |r| Ok((r.get(0)?, r.get(1)?))).optional()?;
    match row {
        Some((conv_id, saved_ms)) => Ok(Some(SavedItem { message_id: message_id.to_string(), conv_id, saved_ms, label_ids: label_ids_of(conn, user_id, message_id)? })),
        None => Ok(None),
    }
}

/// The user's saved items whose message they can still see (member of the conversation and not hidden), newest first.
pub fn load_saved(conn: &Connection, user_id: &str) -> AppResult<Vec<SavedItem>> {
    let vis = visible_sql("?1");
    let mut st = conn.prepare(&format!(
        "SELECT s.message_id FROM saved_items s JOIN messages m ON m.id=s.message_id JOIN members mem ON mem.conv_id=m.conv_id AND mem.user_id=s.user_id
         WHERE s.user_id=?1 AND {vis} ORDER BY s.saved_ms DESC, s.rowid DESC"
    ))?;
    let ids = st.query_map([user_id], |r| r.get::<_, String>(0))?.collect::<Result<Vec<_>, _>>()?;
    let mut out = vec![];
    for id in ids {
        if let Some(i) = load_item(conn, user_id, &id)? {
            out.push(i);
        }
    }
    Ok(out)
}

pub fn valid_color(c: &str) -> bool {
    c.len() == 7 && c.starts_with('#') && c[1..].chars().all(|x| x.is_ascii_hexdigit())
}

fn check_label_ids(conn: &Connection, user_id: &str, ids: &[String]) -> AppResult<Vec<String>> {
    let mut out: Vec<String> = vec![];
    for id in ids {
        let ok: i64 = conn.query_row("SELECT COUNT(*) FROM labels WHERE id=?1 AND user_id=?2", params![id, user_id], |r| r.get(0))?;
        if ok == 0 {
            return Err(AppError::not_found("label not found"));
        }
        if !out.contains(id) {
            out.push(id.clone());
        }
    }
    Ok(out)
}

fn emit_item(conn: &Connection, em: &mut Emitter, user_id: &str, message_id: &str) -> AppResult<Option<SavedItem>> {
    let item = load_item(conn, user_id, message_id)?;
    if let Some(i) = &item {
        em.emit(conn, user_id, EventData::SavedUpdate { item: i.clone() })?;
    }
    Ok(item)
}

fn emit_labels(conn: &Connection, em: &mut Emitter, user_id: &str) -> AppResult<Vec<Label>> {
    let l = load_labels(conn, user_id)?;
    em.emit(conn, user_id, EventData::LabelsUpdate { labels: l.clone() })?;
    Ok(l)
}

/// Saves a message the user can see; idempotent. `labels` replaces the label set when given.
pub fn save(conn: &mut Connection, hub: &crate::hub::Hub, user_id: &str, message_id: &str, labels: Option<&[String]>) -> AppResult<SavedItem> {
    let conv: String = conn
        .query_row("SELECT conv_id FROM messages WHERE id=?1", [message_id], |r| r.get(0))
        .optional()?
        .ok_or_else(|| AppError::not_found("message not found"))?;
    member_conv(conn, &conv, user_id)?;
    let vis = visible_sql("?2");
    let seen: i64 = conn.query_row(&format!("SELECT COUNT(*) FROM messages m WHERE m.id=?1 AND {vis}"), params![message_id, user_id], |r| r.get(0))?;
    if seen == 0 {
        return Err(AppError::not_found("message not found"));
    }
    let tx = conn.transaction()?;
    let n: i64 = tx.query_row("SELECT COUNT(*) FROM saved_items WHERE user_id=?1", [user_id], |r| r.get(0))?;
    let exists: i64 = tx.query_row("SELECT COUNT(*) FROM saved_items WHERE user_id=?1 AND message_id=?2", params![user_id, message_id], |r| r.get(0))?;
    if exists == 0 && n >= MAX_SAVED {
        return Err(AppError::conflict("too many saved items"));
    }
    let new_labels = match labels {
        Some(l) => Some(check_label_ids(&tx, user_id, l)?),
        None => None,
    };
    tx.execute("INSERT OR IGNORE INTO saved_items(user_id,message_id,conv_id,saved_ms) VALUES(?1,?2,?3,?4)", params![user_id, message_id, conv, now_ms()])?;
    if let Some(l) = new_labels {
        tx.execute("DELETE FROM saved_labels WHERE user_id=?1 AND message_id=?2", params![user_id, message_id])?;
        for id in l {
            tx.execute("INSERT OR IGNORE INTO saved_labels(user_id,message_id,label_id) VALUES(?1,?2,?3)", params![user_id, message_id, id])?;
        }
    }
    let mut em = Emitter::new();
    let item = emit_item(&tx, &mut em, user_id, message_id)?.ok_or_else(|| AppError::internal("saved item vanished"))?;
    tx.commit()?;
    em.flush(hub);
    Ok(item)
}

pub fn unsave(conn: &mut Connection, hub: &crate::hub::Hub, user_id: &str, message_id: &str) -> AppResult<()> {
    let tx = conn.transaction()?;
    tx.execute("DELETE FROM saved_labels WHERE user_id=?1 AND message_id=?2", params![user_id, message_id])?;
    tx.execute("DELETE FROM saved_items WHERE user_id=?1 AND message_id=?2", params![user_id, message_id])?;
    let mut em = Emitter::new();
    em.emit(&tx, user_id, EventData::SavedRemove { message_id: message_id.to_string() })?;
    tx.commit()?;
    em.flush(hub);
    Ok(())
}

pub fn set_item_labels(conn: &mut Connection, hub: &crate::hub::Hub, user_id: &str, message_id: &str, ids: &[String]) -> AppResult<SavedItem> {
    if load_item(conn, user_id, message_id)?.is_none() {
        return Err(AppError::not_found("this message is not saved"));
    }
    let tx = conn.transaction()?;
    let l = check_label_ids(&tx, user_id, ids)?;
    tx.execute("DELETE FROM saved_labels WHERE user_id=?1 AND message_id=?2", params![user_id, message_id])?;
    for id in l {
        tx.execute("INSERT OR IGNORE INTO saved_labels(user_id,message_id,label_id) VALUES(?1,?2,?3)", params![user_id, message_id, id])?;
    }
    let mut em = Emitter::new();
    let item = emit_item(&tx, &mut em, user_id, message_id)?.ok_or_else(|| AppError::internal("saved item vanished"))?;
    tx.commit()?;
    em.flush(hub);
    Ok(item)
}

pub fn create_label(conn: &mut Connection, hub: &crate::hub::Hub, user_id: &str, req: &LabelRequest) -> AppResult<Vec<Label>> {
    let name = req.name.clone().unwrap_or_default();
    let name = name.trim().to_string();
    if name.is_empty() || name.chars().count() > 30 {
        return Err(AppError::validation("label name must be 1-30 characters"));
    }
    let color = req.color.clone().unwrap_or_else(|| "#0dbd8b".into());
    if !valid_color(&color) {
        return Err(AppError::validation("colour must look like #0dbd8b"));
    }
    let dup: i64 = conn.query_row("SELECT COUNT(*) FROM labels WHERE user_id=?1 AND name=?2 COLLATE NOCASE", params![user_id, name], |r| r.get(0))?;
    if dup > 0 {
        return Err(AppError::conflict("you already have a label with that name"));
    }
    let n: i64 = conn.query_row("SELECT COUNT(*) FROM labels WHERE user_id=?1", [user_id], |r| r.get(0))?;
    if n >= MAX_LABELS {
        return Err(AppError::conflict("too many labels"));
    }
    let tx = conn.transaction()?;
    tx.execute("INSERT INTO labels(id,user_id,name,color,position) VALUES(?1,?2,?3,?4,?5)", params![new_id(), user_id, name, color, req.position.unwrap_or(n)])?;
    let mut em = Emitter::new();
    let l = emit_labels(&tx, &mut em, user_id)?;
    tx.commit()?;
    em.flush(hub);
    Ok(l)
}

pub fn update_label(conn: &mut Connection, hub: &crate::hub::Hub, user_id: &str, id: &str, req: &LabelRequest) -> AppResult<Vec<Label>> {
    let ok: i64 = conn.query_row("SELECT COUNT(*) FROM labels WHERE id=?1 AND user_id=?2", params![id, user_id], |r| r.get(0))?;
    if ok == 0 {
        return Err(AppError::not_found("label not found"));
    }
    let tx = conn.transaction()?;
    if let Some(n) = &req.name {
        let n = n.trim();
        if n.is_empty() || n.chars().count() > 30 {
            return Err(AppError::validation("label name must be 1-30 characters"));
        }
        let dup: i64 = tx.query_row("SELECT COUNT(*) FROM labels WHERE user_id=?1 AND name=?2 COLLATE NOCASE AND id!=?3", params![user_id, n, id], |r| r.get(0))?;
        if dup > 0 {
            return Err(AppError::conflict("you already have a label with that name"));
        }
        tx.execute("UPDATE labels SET name=?2 WHERE id=?1", params![id, n])?;
    }
    if let Some(c) = &req.color {
        if !valid_color(c) {
            return Err(AppError::validation("colour must look like #0dbd8b"));
        }
        tx.execute("UPDATE labels SET color=?2 WHERE id=?1", params![id, c])?;
    }
    if let Some(p) = req.position {
        tx.execute("UPDATE labels SET position=?2 WHERE id=?1", params![id, p])?;
    }
    let mut em = Emitter::new();
    let l = emit_labels(&tx, &mut em, user_id)?;
    tx.commit()?;
    em.flush(hub);
    Ok(l)
}

pub fn delete_label(conn: &mut Connection, hub: &crate::hub::Hub, user_id: &str, id: &str) -> AppResult<Vec<Label>> {
    let tx = conn.transaction()?;
    let n = tx.execute("DELETE FROM labels WHERE id=?1 AND user_id=?2", params![id, user_id])?;
    if n == 0 {
        return Err(AppError::not_found("label not found"));
    }
    let affected: Vec<String> = {
        let mut s = tx.prepare("SELECT message_id FROM saved_labels WHERE user_id=?1 AND label_id=?2")?;
        let v = s.query_map(params![user_id, id], |r| r.get::<_, String>(0))?.collect::<Result<Vec<_>, _>>()?;
        v
    };
    tx.execute("DELETE FROM saved_labels WHERE user_id=?1 AND label_id=?2", params![user_id, id])?;
    let mut em = Emitter::new();
    for m in affected {
        emit_item(&tx, &mut em, user_id, &m)?;
    }
    let l = emit_labels(&tx, &mut em, user_id)?;
    tx.commit()?;
    em.flush(hub);
    Ok(l)
}

// ------------------------------------------------------------------ activity feed

pub fn activity_read_ms(conn: &Connection, user_id: &str) -> AppResult<i64> {
    Ok(conn.query_row("SELECT read_ms FROM activity_state WHERE user_id=?1", [user_id], |r| r.get(0)).optional()?.unwrap_or(0))
}

/// Mentions, reactions to your messages and replies in threads you are in, newest first (at most `limit`).
pub fn load_activity(conn: &Connection, user_id: &str, limit: i64) -> AppResult<ActivityFeed> {
    let read = activity_read_ms(conn, user_id)?;
    let vis = visible_sql("?1");
    let like = format!("%\"{user_id}\"%");
    let mut items: Vec<ActivityItem> = vec![];
    // Mentions (also inside threads).
    {
        let mut st = conn.prepare(&format!(
            "SELECT m.id,m.conv_id,m.sender_id,m.ts,m.thread_root FROM messages m JOIN members mem ON mem.conv_id=m.conv_id AND mem.user_id=?1
             WHERE m.sender_id!=?1 AND m.deleted=0 AND m.kind!='system' AND m.mentions LIKE ?2 AND {vis} ORDER BY m.ts DESC LIMIT ?3"
        ))?;
        let rows = st
            .query_map(params![user_id, like, limit], |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?, r.get::<_, String>(2)?, r.get::<_, i64>(3)?, r.get::<_, Option<String>>(4)?)))?
            .collect::<Result<Vec<_>, _>>()?;
        for (id, conv, actor, ts, root) in rows {
            items.push(ActivityItem { id: format!("mention:{id}"), kind: "mention".into(), ts, conv_id: conv, message_id: id, actor_id: actor, emoji: None, thread_root: root.filter(|r| !r.is_empty()), unread: ts > read });
        }
    }
    // Reactions by others to my messages.
    {
        let mut st = conn.prepare(&format!(
            "SELECT r.message_id,m.conv_id,r.user_id,r.emoji,r.ts,m.thread_root FROM reactions r JOIN messages m ON m.id=r.message_id
             JOIN members mem ON mem.conv_id=m.conv_id AND mem.user_id=?1
             WHERE m.sender_id=?1 AND r.user_id!=?1 AND m.deleted=0 AND {vis} ORDER BY r.ts DESC LIMIT ?2"
        ))?;
        let rows = st
            .query_map(params![user_id, limit], |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?, r.get::<_, String>(2)?, r.get::<_, String>(3)?, r.get::<_, i64>(4)?, r.get::<_, Option<String>>(5)?)))?
            .collect::<Result<Vec<_>, _>>()?;
        for (id, conv, actor, emoji, ts, root) in rows {
            items.push(ActivityItem { id: format!("reaction:{id}:{actor}:{emoji}"), kind: "reaction".into(), ts, conv_id: conv, message_id: id, actor_id: actor, emoji: Some(emoji), thread_root: root.filter(|r| !r.is_empty()), unread: ts > read });
        }
    }
    // Replies in threads I follow (and not already listed as a mention).
    {
        let mut st = conn.prepare(&format!(
            "SELECT m.id,m.conv_id,m.sender_id,m.ts,m.thread_root FROM messages m JOIN thread_follows f ON f.thread_root=m.thread_root AND f.user_id=?1 AND f.following=1
             JOIN members mem ON mem.conv_id=m.conv_id AND mem.user_id=?1
             WHERE m.sender_id!=?1 AND m.deleted=0 AND m.kind!='system' AND m.mentions NOT LIKE ?2 AND {vis} ORDER BY m.ts DESC LIMIT ?3"
        ))?;
        let rows = st
            .query_map(params![user_id, like, limit], |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?, r.get::<_, String>(2)?, r.get::<_, i64>(3)?, r.get::<_, Option<String>>(4)?)))?
            .collect::<Result<Vec<_>, _>>()?;
        for (id, conv, actor, ts, root) in rows {
            items.push(ActivityItem { id: format!("thread_reply:{id}"), kind: "thread_reply".into(), ts, conv_id: conv, message_id: id, actor_id: actor, emoji: None, thread_root: root, unread: ts > read });
        }
    }
    items.sort_by(|a, b| b.ts.cmp(&a.ts).then_with(|| a.id.cmp(&b.id)));
    items.truncate(limit.max(1) as usize);
    let unread = items.iter().filter(|i| i.unread).count() as i64;
    Ok(ActivityFeed { items, read_ms: read, unread })
}

pub fn mark_activity_read(conn: &mut Connection, hub: &crate::hub::Hub, user_id: &str, up_to: Option<i64>) -> AppResult<i64> {
    let ms = up_to.unwrap_or_else(now_ms).min(now_ms() + 60_000);
    let cur = activity_read_ms(conn, user_id)?;
    let ms = ms.max(cur);
    let tx = conn.transaction()?;
    tx.execute("INSERT INTO activity_state(user_id,read_ms) VALUES(?1,?2) ON CONFLICT(user_id) DO UPDATE SET read_ms=excluded.read_ms", params![user_id, ms])?;
    let mut em = Emitter::new();
    em.emit(&tx, user_id, EventData::ActivityRead { read_ms: ms })?;
    tx.commit()?;
    em.flush(hub);
    Ok(ms)
}
