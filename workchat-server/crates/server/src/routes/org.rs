//! Organisation endpoints: threads, pins, bookmarks, user groups, preferences, sections,
//! drafts, forwarding and the Files browser.

use crate::auth::{Admin, Auth};
use crate::db::{new_id, now_ms};
use crate::emit::Emitter;
use crate::error::{AppError, AppResult};
use crate::org::*;
use crate::queries::*;
use crate::state::St;
use axum::extract::{Path, Query, State};
use axum::Json;
use proto::*;
use rusqlite::{params, OptionalExtension};
use serde::Deserialize;

type J<T> = AppResult<Json<T>>;

// ------------------------------------------------------------------ threads (M-05..M-07)

#[derive(Deserialize)]
pub struct ThreadsParams {
    /// Only threads of this conversation.
    conv: Option<String>,
    /// `true`: only threads with unread replies.
    unread: Option<bool>,
    /// Paging: only threads whose last activity (newest reply or the root) is older than this time (UTC ms).
    before: Option<i64>,
    limit: Option<usize>,
}

/// Threads the caller takes part in (follows), newest activity first; each has the root, reply count, the newest
/// reply, participants and the caller's unread count. Without parameters it returns up to 200.
pub async fn list_threads(State(st): State<St>, auth: Auth, Query(p): Query<ThreadsParams>) -> J<Vec<ThreadSummary>> {
    Ok(Json(
        st.db
            .call(move |c| {
                let mut v = user_threads(c, &auth.user_id)?;
                if let Some(cv) = &p.conv {
                    v.retain(|t| &t.conv_id == cv);
                }
                if p.unread == Some(true) {
                    v.retain(|t| t.unread > 0);
                }
                if let Some(b) = p.before {
                    v.retain(|t| t.last_reply_ts.max(t.root.ts) < b);
                }
                v.truncate(p.limit.unwrap_or(200).clamp(1, 200));
                Ok(v)
            })
            .await?,
    ))
}

/// The numbers behind the filter pills: conversations, unread, threads, files.
pub async fn my_counts(State(st): State<St>, auth: Auth) -> J<Counts> {
    Ok(Json(st.db.call(move |c| compute_counts(c, &auth.user_id)).await?))
}

pub fn compute_counts(c: &rusqlite::Connection, user_id: &str) -> AppResult<Counts> {
    let mut n = Counts::default();
    for id in user_conversation_ids(c, user_id)? {
        let s = conv_summary(c, &id, user_id)?;
        let cv = &s.conversation;
        if cv.archived || cv.kind == ConversationKind::SelfNote {
            continue;
        }
        let is_unread = s.unread > 0 || s.marked_unread;
        n.conversations += 1;
        n.unread_messages += s.unread;
        n.mentions += s.mentions;
        n.unread_conversations += is_unread as i64;
        if cv.kind == ConversationKind::Channel {
            n.channels += 1;
            n.unread_channels += is_unread as i64;
        } else {
            n.dms += 1;
            n.unread_dms += is_unread as i64;
        }
    }
    for t in user_threads(c, user_id)? {
        n.threads += 1;
        n.threads_unread += (t.unread > 0) as i64;
    }
    let vis = visible_sql("?1");
    let (all, img, vid, aud, doc): (i64, i64, i64, i64, i64) = c.query_row(
        &format!(
            "SELECT COUNT(*),
                    COALESCE(SUM(a.content_type LIKE 'image/%'),0),
                    COALESCE(SUM(a.content_type LIKE 'video/%'),0),
                    COALESCE(SUM(a.content_type LIKE 'audio/%'),0),
                    COALESCE(SUM(a.content_type NOT LIKE 'image/%' AND a.content_type NOT LIKE 'video/%' AND a.content_type NOT LIKE 'audio/%'),0)
             FROM message_attachments ma JOIN attachments a ON a.id=ma.attachment_id JOIN messages m ON m.id=ma.message_id
             JOIN members mem ON mem.conv_id=m.conv_id AND mem.user_id=?1
             WHERE m.deleted=0 AND a.purpose='file' AND a.removed=0 AND {vis}"
        ),
        [user_id],
        |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?)),
    )?;
    (n.files, n.files_image, n.files_video, n.files_audio, n.files_document) = (all, img, vid, aud, doc);
    n.saved = c.query_row("SELECT COUNT(*) FROM saved_items WHERE user_id=?1", [user_id], |r| r.get(0))?;
    Ok(n)
}

pub async fn get_thread(State(st): State<St>, auth: Auth, Path((conv, root)): Path<(String, String)>) -> J<ThreadView> {
    Ok(Json(
        st.db
            .call(move |c| {
                member_conv(c, &conv, &auth.user_id)?;
                let root_id = resolve_thread_root(c, &conv, &root)?;
                let summary = thread_summary(c, &root_id, &auth.user_id)?;
                let vis = visible_sql("?2");
                let mut stmt = c.prepare(&format!("SELECT {MSG_COLS} FROM messages m WHERE m.thread_root=?1 AND {vis} ORDER BY m.seq"))?;
                let rows = stmt.query_map(params![root_id, auth.user_id], msg_base)?.collect::<Result<Vec<_>, _>>()?;
                Ok(ThreadView { summary, replies: hydrate(c, rows)? })
            })
            .await?,
    ))
}

async fn follow(st: St, auth: Auth, root: String, following: bool) -> J<ThreadSummary> {
    let hub = st.hub.clone();
    Ok(Json(
        st.db
            .call(move |c| {
                let (conv, thread_root): (String, Option<String>) = c
                    .query_row("SELECT conv_id, thread_root FROM messages WHERE id=?1", [&root], |r| Ok((r.get(0)?, r.get(1)?)))
                    .optional()?
                    .ok_or_else(|| AppError::not_found("message not found"))?;
                member_conv(c, &conv, &auth.user_id)?;
                let root_id = thread_root.unwrap_or(root);
                let tx = c.transaction()?;
                set_follow(&tx, &auth.user_id, &root_id, &conv, following, false)?;
                let mut em = Emitter::new();
                emit_thread(&tx, &mut em, &auth.user_id, &root_id)?;
                let t = thread_summary(&tx, &root_id, &auth.user_id)?;
                tx.commit()?;
                em.flush(&hub);
                Ok(t)
            })
            .await?,
    ))
}

pub async fn follow_thread(State(st): State<St>, auth: Auth, Path(root): Path<String>) -> J<ThreadSummary> {
    follow(st, auth, root, true).await
}

pub async fn unfollow_thread(State(st): State<St>, auth: Auth, Path(root): Path<String>) -> J<ThreadSummary> {
    follow(st, auth, root, false).await
}

pub async fn read_thread(State(st): State<St>, auth: Auth, Path(root): Path<String>) -> J<ThreadSummary> {
    let hub = st.hub.clone();
    Ok(Json(
        st.db
            .call(move |c| {
                let conv: String = c
                    .query_row("SELECT conv_id FROM messages WHERE id=?1", [&root], |r| r.get(0))
                    .optional()?
                    .ok_or_else(|| AppError::not_found("message not found"))?;
                member_conv(c, &conv, &auth.user_id)?;
                let tx = c.transaction()?;
                let last: i64 = tx.query_row("SELECT COALESCE(MAX(seq),0) FROM messages WHERE thread_root=?1", [&root], |r| r.get(0))?;
                tx.execute(
                    "INSERT INTO thread_follows(user_id,thread_root,conv_id,following,last_read_seq) VALUES(?1,?2,?3,0,?4)
                     ON CONFLICT(user_id,thread_root) DO UPDATE SET last_read_seq=MAX(last_read_seq, excluded.last_read_seq)",
                    params![auth.user_id, root, conv, last],
                )?;
                let mut em = Emitter::new();
                emit_thread(&tx, &mut em, &auth.user_id, &root)?;
                let t = thread_summary(&tx, &root, &auth.user_id)?;
                tx.commit()?;
                em.flush(&hub);
                Ok(t)
            })
            .await?,
    ))
}

// ------------------------------------------------------------------ pins (C-09)

pub async fn list_pins(State(st): State<St>, auth: Auth, Path(conv): Path<String>) -> J<Vec<Pin>> {
    Ok(Json(
        st.db
            .call(move |c| {
                member_conv(c, &conv, &auth.user_id)?;
                load_pins(c, &conv)
            })
            .await?,
    ))
}

pub async fn pin_message(State(st): State<St>, auth: Auth, Path((conv, mid)): Path<(String, String)>) -> J<Vec<Pin>> {
    pin_op(st, auth, conv, mid, true).await
}

pub async fn unpin_message(State(st): State<St>, auth: Auth, Path((conv, mid)): Path<(String, String)>) -> J<Vec<Pin>> {
    pin_op(st, auth, conv, mid, false).await
}

async fn pin_op(st: St, auth: Auth, conv: String, mid: String, add: bool) -> J<Vec<Pin>> {
    let hub = st.hub.clone();
    Ok(Json(
        st.db
            .call(move |c| {
                if !can_pin(c, auth.is_admin(), &conv, &auth.user_id)? {
                    return Err(AppError::forbidden("only channel managers and admins can pin in this channel"));
                }
                let tx = c.transaction()?;
                if add {
                    let ok: i64 = tx.query_row("SELECT COUNT(*) FROM messages WHERE id=?1 AND conv_id=?2 AND deleted=0", params![mid, conv], |r| r.get(0))?;
                    if ok == 0 {
                        return Err(AppError::not_found("message not found"));
                    }
                    let n: i64 = tx.query_row("SELECT COUNT(*) FROM pins WHERE conv_id=?1", [&conv], |r| r.get(0))?;
                    let already: i64 = tx.query_row("SELECT COUNT(*) FROM pins WHERE conv_id=?1 AND message_id=?2", params![conv, mid], |r| r.get(0))?;
                    if already == 0 && n >= MAX_PINS {
                        return Err(AppError::conflict(format!("a conversation can have at most {MAX_PINS} pins")));
                    }
                    tx.execute("INSERT OR IGNORE INTO pins(conv_id,message_id,pinned_by,ts) VALUES(?1,?2,?3,?4)", params![conv, mid, auth.user_id, now_ms()])?;
                } else {
                    tx.execute("DELETE FROM pins WHERE conv_id=?1 AND message_id=?2", params![conv, mid])?;
                }
                let mut em = Emitter::new();
                emit_pins(&tx, &mut em, &conv)?;
                let pins = load_pins(&tx, &conv)?;
                tx.commit()?;
                em.flush(&hub);
                Ok(pins)
            })
            .await?,
    ))
}

// ------------------------------------------------------------------ bookmarks (C-07)

fn check_bookmark(req: &BookmarkRequest) -> AppResult<()> {
    let t = req.title.trim();
    if t.is_empty() || t.chars().count() > 120 {
        return Err(AppError::validation("bookmark title must be 1-120 characters"));
    }
    match (&req.url, &req.attachment_id) {
        (Some(u), None) => {
            let parsed = url::Url::parse(u).map_err(|_| AppError::validation("bookmark link is not a valid URL"))?;
            if !matches!(parsed.scheme(), "http" | "https") {
                return Err(AppError::validation("bookmark links must start with http:// or https://"));
            }
            Ok(())
        }
        (None, Some(_)) => Ok(()),
        _ => Err(AppError::validation("a bookmark is either a link or a file")),
    }
}

pub async fn list_bookmarks(State(st): State<St>, auth: Auth, Path(conv): Path<String>) -> J<Vec<Bookmark>> {
    Ok(Json(
        st.db
            .call(move |c| {
                if auth.is_admin() {
                    conv_row(c, &conv)?;
                } else {
                    member_conv(c, &conv, &auth.user_id)?;
                }
                load_bookmarks(c, &conv)
            })
            .await?,
    ))
}

pub async fn add_bookmark(State(st): State<St>, auth: Auth, Path(conv): Path<String>, Json(req): Json<BookmarkRequest>) -> J<Vec<Bookmark>> {
    check_bookmark(&req)?;
    let hub = st.hub.clone();
    Ok(Json(
        st.db
            .call(move |c| {
                if !can_curate(c, auth.is_admin(), &conv, &auth.user_id)? {
                    return Err(AppError::forbidden("only owners and admins can edit bookmarks"));
                }
                let tx = c.transaction()?;
                let n: i64 = tx.query_row("SELECT COUNT(*) FROM bookmarks WHERE conv_id=?1", [&conv], |r| r.get(0))?;
                if n >= 50 {
                    return Err(AppError::conflict("a conversation can have at most 50 bookmarks"));
                }
                if let Some(a) = &req.attachment_id {
                    let ok: i64 = tx.query_row(
                        "SELECT COUNT(*) FROM message_attachments ma JOIN messages m ON m.id=ma.message_id WHERE ma.attachment_id=?1 AND m.conv_id=?2",
                        params![a, conv],
                        |r| r.get(0),
                    )?;
                    if ok == 0 {
                        return Err(AppError::validation("that file is not in this conversation"));
                    }
                }
                tx.execute(
                    "INSERT INTO bookmarks(id,conv_id,title,url,attachment_id,position,created_by,created_ms) VALUES(?1,?2,?3,?4,?5,?6,?7,?8)",
                    params![new_id(), conv, req.title.trim(), req.url, req.attachment_id, req.position.unwrap_or(n), auth.user_id, now_ms()],
                )?;
                let mut em = Emitter::new();
                emit_bookmarks(&tx, &mut em, &conv)?;
                let b = load_bookmarks(&tx, &conv)?;
                tx.commit()?;
                em.flush(&hub);
                Ok(b)
            })
            .await?,
    ))
}

pub async fn update_bookmark(State(st): State<St>, auth: Auth, Path(id): Path<String>, Json(req): Json<BookmarkRequest>) -> J<Vec<Bookmark>> {
    check_bookmark(&req)?;
    let hub = st.hub.clone();
    Ok(Json(
        st.db
            .call(move |c| {
                let conv: String = c
                    .query_row("SELECT conv_id FROM bookmarks WHERE id=?1", [&id], |r| r.get(0))
                    .optional()?
                    .ok_or_else(|| AppError::not_found("bookmark not found"))?;
                if !can_curate(c, auth.is_admin(), &conv, &auth.user_id)? {
                    return Err(AppError::forbidden("only owners and admins can edit bookmarks"));
                }
                let tx = c.transaction()?;
                tx.execute(
                    "UPDATE bookmarks SET title=?2, url=?3, attachment_id=?4, position=COALESCE(?5, position) WHERE id=?1",
                    params![id, req.title.trim(), req.url, req.attachment_id, req.position],
                )?;
                let mut em = Emitter::new();
                emit_bookmarks(&tx, &mut em, &conv)?;
                let b = load_bookmarks(&tx, &conv)?;
                tx.commit()?;
                em.flush(&hub);
                Ok(b)
            })
            .await?,
    ))
}

pub async fn delete_bookmark(State(st): State<St>, auth: Auth, Path(id): Path<String>) -> J<Vec<Bookmark>> {
    let hub = st.hub.clone();
    Ok(Json(
        st.db
            .call(move |c| {
                let conv: String = c
                    .query_row("SELECT conv_id FROM bookmarks WHERE id=?1", [&id], |r| r.get(0))
                    .optional()?
                    .ok_or_else(|| AppError::not_found("bookmark not found"))?;
                if !can_curate(c, auth.is_admin(), &conv, &auth.user_id)? {
                    return Err(AppError::forbidden("only owners and admins can edit bookmarks"));
                }
                let tx = c.transaction()?;
                tx.execute("DELETE FROM bookmarks WHERE id=?1", [&id])?;
                let mut em = Emitter::new();
                emit_bookmarks(&tx, &mut em, &conv)?;
                let b = load_bookmarks(&tx, &conv)?;
                tx.commit()?;
                em.flush(&hub);
                Ok(b)
            })
            .await?,
    ))
}

// ------------------------------------------------------------------ user groups (M-18)

pub async fn list_groups(State(st): State<St>, _auth: Auth) -> J<Vec<UserGroup>> {
    Ok(Json(st.db.call(|c| load_groups(c)).await?))
}

fn set_group_members(tx: &rusqlite::Connection, gid: &str, ids: &[String]) -> AppResult<()> {
    tx.execute("DELETE FROM user_group_members WHERE group_id=?1", [gid])?;
    for u in ids {
        let row = get_user(tx, u)?;
        if row.deleted {
            continue;
        }
        tx.execute("INSERT OR IGNORE INTO user_group_members(group_id,user_id) VALUES(?1,?2)", params![gid, u])?;
    }
    Ok(())
}

pub async fn create_group(State(st): State<St>, Admin(_a): Admin, Json(req): Json<UserGroupRequest>) -> J<UserGroup> {
    let hub = st.hub.clone();
    Ok(Json(
        st.db
            .call(move |c| {
                let handle = req.handle.clone().unwrap_or_default().trim().trim_start_matches('@').to_string();
                if !valid_handle(&handle) {
                    return Err(AppError::validation("group handle must be 2-32 letters, digits, - or _ (and not channel/here/everyone)"));
                }
                let taken: i64 = c.query_row("SELECT COUNT(*) FROM user_groups WHERE handle=?1 COLLATE NOCASE", [&handle], |r| r.get(0))?;
                let clash: i64 = c.query_row("SELECT COUNT(*) FROM users WHERE username=?1 COLLATE NOCASE", [&handle], |r| r.get(0))?;
                if taken > 0 || clash > 0 {
                    return Err(AppError::conflict("that handle is already used by a group or a person"));
                }
                let tx = c.transaction()?;
                let id = new_id();
                let name = req.name.clone().filter(|n| !n.trim().is_empty()).unwrap_or_else(|| handle.clone());
                tx.execute("INSERT INTO user_groups(id,handle,name,created_ms) VALUES(?1,?2,?3,?4)", params![id, handle, name.trim(), now_ms()])?;
                set_group_members(&tx, &id, &req.member_ids.clone().unwrap_or_default())?;
                let mut em = Emitter::new();
                emit_groups(&tx, &mut em)?;
                let g = load_groups(&tx)?.into_iter().find(|g| g.id == id).ok_or_else(|| AppError::internal("group vanished"))?;
                tx.commit()?;
                em.flush(&hub);
                Ok(g)
            })
            .await?,
    ))
}

pub async fn update_group(State(st): State<St>, Admin(_a): Admin, Path(id): Path<String>, Json(req): Json<UserGroupRequest>) -> J<UserGroup> {
    let hub = st.hub.clone();
    Ok(Json(
        st.db
            .call(move |c| {
                let exists: i64 = c.query_row("SELECT COUNT(*) FROM user_groups WHERE id=?1", [&id], |r| r.get(0))?;
                if exists == 0 {
                    return Err(AppError::not_found("group not found"));
                }
                let tx = c.transaction()?;
                if let Some(h) = &req.handle {
                    let h = h.trim().trim_start_matches('@');
                    if !valid_handle(h) {
                        return Err(AppError::validation("invalid group handle"));
                    }
                    let taken: i64 = tx.query_row("SELECT COUNT(*) FROM user_groups WHERE handle=?1 COLLATE NOCASE AND id!=?2", params![h, id], |r| r.get(0))?;
                    if taken > 0 {
                        return Err(AppError::conflict("that handle is already used"));
                    }
                    tx.execute("UPDATE user_groups SET handle=?2 WHERE id=?1", params![id, h])?;
                }
                if let Some(n) = &req.name {
                    tx.execute("UPDATE user_groups SET name=?2 WHERE id=?1", params![id, n.trim()])?;
                }
                if let Some(m) = &req.member_ids {
                    set_group_members(&tx, &id, m)?;
                }
                let mut em = Emitter::new();
                emit_groups(&tx, &mut em)?;
                let g = load_groups(&tx)?.into_iter().find(|g| g.id == id).ok_or_else(|| AppError::internal("group vanished"))?;
                tx.commit()?;
                em.flush(&hub);
                Ok(g)
            })
            .await?,
    ))
}

pub async fn delete_group(State(st): State<St>, Admin(_a): Admin, Path(id): Path<String>) -> J<serde_json::Value> {
    let hub = st.hub.clone();
    st.db
        .call(move |c| {
            let tx = c.transaction()?;
            let n = tx.execute("DELETE FROM user_groups WHERE id=?1", [&id])?;
            if n == 0 {
                return Err(AppError::not_found("group not found"));
            }
            tx.execute("DELETE FROM user_group_members WHERE group_id=?1", [&id])?;
            let mut em = Emitter::new();
            emit_groups(&tx, &mut em)?;
            tx.commit()?;
            em.flush(&hub);
            Ok(())
        })
        .await?;
    Ok(Json(serde_json::json!({"ok": true})))
}

// ------------------------------------------------------------------ preferences (NV-01..NV-07, N-03)

pub async fn list_prefs(State(st): State<St>, auth: Auth) -> J<Vec<ConvPref>> {
    Ok(Json(st.db.call(move |c| load_prefs(c, &auth.user_id)).await?))
}

pub async fn set_pref(State(st): State<St>, auth: Auth, Path(conv): Path<String>, Json(req): Json<ConvPrefRequest>) -> J<ConvPref> {
    let hub = st.hub.clone();
    Ok(Json(
        st.db
            .call(move |c| {
                member_conv(c, &conv, &auth.user_id)?;
                let mut p = pref_for(c, &auth.user_id, &conv)?.unwrap_or_else(|| ConvPref::default_for(&conv));
                if let Some(f) = req.favourite {
                    p.favourite = f;
                }
                if let Some(s) = &req.section_id {
                    if s.is_empty() {
                        p.section_id = None;
                    } else {
                        let ok: i64 = c.query_row("SELECT COUNT(*) FROM sections WHERE id=?1 AND user_id=?2", params![s, auth.user_id], |r| r.get(0))?;
                        if ok == 0 {
                            return Err(AppError::validation("unknown section"));
                        }
                        p.section_id = Some(s.clone());
                    }
                }
                if let Some(m) = req.mute_until_ms {
                    p.mute_until_ms = if m == 0 { None } else { Some(m) };
                }
                if let Some(n) = &req.notify {
                    if !matches!(n.as_str(), "all" | "mentions" | "nothing") {
                        return Err(AppError::validation("notify must be all, mentions or nothing"));
                    }
                    p.notify = n.clone();
                }
                let tx = c.transaction()?;
                tx.execute(
                    "INSERT INTO conv_prefs(user_id,conv_id,favourite,section_id,mute_until_ms,notify,position) VALUES(?1,?2,?3,?4,?5,?6,?7)
                     ON CONFLICT(user_id,conv_id) DO UPDATE SET favourite=excluded.favourite, section_id=excluded.section_id, mute_until_ms=excluded.mute_until_ms, notify=excluded.notify, position=excluded.position",
                    params![auth.user_id, conv, p.favourite as i64, p.section_id, p.mute_until_ms, p.notify, p.position],
                )?;
                let mut em = Emitter::new();
                em.emit(&tx, &auth.user_id, EventData::PrefUpdate { pref: p.clone() })?;
                tx.commit()?;
                em.flush(&hub);
                Ok(p)
            })
            .await?,
    ))
}

/// NV-04: stores the user's own order of some conversations (positions 0..n) and tells their other devices.
pub async fn order_conversations(State(st): State<St>, auth: Auth, Json(req): Json<ConvOrderRequest>) -> J<Vec<ConvPref>> {
    let hub = st.hub.clone();
    Ok(Json(
        st.db
            .call(move |c| {
                if req.conv_ids.len() > 2000 {
                    return Err(AppError::validation("too many conversations"));
                }
                let tx = c.transaction()?;
                let mut em = Emitter::new();
                let mut out = vec![];
                if req.conv_ids.is_empty() {
                    // Reset: every manual order of this user goes away.
                    let ids: Vec<String> = {
                        let mut q = tx.prepare("SELECT conv_id FROM conv_prefs WHERE user_id=?1 AND position IS NOT NULL")?;
                        let rows = q.query_map([&auth.user_id], |r| r.get::<_, String>(0))?.collect::<Result<Vec<_>, _>>()?;
                        rows
                    };
                    tx.execute("UPDATE conv_prefs SET position=NULL WHERE user_id=?1", [&auth.user_id])?;
                    for id in ids {
                        if let Some(p) = pref_for(&tx, &auth.user_id, &id)? {
                            em.emit(&tx, &auth.user_id, EventData::PrefUpdate { pref: p.clone() })?;
                            out.push(p);
                        }
                    }
                } else {
                    for (i, id) in req.conv_ids.iter().enumerate() {
                        member_conv(&tx, id, &auth.user_id)?;
                        tx.execute(
                            "INSERT INTO conv_prefs(user_id,conv_id,position) VALUES(?1,?2,?3) ON CONFLICT(user_id,conv_id) DO UPDATE SET position=excluded.position",
                            params![auth.user_id, id, i as i64],
                        )?;
                        if let Some(p) = pref_for(&tx, &auth.user_id, id)? {
                            em.emit(&tx, &auth.user_id, EventData::PrefUpdate { pref: p.clone() })?;
                            out.push(p);
                        }
                    }
                }
                tx.commit()?;
                em.flush(&hub);
                Ok(out)
            })
            .await?,
    ))
}

/// NV-04: the order of the sidebar sections, custom ones and the built-in Favourites / Channels / Direct messages.
pub async fn order_sections(State(st): State<St>, auth: Auth, Json(req): Json<SectionOrderRequest>) -> J<Vec<Section>> {
    let hub = st.hub.clone();
    Ok(Json(
        st.db
            .call(move |c| {
                let tx = c.transaction()?;
                if req.ids.is_empty() {
                    tx.execute("DELETE FROM sections WHERE user_id=?1 AND id LIKE 'builtin:%'", [&auth.user_id])?;
                    // Custom sections keep their relative order, renumbered from 0.
                    let ids: Vec<String> = {
                        let mut q = tx.prepare("SELECT id FROM sections WHERE user_id=?1 ORDER BY position, rowid")?;
                        let rows = q.query_map([&auth.user_id], |r| r.get::<_, String>(0))?.collect::<Result<Vec<_>, _>>()?;
                        rows
                    };
                    for (i, id) in ids.iter().enumerate() {
                        tx.execute("UPDATE sections SET position=?2 WHERE id=?1", params![id, i as i64])?;
                    }
                } else {
                    for (i, key) in req.ids.iter().enumerate() {
                        if let Some(b) = key.strip_prefix("builtin:") {
                            if !matches!(b, "fav" | "channels" | "dms") {
                                return Err(AppError::validation("unknown built-in section"));
                            }
                            let id = format!("builtin:{b}:{}", auth.user_id);
                            tx.execute(
                                "INSERT INTO sections(id,user_id,name,position,collapsed) VALUES(?1,?2,'',?3,0) ON CONFLICT(id) DO UPDATE SET position=excluded.position",
                                params![id, auth.user_id, i as i64],
                            )?;
                        } else {
                            let n = tx.execute("UPDATE sections SET position=?3 WHERE id=?1 AND user_id=?2", params![key, auth.user_id, i as i64])?;
                            if n == 0 {
                                return Err(AppError::not_found("section not found"));
                            }
                        }
                    }
                }
                let mut em = Emitter::new();
                let s = emit_sections(&tx, &mut em, &auth.user_id)?;
                tx.commit()?;
                em.flush(&hub);
                Ok(s)
            })
            .await?,
    ))
}

pub async fn list_sections(State(st): State<St>, auth: Auth) -> J<Vec<Section>> {
    Ok(Json(st.db.call(move |c| load_sections(c, &auth.user_id)).await?))
}

fn emit_sections(conn: &rusqlite::Connection, em: &mut Emitter, user_id: &str) -> AppResult<Vec<Section>> {
    let s = load_sections(conn, user_id)?;
    em.emit(conn, user_id, EventData::SectionsUpdate { sections: s.clone() })?;
    Ok(s)
}

pub async fn create_section(State(st): State<St>, auth: Auth, Json(req): Json<SectionRequest>) -> J<Vec<Section>> {
    let hub = st.hub.clone();
    Ok(Json(
        st.db
            .call(move |c| {
                let name = req.name.clone().unwrap_or_default();
                let name = name.trim();
                if name.is_empty() || name.chars().count() > 40 {
                    return Err(AppError::validation("section name must be 1-40 characters"));
                }
                let n: i64 = c.query_row("SELECT COUNT(*) FROM sections WHERE user_id=?1 AND id NOT LIKE 'builtin:%'", [&auth.user_id], |r| r.get(0))?;
                if n >= 30 {
                    return Err(AppError::conflict("too many sections"));
                }
                let tx = c.transaction()?;
                tx.execute(
                    "INSERT INTO sections(id,user_id,name,position,collapsed) VALUES(?1,?2,?3,?4,0)",
                    params![new_id(), auth.user_id, name, req.position.unwrap_or(n)],
                )?;
                let mut em = Emitter::new();
                let s = emit_sections(&tx, &mut em, &auth.user_id)?;
                tx.commit()?;
                em.flush(&hub);
                Ok(s)
            })
            .await?,
    ))
}

pub async fn update_section(State(st): State<St>, auth: Auth, Path(id): Path<String>, Json(req): Json<SectionRequest>) -> J<Vec<Section>> {
    let hub = st.hub.clone();
    Ok(Json(
        st.db
            .call(move |c| {
                let ok: i64 = c.query_row("SELECT COUNT(*) FROM sections WHERE id=?1 AND user_id=?2", params![id, auth.user_id], |r| r.get(0))?;
                if ok == 0 {
                    return Err(AppError::not_found("section not found"));
                }
                let tx = c.transaction()?;
                if let Some(n) = &req.name {
                    let n = n.trim();
                    if n.is_empty() || n.chars().count() > 40 {
                        return Err(AppError::validation("section name must be 1-40 characters"));
                    }
                    tx.execute("UPDATE sections SET name=?2 WHERE id=?1", params![id, n])?;
                }
                if let Some(p) = req.position {
                    tx.execute("UPDATE sections SET position=?2 WHERE id=?1", params![id, p])?;
                }
                if let Some(cl) = req.collapsed {
                    tx.execute("UPDATE sections SET collapsed=?2 WHERE id=?1", params![id, cl as i64])?;
                }
                let mut em = Emitter::new();
                let s = emit_sections(&tx, &mut em, &auth.user_id)?;
                tx.commit()?;
                em.flush(&hub);
                Ok(s)
            })
            .await?,
    ))
}

pub async fn delete_section(State(st): State<St>, auth: Auth, Path(id): Path<String>) -> J<Vec<Section>> {
    let hub = st.hub.clone();
    Ok(Json(
        st.db
            .call(move |c| {
                let tx = c.transaction()?;
                let n = tx.execute("DELETE FROM sections WHERE id=?1 AND user_id=?2", params![id, auth.user_id])?;
                if n == 0 {
                    return Err(AppError::not_found("section not found"));
                }
                // Conversations in the deleted section fall back to the main list.
                let affected: Vec<String> = {
                    let mut s = tx.prepare("SELECT conv_id FROM conv_prefs WHERE user_id=?1 AND section_id=?2")?;
                    let v = s.query_map(params![auth.user_id, id], |r| r.get::<_, String>(0))?.collect::<Result<Vec<_>, _>>()?;
                    v
                };
                tx.execute("UPDATE conv_prefs SET section_id=NULL WHERE user_id=?1 AND section_id=?2", params![auth.user_id, id])?;
                let mut em = Emitter::new();
                for cid in affected {
                    if let Some(p) = pref_for(&tx, &auth.user_id, &cid)? {
                        em.emit(&tx, &auth.user_id, EventData::PrefUpdate { pref: p })?;
                    }
                }
                let s = emit_sections(&tx, &mut em, &auth.user_id)?;
                tx.commit()?;
                em.flush(&hub);
                Ok(s)
            })
            .await?,
    ))
}

// ------------------------------------------------------------------ drafts (M-22)

pub async fn list_drafts(State(st): State<St>, auth: Auth) -> J<Vec<DraftItem>> {
    Ok(Json(st.db.call(move |c| load_drafts(c, &auth.user_id)).await?))
}

pub async fn put_draft(State(st): State<St>, auth: Auth, Path(conv): Path<String>, Json(req): Json<DraftRequest>) -> J<DraftItem> {
    if req.body.chars().count() > 20_000 {
        return Err(AppError::validation("draft too long"));
    }
    let hub = st.hub.clone();
    Ok(Json(
        st.db
            .call(move |c| {
                member_conv(c, &conv, &auth.user_id)?;
                let tr = req.thread_root.clone().unwrap_or_default();
                let now = now_ms();
                let tx = c.transaction()?;
                if req.body.is_empty() {
                    tx.execute("DELETE FROM drafts WHERE user_id=?1 AND conv_id=?2 AND thread_root=?3", params![auth.user_id, conv, tr])?;
                } else {
                    tx.execute(
                        "INSERT INTO drafts(user_id,conv_id,thread_root,body,updated_ms) VALUES(?1,?2,?3,?4,?5)
                         ON CONFLICT(user_id,conv_id,thread_root) DO UPDATE SET body=excluded.body, updated_ms=excluded.updated_ms",
                        params![auth.user_id, conv, tr, req.body, now],
                    )?;
                }
                let d = DraftItem { conv_id: conv, thread_root: req.thread_root.clone(), body: req.body.clone(), updated_ms: now };
                let mut em = Emitter::new();
                em.emit(&tx, &auth.user_id, EventData::DraftUpdate { draft: d.clone() })?;
                tx.commit()?;
                em.flush(&hub);
                Ok(d)
            })
            .await?,
    ))
}

// ------------------------------------------------------------------ forward (M-20, F-04)

pub async fn forward_message(State(st): State<St>, auth: Auth, Path(id): Path<String>, Json(req): Json<ForwardRequest>) -> J<Vec<Message>> {
    if req.conv_ids.is_empty() || req.conv_ids.len() > 10 {
        return Err(AppError::validation("choose between 1 and 10 conversations"));
    }
    let hub = st.hub.clone();
    Ok(Json(
        st.db
            .call(move |c| {
                let src = load_message(c, &id)?;
                member_conv(c, &src.conv_id, &auth.user_id)?;
                let hidden: i64 = c.query_row(
                    "SELECT COUNT(*) FROM message_hidden WHERE user_id=?1 AND message_id=?2",
                    params![auth.user_id, id],
                    |r| r.get(0),
                )?;
                if hidden > 0 || src.deleted {
                    return Err(AppError::not_found("message not found"));
                }
                // Forwarding a forward keeps the original attribution.
                let fwd = src.forwarded.clone().unwrap_or(ForwardInfo { sender_id: src.sender_id.clone(), conv_id: src.conv_id.clone(), message_id: src.id.clone(), ts: src.ts });
                let tx = c.transaction()?;
                let mut em = Emitter::new();
                let mut out = vec![];
                for target in &req.conv_ids {
                    let cr = member_conv(&tx, target, &auth.user_id)?;
                    if cr.archived {
                        return Err(AppError::forbidden("a chosen conversation is archived"));
                    }
                    let members = member_ids(&tx, target)?;
                    let mut new_ids = vec![];
                    if let Some(note) = req.comment.as_ref().map(|s| s.trim()).filter(|s| !s.is_empty()) {
                        new_ids.push(insert_forward_row(&tx, target, &auth.user_id, note, None, &[])?);
                    }
                    let att_ids: Vec<String> = src.attachments.iter().filter(|a| !a.removed).map(|a| a.id.clone()).collect();
                    new_ids.push(insert_forward_row(&tx, target, &auth.user_id, &src.body, Some(&fwd), &att_ids)?);
                    for nid in &new_ids {
                        let m = load_message(&tx, nid)?;
                        for u in &members {
                            em.emit(&tx, u, EventData::MessageNew { message: m.clone() })?;
                        }
                        out.push(m);
                    }
                    let s = conv_summary(&tx, target, &auth.user_id)?;
                    em.emit(&tx, &auth.user_id, EventData::ReadUpdate { conv_id: target.clone(), last_read_seq: s.last_read_seq, marked_unread: false, unread: s.unread, mentions: s.mentions })?;
                }
                tx.commit()?;
                em.flush(&hub);
                Ok(out)
            })
            .await?,
    ))
}

fn insert_forward_row(tx: &rusqlite::Connection, conv: &str, sender: &str, body: &str, fwd: Option<&ForwardInfo>, atts: &[String]) -> AppResult<String> {
    let now = now_ms();
    let seq: i64 = tx.query_row("UPDATE conversations SET last_seq=last_seq+1, last_ts=?2 WHERE id=?1 RETURNING last_seq", params![conv, now], |r| r.get(0))?;
    let mid = new_id();
    let kind = if body.trim().is_empty() { "file" } else { "text" };
    let fwd_json = fwd.map(serde_json::to_string).transpose()?;
    tx.execute(
        "INSERT INTO messages(id,conv_id,seq,sender_id,client_id,body,kind,ts,mentions,fwd) VALUES(?1,?2,?3,?4,NULL,?5,?6,?7,'[]',?8)",
        params![mid, conv, seq, sender, body, kind, now, fwd_json],
    )?;
    for a in atts {
        tx.execute("INSERT OR IGNORE INTO message_attachments(message_id,attachment_id) VALUES(?1,?2)", params![mid, a])?;
    }
    tx.execute(
        "INSERT INTO read_state(user_id,conv_id,last_read_seq,marked_unread) VALUES(?1,?2,?3,0)
         ON CONFLICT(user_id,conv_id) DO UPDATE SET last_read_seq=MAX(last_read_seq, excluded.last_read_seq), marked_unread=0",
        params![sender, conv, seq],
    )?;
    Ok(mid)
}

// ------------------------------------------------------------------ messages by id (permalinks, M-19)

pub async fn get_message(State(st): State<St>, auth: Auth, Path(id): Path<String>) -> J<Message> {
    Ok(Json(
        st.db
            .call(move |c| {
                let m = load_message(c, &id)?;
                member_conv(c, &m.conv_id, &auth.user_id)?;
                let hidden: i64 = c.query_row("SELECT COUNT(*) FROM message_hidden WHERE user_id=?1 AND message_id=?2", params![auth.user_id, id], |r| r.get(0))?;
                if hidden > 0 {
                    return Err(AppError::not_found("message not found"));
                }
                Ok(m)
            })
            .await?,
    ))
}

// ------------------------------------------------------------------ files browser (F-05)

#[derive(Deserialize)]
pub struct FilesParams {
    conv: Option<String>,
    /// image, video, audio or document (also accepted as `kind`)
    #[serde(rename = "type", alias = "kind")]
    kind: Option<String>,
    q: Option<String>,
    before: Option<i64>,
    /// Paging: skip this many files (newest first).
    offset: Option<i64>,
    limit: Option<i64>,
}

pub fn type_clause(kind: &str) -> Option<&'static str> {
    match kind {
        "image" => Some("a.content_type LIKE 'image/%'"),
        "video" => Some("a.content_type LIKE 'video/%'"),
        "audio" => Some("a.content_type LIKE 'audio/%'"),
        "document" => Some("(a.content_type NOT LIKE 'image/%' AND a.content_type NOT LIKE 'video/%' AND a.content_type NOT LIKE 'audio/%')"),
        _ => None,
    }
}

pub fn like_escape(q: &str) -> String {
    format!("%{}%", q.replace('\\', "\\\\").replace('%', "\\%").replace('_', "\\_"))
}

/// Files visible to the caller (member conversations, not hidden), newest first.
pub async fn list_files(State(st): State<St>, auth: Auth, Query(p): Query<FilesParams>) -> J<Vec<FileHit>> {
    Ok(Json(st.db.call(move |c| search_files(c, &auth.user_id, p.conv.as_deref(), p.kind.as_deref(), p.q.as_deref(), p.before, p.limit.unwrap_or(50), p.offset.unwrap_or(0))).await?))
}

pub fn search_files(c: &rusqlite::Connection, user_id: &str, conv: Option<&str>, kind: Option<&str>, q: Option<&str>, before: Option<i64>, limit: i64, offset: i64) -> AppResult<Vec<FileHit>> {
    let vis = visible_sql("?1");
    let mut sql = format!(
        "SELECT a.id,a.name,a.content_type,a.size,a.removed,m.id,m.conv_id,m.sender_id,m.ts,a.meta FROM message_attachments ma
         JOIN attachments a ON a.id=ma.attachment_id JOIN messages m ON m.id=ma.message_id
         JOIN members mem ON mem.conv_id=m.conv_id AND mem.user_id=?1
         WHERE m.deleted=0 AND a.purpose='file' AND {vis} "
    );
    let mut args: Vec<Box<dyn rusqlite::ToSql>> = vec![Box::new(user_id.to_string())];
    if let Some(cv) = conv {
        sql.push_str(&format!("AND m.conv_id=?{} ", args.len() + 1));
        args.push(Box::new(cv.to_string()));
    }
    if let Some(t) = kind.and_then(type_clause) {
        sql.push_str(&format!("AND {t} "));
    }
    if let Some(q) = q.map(str::trim).filter(|q| !q.is_empty()) {
        sql.push_str(&format!("AND a.name LIKE ?{} ESCAPE '\\' ", args.len() + 1));
        args.push(Box::new(like_escape(q)));
    }
    if let Some(b) = before {
        sql.push_str(&format!("AND m.ts<?{} ", args.len() + 1));
        args.push(Box::new(b));
    }
    sql.push_str(&format!("ORDER BY m.ts DESC, a.rowid DESC LIMIT {} OFFSET {}", limit.clamp(1, 200), offset.max(0)));
    let mut st = c.prepare(&sql)?;
    let refs: Vec<&dyn rusqlite::ToSql> = args.iter().map(|b| b.as_ref()).collect();
    let rows = st
        .query_map(refs.as_slice(), |r| {
            Ok(FileHit {
                attachment: Attachment { id: r.get(0)?, name: r.get(1)?, content_type: r.get(2)?, size: r.get(3)?, removed: r.get::<_, i64>(4)? != 0, meta: r.get::<_, Option<String>>(9)?.and_then(|j| serde_json::from_str(&j).ok()) },
                message_id: r.get(5)?,
                conv_id: r.get(6)?,
                sender_id: r.get(7)?,
                ts: r.get(8)?,
            })
        })?
        .collect::<Result<Vec<_>, _>>()?;
    Ok(rows)
}
