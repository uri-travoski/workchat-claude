//! Organisation data shared by routes, bootstrap and events: pins, bookmarks, user groups,
//! threads and follows, per-conversation preferences, sections, drafts, mention expansion.

use crate::db::now_ms;
use crate::emit::Emitter;
use crate::error::{AppError, AppResult};
use crate::hub::Hub;
use crate::queries::*;
use proto::*;
use rusqlite::{params, Connection, OptionalExtension};
use std::collections::BTreeMap;

pub const MAX_PINS: i64 = 100;

// ------------------------------------------------------------------ pins and bookmarks

pub fn load_pins(conn: &Connection, conv_id: &str) -> AppResult<Vec<Pin>> {
    let mut st = conn.prepare("SELECT conv_id,message_id,pinned_by,ts FROM pins WHERE conv_id=?1 ORDER BY ts DESC, rowid DESC")?;
    let v = st
        .query_map([conv_id], |r| Ok(Pin { conv_id: r.get(0)?, message_id: r.get(1)?, pinned_by: r.get(2)?, ts: r.get(3)? }))?
        .collect::<Result<Vec<_>, _>>()?;
    Ok(v)
}

pub fn load_bookmarks(conn: &Connection, conv_id: &str) -> AppResult<Vec<Bookmark>> {
    let mut st = conn.prepare("SELECT id,conv_id,title,url,attachment_id,position,created_by FROM bookmarks WHERE conv_id=?1 ORDER BY position, created_ms, rowid")?;
    let v = st
        .query_map([conv_id], |r| {
            Ok(Bookmark { id: r.get(0)?, conv_id: r.get(1)?, title: r.get(2)?, url: r.get(3)?, attachment_id: r.get(4)?, position: r.get(5)?, created_by: r.get(6)? })
        })?
        .collect::<Result<Vec<_>, _>>()?;
    Ok(v)
}

/// Members of the conversation (they receive pin / bookmark events).
pub fn emit_pins(conn: &Connection, em: &mut Emitter, conv_id: &str) -> AppResult<()> {
    let pins = load_pins(conn, conv_id)?;
    for u in member_ids(conn, conv_id)? {
        em.emit(conn, &u, EventData::PinsUpdate { conv_id: conv_id.to_string(), pins: pins.clone() })?;
    }
    Ok(())
}

pub fn emit_bookmarks(conn: &Connection, em: &mut Emitter, conv_id: &str) -> AppResult<()> {
    let b = load_bookmarks(conn, conv_id)?;
    for u in member_ids(conn, conv_id)? {
        em.emit(conn, &u, EventData::BookmarksUpdate { conv_id: conv_id.to_string(), bookmarks: b.clone() })?;
    }
    Ok(())
}

// ------------------------------------------------------------------ user groups

pub fn load_groups(conn: &Connection) -> AppResult<Vec<UserGroup>> {
    let mut st = conn.prepare("SELECT id,handle,name FROM user_groups ORDER BY handle COLLATE NOCASE")?;
    let rows = st
        .query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?, r.get::<_, String>(2)?)))?
        .collect::<Result<Vec<_>, _>>()?;
    let mut out = vec![];
    for (id, handle, name) in rows {
        let mut m = conn.prepare("SELECT user_id FROM user_group_members WHERE group_id=?1 ORDER BY user_id")?;
        let member_ids = m.query_map([&id], |r| r.get::<_, String>(0))?.collect::<Result<Vec<_>, _>>()?;
        out.push(UserGroup { id, handle, name, member_ids });
    }
    Ok(out)
}

pub fn emit_groups(conn: &Connection, em: &mut Emitter) -> AppResult<()> {
    let groups = load_groups(conn)?;
    for u in user_ids_all_active(conn)? {
        em.emit(conn, &u, EventData::GroupsUpdate { groups: groups.clone() })?;
    }
    Ok(())
}

pub fn valid_handle(h: &str) -> bool {
    (2..=32).contains(&h.len())
        && h.chars().all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_'))
        && !rules::is_broadcast(&h.to_lowercase())
}

/// M-17 / M-18: the structured mention list for a message. Starts from the mentions the client
/// sent, then expands `@channel` / `@everyone` (all members), `@here` (members currently online)
/// and `@group` handles into member ids. Only members of the conversation can be mentioned and
/// the sender is never mentioned.
pub fn expand_mentions(conn: &Connection, hub: &Hub, conv_id: &str, sender: &str, body: &str, given: &[String]) -> AppResult<Vec<String>> {
    let members = member_ids(conn, conv_id)?;
    let mut out: Vec<String> = vec![];
    let add = |id: &str, out: &mut Vec<String>| {
        if id != sender && members.iter().any(|m| m == id) && !out.iter().any(|o| o == id) {
            out.push(id.to_string());
        }
    };
    for g in given {
        add(g, &mut out);
    }
    for tok in rules::at_tokens(body) {
        match tok.as_str() {
            "channel" | "everyone" => {
                for m in &members {
                    add(m, &mut out);
                }
            }
            "here" => {
                for m in &members {
                    if hub.is_online(m) {
                        add(m, &mut out);
                    }
                }
            }
            other => {
                let gid: Option<String> = conn.query_row("SELECT id FROM user_groups WHERE handle=?1 COLLATE NOCASE", [other], |r| r.get(0)).optional()?;
                if let Some(gid) = gid {
                    let mut st = conn.prepare("SELECT user_id FROM user_group_members WHERE group_id=?1")?;
                    let ids = st.query_map([gid], |r| r.get::<_, String>(0))?.collect::<Result<Vec<_>, _>>()?;
                    for id in ids {
                        add(&id, &mut out);
                    }
                }
            }
        }
    }
    Ok(out)
}

// ------------------------------------------------------------------ preferences, sections, drafts

pub fn load_prefs(conn: &Connection, user_id: &str) -> AppResult<Vec<ConvPref>> {
    let mut st = conn.prepare("SELECT conv_id,favourite,section_id,mute_until_ms,notify,position FROM conv_prefs WHERE user_id=?1")?;
    let v = st
        .query_map([user_id], |r| {
            Ok(ConvPref { conv_id: r.get(0)?, favourite: r.get::<_, i64>(1)? != 0, section_id: r.get(2)?, mute_until_ms: r.get(3)?, notify: r.get(4)?, position: r.get(5)? })
        })?
        .collect::<Result<Vec<_>, _>>()?;
    Ok(v)
}

pub fn pref_for(conn: &Connection, user_id: &str, conv_id: &str) -> AppResult<Option<ConvPref>> {
    Ok(conn
        .query_row(
            "SELECT conv_id,favourite,section_id,mute_until_ms,notify,position FROM conv_prefs WHERE user_id=?1 AND conv_id=?2",
            [user_id, conv_id],
            |r| Ok(ConvPref { conv_id: r.get(0)?, favourite: r.get::<_, i64>(1)? != 0, section_id: r.get(2)?, mute_until_ms: r.get(3)?, notify: r.get(4)?, position: r.get(5)? }),
        )
        .optional()?)
}

pub fn load_sections(conn: &Connection, user_id: &str) -> AppResult<Vec<Section>> {
    let mut st = conn.prepare("SELECT id,name,position,collapsed FROM sections WHERE user_id=?1 ORDER BY position, rowid")?;
    let v = st
        .query_map([user_id], |r| Ok(Section { id: r.get(0)?, name: r.get(1)?, position: r.get(2)?, collapsed: r.get::<_, i64>(3)? != 0 }))?
        .collect::<Result<Vec<_>, _>>()?;
    Ok(v)
}

pub fn load_drafts(conn: &Connection, user_id: &str) -> AppResult<Vec<DraftItem>> {
    let mut st = conn.prepare("SELECT conv_id,thread_root,body,updated_ms FROM drafts WHERE user_id=?1 ORDER BY updated_ms DESC")?;
    let v = st
        .query_map([user_id], |r| {
            let tr: String = r.get(1)?;
            Ok(DraftItem { conv_id: r.get(0)?, thread_root: if tr.is_empty() { None } else { Some(tr) }, body: r.get(2)?, updated_ms: r.get(3)? })
        })?
        .collect::<Result<Vec<_>, _>>()?;
    Ok(v)
}

// ------------------------------------------------------------------ threads

/// The root of the thread a message belongs to (messages replying inside a thread resolve to its root).
pub fn resolve_thread_root(conn: &Connection, conv_id: &str, candidate: &str) -> AppResult<String> {
    let row: Option<(String, Option<String>)> = conn
        .query_row("SELECT conv_id, thread_root FROM messages WHERE id=?1", [candidate], |r| Ok((r.get(0)?, r.get(1)?)))
        .optional()?;
    match row {
        Some((c, tr)) if c == conv_id => Ok(tr.unwrap_or_else(|| candidate.to_string())),
        _ => Err(AppError::validation("referenced message not found in this conversation")),
    }
}

pub fn thread_summary(conn: &Connection, root_id: &str, user_id: &str) -> AppResult<ThreadSummary> {
    let root = load_message(conn, root_id)?;
    let vis = visible_sql("?2");
    let (count, last, participants): (i64, Option<i64>, Vec<String>) = {
        let count: i64 = conn.query_row(
            &format!("SELECT COUNT(*) FROM messages m WHERE m.thread_root=?1 AND m.deleted=0 AND {vis}"),
            params![root_id, user_id],
            |r| r.get(0),
        )?;
        let last: Option<i64> = conn.query_row(
            &format!("SELECT MAX(m.ts) FROM messages m WHERE m.thread_root=?1 AND m.deleted=0 AND {vis}"),
            params![root_id, user_id],
            |r| r.get(0),
        )?;
        let mut st = conn.prepare(&format!("SELECT DISTINCT m.sender_id FROM messages m WHERE m.thread_root=?1 AND m.deleted=0 AND {vis} ORDER BY m.seq"))?;
        let mut p = st.query_map(params![root_id, user_id], |r| r.get::<_, String>(0))?.collect::<Result<Vec<_>, _>>()?;
        if !p.contains(&root.sender_id) {
            p.insert(0, root.sender_id.clone());
        }
        (count, last, p)
    };
    let fol: Option<(i64, i64)> = conn
        .query_row("SELECT following,last_read_seq FROM thread_follows WHERE user_id=?1 AND thread_root=?2", params![user_id, root_id], |r| Ok((r.get(0)?, r.get(1)?)))
        .optional()?;
    let (following, last_read) = fol.map(|(f, l)| (f != 0, l)).unwrap_or((false, 0));
    let unread: i64 = conn.query_row(
        &format!("SELECT COUNT(*) FROM messages m WHERE m.thread_root=?1 AND m.deleted=0 AND m.sender_id!=?3 AND m.seq>?4 AND {vis}"),
        params![root_id, user_id, user_id, last_read],
        |r| r.get(0),
    )?;
    let last_reply_id: Option<String> = conn
        .query_row(
            &format!("SELECT m.id FROM messages m WHERE m.thread_root=?1 AND m.deleted=0 AND {vis} ORDER BY m.seq DESC LIMIT 1"),
            params![root_id, user_id],
            |r| r.get(0),
        )
        .optional()?;
    let last_reply = match last_reply_id {
        Some(id) => Some(load_message(conn, &id)?),
        None => None,
    };
    Ok(ThreadSummary { conv_id: root.conv_id.clone(), root, reply_count: count, last_reply_ts: last.unwrap_or(0), unread, following, participants, last_reply })
}

/// Threads the user follows, newest activity first.
pub fn user_threads(conn: &Connection, user_id: &str) -> AppResult<Vec<ThreadSummary>> {
    let mut st = conn.prepare(
        "SELECT f.thread_root FROM thread_follows f JOIN messages r ON r.id=f.thread_root
         JOIN members mem ON mem.conv_id=r.conv_id AND mem.user_id=f.user_id
         WHERE f.user_id=?1 AND f.following=1
           AND NOT EXISTS (SELECT 1 FROM message_hidden h WHERE h.user_id=f.user_id AND h.message_id=f.thread_root)",
    )?;
    let roots = st.query_map([user_id], |r| r.get::<_, String>(0))?.collect::<Result<Vec<_>, _>>()?;
    let mut out = vec![];
    for r in roots {
        let t = thread_summary(conn, &r, user_id)?;
        if t.root.deleted && t.reply_count == 0 {
            continue;
        }
        out.push(t);
    }
    out.sort_by_key(|t| std::cmp::Reverse(t.last_reply_ts.max(t.root.ts)));
    Ok(out)
}

pub fn set_follow(conn: &Connection, user_id: &str, root_id: &str, conv_id: &str, following: bool, only_if_absent: bool) -> AppResult<bool> {
    let existing: Option<i64> = conn
        .query_row("SELECT following FROM thread_follows WHERE user_id=?1 AND thread_root=?2", params![user_id, root_id], |r| r.get(0))
        .optional()?;
    if only_if_absent && existing.is_some() {
        return Ok(false);
    }
    // Already in the wanted state.
    if existing == Some(following as i64) {
        return Ok(false);
    }
    conn.execute(
        "INSERT INTO thread_follows(user_id,thread_root,conv_id,following,last_read_seq) VALUES(?1,?2,?3,?4,0)
         ON CONFLICT(user_id,thread_root) DO UPDATE SET following=excluded.following",
        params![user_id, root_id, conv_id, following as i64],
    )?;
    Ok(true)
}

/// Pushes the current summary of one thread to a user (their `following` flag tells clients to add or drop it).
pub fn emit_thread(conn: &Connection, em: &mut Emitter, user_id: &str, root_id: &str) -> AppResult<()> {
    let t = thread_summary(conn, root_id, user_id)?;
    em.emit(conn, user_id, EventData::ThreadUpdate { thread: t })?;
    Ok(())
}

/// Called inside the transaction that stores a reply: auto-follow (M-07) and notify followers.
pub fn on_thread_reply(conn: &Connection, em: &mut Emitter, root_id: &str, conv_id: &str, sender: &str, seq: i64, mentions: &[String]) -> AppResult<()> {
    let root_author: String = conn.query_row("SELECT sender_id FROM messages WHERE id=?1", [root_id], |r| r.get(0))?;
    // Replying always follows (also after an earlier unfollow); the root author and mentioned users follow if they never decided.
    set_follow(conn, sender, root_id, conv_id, true, false)?;
    set_follow(conn, &root_author, root_id, conv_id, true, true)?;
    for m in mentions {
        set_follow(conn, m, root_id, conv_id, true, true)?;
    }
    // The sender has read up to their own reply.
    conn.execute("UPDATE thread_follows SET last_read_seq=MAX(last_read_seq,?3) WHERE user_id=?1 AND thread_root=?2", params![sender, root_id, seq])?;
    let mut st = conn.prepare("SELECT user_id FROM thread_follows WHERE thread_root=?1 AND following=1")?;
    let followers = st.query_map([root_id], |r| r.get::<_, String>(0))?.collect::<Result<Vec<_>, _>>()?;
    for u in followers {
        if is_member(conn, conv_id, &u)? {
            emit_thread(conn, em, &u, root_id)?;
        }
    }
    Ok(())
}

// ------------------------------------------------------------------ bootstrap pieces

pub struct OrgSnapshot {
    pub prefs: Vec<ConvPref>,
    pub sections: Vec<Section>,
    pub groups: Vec<UserGroup>,
    pub drafts: Vec<DraftItem>,
    pub threads: Vec<ThreadSummary>,
    pub pins: BTreeMap<String, Vec<Pin>>,
    pub bookmarks: BTreeMap<String, Vec<Bookmark>>,
}

pub fn snapshot(conn: &Connection, user_id: &str, conv_ids: &[String]) -> AppResult<OrgSnapshot> {
    let mut pins = BTreeMap::new();
    let mut bookmarks = BTreeMap::new();
    for c in conv_ids {
        let p = load_pins(conn, c)?;
        if !p.is_empty() {
            pins.insert(c.clone(), p);
        }
        let b = load_bookmarks(conn, c)?;
        if !b.is_empty() {
            bookmarks.insert(c.clone(), b);
        }
    }
    Ok(OrgSnapshot {
        prefs: load_prefs(conn, user_id)?,
        sections: load_sections(conn, user_id)?,
        groups: load_groups(conn)?,
        drafts: load_drafts(conn, user_id)?,
        threads: user_threads(conn, user_id)?,
        pins,
        bookmarks,
    })
}

/// Whether `user_id` may write pins / bookmarks in the conversation: admins and owner/admin members (Q4).
pub fn can_curate(conn: &Connection, auth_admin: bool, conv_id: &str, user_id: &str) -> AppResult<bool> {
    if auth_admin {
        conv_row(conn, conv_id)?;
        return Ok(true);
    }
    let role: Option<String> = conn
        .query_row("SELECT role FROM members WHERE conv_id=?1 AND user_id=?2", [conv_id, user_id], |r| r.get(0))
        .optional()?;
    Ok(role.as_deref() == Some("owner"))
}

pub fn touch_now() -> i64 {
    now_ms()
}

// ------------------------------------------------------------------ organisation settings, pin and manager rules, channel directory

const ORG_KEY: &str = "org_settings";

pub fn load_org_settings(conn: &Connection) -> AppResult<OrgSettings> {
    Ok(crate::db::get_setting(conn, ORG_KEY)?.and_then(|s| serde_json::from_str(&s).ok()).unwrap_or_default())
}

pub fn save_org_settings(conn: &Connection, s: &OrgSettings) -> AppResult<()> {
    crate::db::set_setting(conn, ORG_KEY, &serde_json::to_string(s)?)
}

/// Sends the settings to every active person.
pub fn emit_org_settings(conn: &Connection, em: &mut Emitter, s: &OrgSettings) -> AppResult<()> {
    for u in user_ids_all_active(conn)? {
        em.emit(conn, &u, EventData::OrgSettingsUpdate { settings: s.clone() })?;
    }
    Ok(())
}

/// The caller's conversation role (`owner` = channel manager), None when not a member.
pub fn member_role(conn: &Connection, conv_id: &str, user_id: &str) -> AppResult<Option<String>> {
    Ok(conn.query_row("SELECT role FROM members WHERE conv_id=?1 AND user_id=?2", [conv_id, user_id], |r| r.get(0)).optional()?)
}

/// Whether the caller may pin and unpin in the conversation. Admins always; members of DMs, group DMs and notes always;
/// members of channels unless the organisation restricts pinning to managers. People outside the conversation get `not_found`.
pub fn can_pin(conn: &Connection, auth_admin: bool, conv_id: &str, user_id: &str) -> AppResult<bool> {
    let cr = conv_row(conn, conv_id)?;
    if auth_admin {
        return Ok(true);
    }
    let role = member_role(conn, conv_id, user_id)?.ok_or_else(|| AppError::not_found("conversation not found"))?;
    if cr.kind != ConversationKind::Channel {
        return Ok(true);
    }
    Ok(load_org_settings(conn)?.channel_pins == ChannelPins::Everyone || role == "owner")
}

/// How a public channel appears in the browse list; None when the conversation is not browsable.
pub fn browse_item(conn: &Connection, conv_id: &str) -> AppResult<Option<ChannelBrowseItem>> {
    let row: Option<(String, Option<String>, Option<String>, Option<String>, i64, String, i64)> = conn
        .query_row(
            "SELECT kind, name, topic, avatar_attachment, archived, visibility, created_ms FROM conversations WHERE id=?1",
            [conv_id],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?, r.get(5)?, r.get(6)?)),
        )
        .optional()?;
    let Some((kind, name, topic, avatar, archived, vis, created_ms)) = row else { return Ok(None) };
    if kind != "channel" || vis != "public" || archived != 0 {
        return Ok(None);
    }
    let member_count: i64 = conn.query_row("SELECT COUNT(*) FROM members WHERE conv_id=?1", [conv_id], |r| r.get(0))?;
    Ok(Some(ChannelBrowseItem { id: conv_id.to_string(), name: name.unwrap_or_default(), topic, avatar, member_count, created_ms }))
}

/// Tells everyone who is NOT in the channel how it now appears in the browse list (or that it is gone from it).
/// `was_listed` is `browse_item(..).is_some()` from before the change; nothing is sent for conversations that
/// were and are not browsable (private channels stay invisible).
pub fn emit_directory(conn: &Connection, em: &mut Emitter, conv_id: &str, was_listed: bool) -> AppResult<()> {
    let item = browse_item(conn, conv_id)?;
    if item.is_none() && !was_listed {
        return Ok(());
    }
    for u in user_ids_all_active(conn)? {
        if !is_member(conn, conv_id, &u)? {
            em.emit(conn, &u, EventData::ChannelDirectoryUpdate { conv_id: conv_id.to_string(), channel: item.clone() })?;
        }
    }
    Ok(())
}
