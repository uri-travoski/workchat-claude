//! Shared read helpers: users, conversations, messages.

use crate::error::{AppError, AppResult};
use crate::hub::Hub;
use proto::*;
use rusqlite::{params, Connection, OptionalExtension, Row};
use std::collections::HashMap;

pub const USER_COLS: &str = "id,username,display_name,role,disabled,deleted,is_bot,hide_last_active,privacy_controls_enabled,presence_pref,last_active_ms,avatar_attachment,email,password_set,created_ms,title,phone,read_receipts";

#[derive(Clone, Debug)]
pub struct UserRow {
    pub id: String,
    pub username: String,
    pub display_name: String,
    pub role: Role,
    pub disabled: bool,
    pub deleted: bool,
    pub is_bot: bool,
    pub hide_last_active: bool,
    pub privacy_controls_enabled: bool,
    pub presence_pref: String,
    pub last_active_ms: Option<i64>,
    pub avatar: Option<String>,
    pub email: Option<String>,
    pub password_set: bool,
    pub created_ms: i64,
    pub title: Option<String>,
    pub phone: Option<String>,
    pub read_receipts: bool,
}

pub fn user_row(r: &Row) -> rusqlite::Result<UserRow> {
    Ok(UserRow {
        id: r.get(0)?,
        username: r.get(1)?,
        display_name: r.get(2)?,
        role: Role::parse(&r.get::<_, String>(3)?).unwrap_or(Role::Member),
        disabled: r.get::<_, i64>(4)? != 0,
        deleted: r.get::<_, i64>(5)? != 0,
        is_bot: r.get::<_, i64>(6)? != 0,
        hide_last_active: r.get::<_, i64>(7)? != 0,
        privacy_controls_enabled: r.get::<_, i64>(8)? != 0,
        presence_pref: r.get(9)?,
        last_active_ms: r.get(10)?,
        avatar: r.get(11)?,
        email: r.get(12)?,
        password_set: r.get::<_, i64>(13)? != 0,
        created_ms: r.get(14)?,
        title: r.get(15)?,
        phone: r.get(16)?,
        read_receipts: r.get::<_, i64>(17)? != 0,
    })
}

pub fn get_user(conn: &Connection, id: &str) -> AppResult<UserRow> {
    conn.query_row(&format!("SELECT {USER_COLS} FROM users WHERE id=?1"), [id], user_row)
        .optional()?
        .ok_or_else(|| AppError::not_found("user not found"))
}

pub fn presence_of(row: &UserRow, hub: &Hub) -> Presence {
    if row.disabled || row.deleted {
        return Presence::Offline;
    }
    match row.presence_pref.as_str() {
        "dnd" => Presence::Dnd,
        "away" => Presence::Away,
        "auto_away" => {
            if hub.is_online(&row.id) {
                Presence::Away
            } else {
                Presence::Offline
            }
        }
        _ => {
            if hub.is_online(&row.id) {
                Presence::Online
            } else {
                Presence::Offline
            }
        }
    }
}

/// Last active time as other users may see it (SV-12).
pub fn visible_last_active(row: &UserRow) -> Option<i64> {
    if row.hide_last_active && row.privacy_controls_enabled {
        None
    } else {
        row.last_active_ms
    }
}

pub fn to_user(row: &UserRow, hub: &Hub) -> User {
    User {
        id: row.id.clone(),
        username: if row.deleted { "deleted".into() } else { row.username.clone() },
        display_name: if row.deleted { "Deleted user".into() } else { row.display_name.clone() },
        role: row.role,
        avatar: if row.deleted { None } else { row.avatar.clone() },
        disabled: row.disabled,
        is_bot: row.is_bot,
        last_active_ms: if row.deleted { None } else { visible_last_active(row) },
        presence: Some(presence_of(row, hub)),
        title: if row.deleted { None } else { row.title.clone().filter(|t| !t.is_empty()) },
        phone: if row.deleted { None } else { row.phone.clone().filter(|t| !t.is_empty()) },
    }
}

pub fn to_me(row: &UserRow, hub: &Hub) -> Me {
    Me {
        user: to_user(row, hub),
        privacy_controls_enabled: row.privacy_controls_enabled,
        hide_last_active: row.hide_last_active,
        presence_pref: if row.presence_pref == "auto_away" { "auto".into() } else { row.presence_pref.clone() },
        password_set: row.password_set,
        email: row.email.clone(),
        read_receipts: row.read_receipts,
    }
}

pub fn all_users(conn: &Connection, hub: &Hub) -> AppResult<Vec<User>> {
    let mut st = conn.prepare(&format!("SELECT {USER_COLS} FROM users ORDER BY display_name COLLATE NOCASE"))?;
    let rows = st.query_map([], user_row)?.collect::<Result<Vec<_>, _>>()?;
    Ok(rows.iter().map(|r| to_user(r, hub)).collect())
}

pub fn user_ids_all_active(conn: &Connection) -> AppResult<Vec<String>> {
    let mut st = conn.prepare("SELECT id FROM users WHERE deleted=0 AND disabled=0")?;
    let v = st.query_map([], |r| r.get(0))?.collect::<Result<Vec<String>, _>>()?;
    Ok(v)
}

// ------------------------------------------------------------------ conversations

pub struct ConvRow {
    pub id: String,
    pub kind: ConversationKind,
    pub archived: bool,
}

pub fn conv_row(conn: &Connection, id: &str) -> AppResult<ConvRow> {
    conn.query_row("SELECT id,kind,archived FROM conversations WHERE id=?1", [id], |r| {
        Ok(ConvRow {
            id: r.get(0)?,
            kind: ConversationKind::parse(&r.get::<_, String>(1)?).unwrap_or(ConversationKind::Dm),
            archived: r.get::<_, i64>(2)? != 0,
        })
    })
    .optional()?
    .ok_or_else(|| AppError::not_found("conversation not found"))
}

pub fn is_member(conn: &Connection, conv_id: &str, user_id: &str) -> AppResult<bool> {
    Ok(conn
        .query_row(
            "SELECT 1 FROM members WHERE conv_id=?1 AND user_id=?2",
            [conv_id, user_id],
            |_| Ok(()),
        )
        .optional()?
        .is_some())
}

/// Conversation row for a member, or `not_found` (members-only, hidden from non-members).
pub fn member_conv(conn: &Connection, conv_id: &str, user_id: &str) -> AppResult<ConvRow> {
    let c = conv_row(conn, conv_id)?;
    if !is_member(conn, conv_id, user_id)? {
        return Err(AppError::not_found("conversation not found"));
    }
    Ok(c)
}

pub fn member_ids(conn: &Connection, conv_id: &str) -> AppResult<Vec<String>> {
    let mut st = conn.prepare("SELECT user_id FROM members WHERE conv_id=?1 ORDER BY joined_ms, user_id")?;
    let v = st.query_map([conv_id], |r| r.get(0))?.collect::<Result<Vec<String>, _>>()?;
    Ok(v)
}

pub fn load_conversation(conn: &Connection, id: &str) -> AppResult<Conversation> {
    let mut c = conn
        .query_row(
            "SELECT id,kind,name,topic,archived,created_ms,last_seq,last_ts,avatar_attachment FROM conversations WHERE id=?1",
            [id],
            |r| {
                Ok(Conversation {
                    id: r.get(0)?,
                    kind: ConversationKind::parse(&r.get::<_, String>(1)?).unwrap_or(ConversationKind::Dm),
                    name: r.get(2)?,
                    topic: r.get(3)?,
                    archived: r.get::<_, i64>(4)? != 0,
                    created_ms: r.get(5)?,
                    members: vec![],
                    last_seq: r.get(6)?,
                    last_message_ts: r.get(7)?,
                    avatar: r.get(8)?,
                })
            },
        )
        .optional()?
        .ok_or_else(|| AppError::not_found("conversation not found"))?;
    // Read receipts: only DMs and group DMs, only members who have them switched on.
    let with_receipts = matches!(c.kind, ConversationKind::Dm | ConversationKind::GroupDm);
    let mut st = conn.prepare(
        "SELECT m.user_id, m.role, rs.last_read_seq, u.read_receipts FROM members m
         LEFT JOIN read_state rs ON rs.conv_id=m.conv_id AND rs.user_id=m.user_id
         LEFT JOIN users u ON u.id=m.user_id
         WHERE m.conv_id=?1 ORDER BY m.joined_ms, m.user_id",
    )?;
    c.members = st
        .query_map([id], |r| {
            let on = r.get::<_, Option<i64>>(3)?.unwrap_or(1) != 0;
            let seq: Option<i64> = r.get(2)?;
            Ok(Member { user_id: r.get(0)?, role: r.get(1)?, last_read_seq: if with_receipts && on { Some(seq.unwrap_or(0)) } else { None } })
        })?
        .collect::<Result<_, _>>()?;
    Ok(c)
}

const VISIBLE: &str = "NOT EXISTS (SELECT 1 FROM message_hidden h WHERE h.user_id=?U AND h.message_id=m.id)";

fn visible_for(user_param: &str) -> String {
    VISIBLE.replace("?U", user_param)
}

pub fn conv_summary(conn: &Connection, conv_id: &str, user_id: &str) -> AppResult<ConversationSummary> {
    let conversation = load_conversation(conn, conv_id)?;
    let (last_read, marked): (i64, i64) = conn
        .query_row(
            "SELECT last_read_seq, marked_unread FROM read_state WHERE user_id=?1 AND conv_id=?2",
            [user_id, conv_id],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .optional()?
        .unwrap_or((0, 0));
    let vis = visible_for("?3");
    let unread: i64 = conn.query_row(
        &format!(
            "SELECT COUNT(*) FROM messages m WHERE m.conv_id=?1 AND m.seq>?2 AND m.sender_id!=?3 AND m.deleted=0 AND m.kind!='system' AND {vis}"
        ),
        params![conv_id, last_read, user_id],
        |r| r.get(0),
    )?;
    let mentions: i64 = conn.query_row(
        &format!(
            "SELECT COUNT(*) FROM messages m WHERE m.conv_id=?1 AND m.seq>?2 AND m.sender_id!=?3 AND m.deleted=0 AND m.kind!='system' AND m.mentions LIKE ?4 AND {vis}"
        ),
        params![conv_id, last_read, user_id, format!("%\"{user_id}\"%")],
        |r| r.get(0),
    )?;
    Ok(ConversationSummary { conversation, last_read_seq: last_read, marked_unread: marked != 0, unread, mentions })
}

pub fn user_conversation_ids(conn: &Connection, user_id: &str) -> AppResult<Vec<String>> {
    let mut st = conn.prepare(
        "SELECT c.id FROM conversations c JOIN members m ON m.conv_id=c.id WHERE m.user_id=?1 ORDER BY COALESCE(c.last_ts, c.created_ms) DESC",
    )?;
    let v = st.query_map([user_id], |r| r.get(0))?.collect::<Result<Vec<String>, _>>()?;
    Ok(v)
}

// ------------------------------------------------------------------ messages

pub const MSG_COLS: &str = "m.id,m.conv_id,m.seq,m.sender_id,m.client_id,m.body,m.kind,m.ts,m.version,m.edited_ms,m.deleted,m.thread_root,m.reply_to,m.mentions,m.delivered_ms,m.fwd,m.form,m.also_channel";

pub fn msg_base(r: &Row) -> rusqlite::Result<Message> {
    let kind = match r.get::<_, String>(6)?.as_str() {
        "file" => MessageKind::File,
        "system" => MessageKind::System,
        _ => MessageKind::Text,
    };
    let mentions: String = r.get(13)?;
    Ok(Message {
        id: r.get(0)?,
        conv_id: r.get(1)?,
        seq: r.get(2)?,
        sender_id: r.get(3)?,
        client_id: r.get(4)?,
        body: r.get(5)?,
        kind,
        ts: r.get(7)?,
        version: r.get(8)?,
        edited_ms: r.get(9)?,
        deleted: r.get::<_, i64>(10)? != 0,
        thread_root: r.get(11)?,
        reply_to: r.get(12)?,
        mentions: serde_json::from_str(&mentions).unwrap_or_default(),
        attachments: vec![],
        reactions: vec![],
        delivered: r.get::<_, Option<i64>>(14)?.is_some(),
        forwarded: r.get::<_, Option<String>>(15)?.and_then(|j| serde_json::from_str(&j).ok()),
        form: r.get::<_, Option<String>>(16)?.and_then(|j| serde_json::from_str(&j).ok()),
        responded: vec![],
        also_send_to_channel: r.get::<_, i64>(17)? != 0,
    })
}

/// Adds attachments and reactions to message rows.
pub fn hydrate(conn: &Connection, mut msgs: Vec<Message>) -> AppResult<Vec<Message>> {
    if msgs.is_empty() {
        return Ok(msgs);
    }
    let ids = serde_json::to_string(&msgs.iter().map(|m| m.id.clone()).collect::<Vec<_>>())?;
    let mut atts: HashMap<String, Vec<Attachment>> = HashMap::new();
    {
        let mut st = conn.prepare(
            "SELECT ma.message_id, a.id, a.name, a.content_type, a.size, a.removed, a.meta FROM message_attachments ma
             JOIN attachments a ON a.id=ma.attachment_id WHERE ma.message_id IN (SELECT value FROM json_each(?1))",
        )?;
        let rows = st.query_map([&ids], |r| {
            Ok((
                r.get::<_, String>(0)?,
                Attachment {
                    id: r.get(1)?,
                    name: r.get(2)?,
                    content_type: r.get(3)?,
                    size: r.get(4)?,
                    removed: r.get::<_, i64>(5)? != 0,
                    meta: r.get::<_, Option<String>>(6)?.and_then(|j| serde_json::from_str(&j).ok()),
                },
            ))
        })?;
        for row in rows {
            let (mid, a) = row?;
            atts.entry(mid).or_default().push(a);
        }
    }
    let mut reacts: HashMap<String, Vec<ReactionSummary>> = HashMap::new();
    {
        let mut st = conn.prepare(
            "SELECT message_id, emoji, user_id FROM reactions WHERE message_id IN (SELECT value FROM json_each(?1)) ORDER BY ts, rowid",
        )?;
        let rows = st.query_map([&ids], |r| {
            Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?, r.get::<_, String>(2)?))
        })?;
        for row in rows {
            let (mid, emoji, uid) = row?;
            let list = reacts.entry(mid).or_default();
            if let Some(rs) = list.iter_mut().find(|r| r.emoji == emoji) {
                rs.user_ids.push(uid);
            } else {
                list.push(ReactionSummary { emoji, user_ids: vec![uid] });
            }
        }
    }
    let mut responded: HashMap<String, Vec<String>> = HashMap::new();
    if msgs.iter().any(|m| m.form.is_some()) {
        let mut st = conn.prepare(
            "SELECT message_id, user_id FROM form_responses WHERE message_id IN (SELECT value FROM json_each(?1)) ORDER BY ts",
        )?;
        let rows = st.query_map([&ids], |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?)))?;
        for row in rows {
            let (mid, uid) = row?;
            let l = responded.entry(mid).or_default();
            if !l.contains(&uid) {
                l.push(uid);
            }
        }
    }
    for m in &mut msgs {
        if let Some(r) = responded.remove(&m.id) {
            m.responded = r;
        }
        if let Some(a) = atts.remove(&m.id) {
            m.attachments = a;
        }
        if let Some(r) = reacts.remove(&m.id) {
            m.reactions = r;
        }
        if m.deleted {
            m.body.clear();
            m.attachments.clear();
            m.reactions.clear();
        }
    }
    Ok(msgs)
}

pub fn load_message(conn: &Connection, id: &str) -> AppResult<Message> {
    let m = conn
        .query_row(&format!("SELECT {MSG_COLS} FROM messages m WHERE m.id=?1"), [id], msg_base)
        .optional()?
        .ok_or_else(|| AppError::not_found("message not found"))?;
    Ok(hydrate(conn, vec![m])?.remove(0))
}

/// Members who still see the message (not hidden for them).
pub fn recipients(conn: &Connection, conv_id: &str, message_id: &str) -> AppResult<Vec<String>> {
    let mut st = conn.prepare(
        "SELECT user_id FROM members m WHERE conv_id=?1 AND NOT EXISTS (SELECT 1 FROM message_hidden h WHERE h.user_id=m.user_id AND h.message_id=?2)",
    )?;
    let v = st.query_map([conv_id, message_id], |r| r.get(0))?.collect::<Result<Vec<String>, _>>()?;
    Ok(v)
}

pub struct HistoryQuery {
    pub before_seq: Option<i64>,
    pub after_seq: Option<i64>,
    pub around_ts: Option<i64>,
    /// Window around this sequence number (the message itself is included).
    pub around_seq: Option<i64>,
    pub limit: i64,
}

fn visible_query(conn: &Connection, conv_id: &str, user_id: &str, cond: &str, order: &str, limit: i64, extra: &[&dyn rusqlite::ToSql]) -> AppResult<Vec<Message>> {
    let vis = visible_for("?2");
    let sql = format!(
        "SELECT {MSG_COLS} FROM messages m WHERE m.conv_id=?1 AND {vis} AND {cond} ORDER BY m.seq {order} LIMIT {limit}"
    );
    let mut st = conn.prepare(&sql)?;
    let mut all: Vec<&dyn rusqlite::ToSql> = vec![&conv_id, &user_id];
    all.extend_from_slice(extra);
    let rows = st.query_map(all.as_slice(), msg_base)?.collect::<Result<Vec<_>, _>>()?;
    Ok(rows)
}

pub fn history(conn: &Connection, conv_id: &str, user_id: &str, q: &HistoryQuery) -> AppResult<HistoryResponse> {
    let limit = q.limit.clamp(1, 200);
    let mut msgs: Vec<Message>;
    if q.around_ts.is_some() || q.around_seq.is_some() {
        // First visible message at or after the timestamp (or the given message), then a window on both sides.
        let pivot_seq = match (q.around_seq, q.around_ts) {
            (Some(s), _) => Some(s),
            (None, Some(ts)) => visible_query(conn, conv_id, user_id, "m.ts>=?3", "ASC", 1, &[&ts])?.first().map(|m| m.seq),
            _ => None,
        };
        let pivot = match pivot_seq {
            Some(p) => p,
            None => {
                // After the last message: show the latest page.
                let mut older = visible_query(conn, conv_id, user_id, "1=1", "DESC", limit, &[])?;
                older.reverse();
                let has_before = older.first().map(|m| has_older(conn, conv_id, user_id, m.seq)).transpose()?.unwrap_or(false);
                return Ok(HistoryResponse { messages: hydrate(conn, older)?, has_more_before: has_before, has_more_after: false });
            }
        };
        let half = (limit / 2).max(1);
        let mut before = visible_query(conn, conv_id, user_id, "m.seq<?3", "DESC", half, &[&pivot])?;
        before.reverse();
        let after = visible_query(conn, conv_id, user_id, "m.seq>=?3", "ASC", limit - half, &[&pivot])?;
        msgs = before;
        msgs.extend(after);
    } else if let Some(a) = q.after_seq {
        msgs = visible_query(conn, conv_id, user_id, "m.seq>?3", "ASC", limit, &[&a])?;
    } else if let Some(b) = q.before_seq {
        msgs = visible_query(conn, conv_id, user_id, "m.seq<?3", "DESC", limit, &[&b])?;
        msgs.reverse();
    } else {
        msgs = visible_query(conn, conv_id, user_id, "1=1", "DESC", limit, &[])?;
        msgs.reverse();
    }
    let has_before = match msgs.first() {
        Some(m) => has_older(conn, conv_id, user_id, m.seq)?,
        None => false,
    };
    let has_after = match msgs.last() {
        Some(m) => !visible_query(conn, conv_id, user_id, "m.seq>?3", "ASC", 1, &[&m.seq])?.is_empty(),
        None => false,
    };
    Ok(HistoryResponse { messages: hydrate(conn, msgs)?, has_more_before: has_before, has_more_after: has_after })
}

fn has_older(conn: &Connection, conv_id: &str, user_id: &str, seq: i64) -> AppResult<bool> {
    Ok(!visible_query(conn, conv_id, user_id, "m.seq<?3", "DESC", 1, &[&seq])?.is_empty())
}

/// SQL fragment (for use with `?U` replaced) deciding visibility of message alias `m`.
pub fn visible_sql(user_param: &str) -> String {
    visible_for(user_param)
}
