//! Users, sync (bootstrap + events), conversations, messages, reactions, read state, search.

use crate::auth::Auth;
use crate::db::{new_id, now_ms};
use crate::emit::Emitter;
use crate::error::{AppError, AppResult};
use crate::org;
use crate::queries::*;
use crate::state::St;
use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use axum::Json;
use proto::*;
use rusqlite::{params, Connection, OptionalExtension};
use serde::Deserialize;
use std::collections::BTreeMap;

const MAX_BODY_CHARS: usize = 20_000;

#[derive(Deserialize)]
pub struct UsersParams {
    /// Contacts search: matches name, username and job title (case-insensitive).
    q: Option<String>,
    /// `true`: leave out disabled people.
    active: Option<bool>,
}

/// The user directory (contacts). Bots are included (`is_bot`) and appear like any other person.
pub async fn list_users(State(st): State<St>, _auth: Auth, Query(p): Query<UsersParams>) -> AppResult<Json<Vec<User>>> {
    let hub = st.hub.clone();
    Ok(Json(
        st.db
            .call(move |c| {
                let mut v = all_users(c, &hub)?;
                if p.active == Some(true) {
                    v.retain(|u| !u.disabled);
                }
                if let Some(q) = p.q.as_deref().map(|q| q.trim().to_lowercase()).filter(|q| !q.is_empty()) {
                    v.retain(|u| {
                        u.display_name.to_lowercase().contains(&q) || u.username.to_lowercase().contains(&q) || u.title.as_deref().unwrap_or("").to_lowercase().contains(&q)
                    });
                }
                Ok(v)
            })
            .await?,
    ))
}

pub async fn get_user_route(State(st): State<St>, _auth: Auth, Path(id): Path<String>) -> AppResult<Json<User>> {
    let hub = st.hub.clone();
    Ok(Json(st.db.call(move |c| Ok(to_user(&get_user(c, &id)?, &hub))).await?))
}

// ---------------------------------------------------------------- sync

pub fn current_cursor(conn: &Connection) -> AppResult<i64> {
    Ok(conn
        .query_row("SELECT seq FROM sqlite_sequence WHERE name='events'", [], |r| r.get(0))
        .optional()?
        .unwrap_or(0))
}

const BOOTSTRAP_CONVS_WITH_MESSAGES: usize = 80;
const BOOTSTRAP_MESSAGES: i64 = 40;

pub async fn bootstrap(State(st): State<St>, auth: Auth) -> AppResult<Json<Bootstrap>> {
    let hub = st.hub.clone();
    Ok(Json(
        st.db
            .call(move |c| {
                let cursor = current_cursor(c)?;
                let me = to_me(&get_user(c, &auth.user_id)?, &hub);
                let users = all_users(c, &hub)?;
                let mut conversations = vec![];
                let mut messages = BTreeMap::new();
                let mut has_more = BTreeMap::new();
                let conv_ids = user_conversation_ids(c, &auth.user_id)?;
                let o = org::snapshot(c, &auth.user_id, &conv_ids)?;
                for (i, cid) in conv_ids.into_iter().enumerate() {
                    conversations.push(conv_summary(c, &cid, &auth.user_id)?);
                    if i < BOOTSTRAP_CONVS_WITH_MESSAGES {
                        let h = history(
                            c,
                            &cid,
                            &auth.user_id,
                            &HistoryQuery { before_seq: None, after_seq: None, around_ts: None, around_seq: None, limit: BOOTSTRAP_MESSAGES },
                        )?;
                        has_more.insert(cid.clone(), h.has_more_before);
                        messages.insert(cid, h.messages);
                    } else {
                        has_more.insert(cid, true);
                    }
                }
                Ok(Bootstrap {
                    cursor,
                    server_time: now_ms(),
                    me,
                    users,
                    conversations,
                    messages,
                    has_more,
                    prefs: o.prefs,
                    avatar_overrides: crate::routes::avatars::load_overrides(c, &auth.user_id)?,
                    sections: o.sections,
                    groups: o.groups,
                    drafts: o.drafts,
                    threads: o.threads,
                    pins: o.pins,
                    bookmarks: o.bookmarks,
                    saved: crate::saved::load_saved(c, &auth.user_id)?,
                    labels: crate::saved::load_labels(c, &auth.user_id)?,
                    activity_read_ms: crate::saved::activity_read_ms(c, &auth.user_id)?,
                    org_settings: org::load_org_settings(c)?,
                })
            })
            .await?,
    ))
}

#[derive(Deserialize)]
pub struct EventsQuery {
    #[serde(default)]
    after: i64,
    limit: Option<i64>,
}

pub fn events_floor(conn: &Connection) -> AppResult<i64> {
    Ok(crate::db::get_setting(conn, "events_floor")?.and_then(|v| v.parse().ok()).unwrap_or(0))
}

pub fn fetch_events(conn: &Connection, user_id: &str, after: i64, limit: i64) -> AppResult<(Vec<Event>, bool)> {
    if after < events_floor(conn)? {
        return Err(AppError::new(ErrorCode::CursorExpired, "cursor is older than the retained event log; re-bootstrap"));
    }
    let mut st = conn.prepare("SELECT data FROM events WHERE user_id=?1 AND id>?2 ORDER BY id LIMIT ?3")?;
    let rows = st
        .query_map(params![user_id, after, limit + 1], |r| r.get::<_, String>(0))?
        .collect::<Result<Vec<_>, _>>()?;
    let more = rows.len() as i64 > limit;
    let events = rows
        .into_iter()
        .take(limit as usize)
        .map(|s| serde_json::from_str::<Event>(&s))
        .collect::<Result<Vec<_>, _>>()?;
    Ok((events, more))
}

pub async fn get_events(State(st): State<St>, auth: Auth, Query(q): Query<EventsQuery>) -> AppResult<Json<EventsResponse>> {
    let limit = q.limit.unwrap_or(500).clamp(1, 1000);
    let after = q.after;
    Ok(Json(
        st.db
            .call(move |c| {
                let (events, has_more) = fetch_events(c, &auth.user_id, after, limit)?;
                let cursor = events.last().map(|e| e.cursor).unwrap_or(after);
                Ok(EventsResponse { events, cursor, has_more })
            })
            .await?,
    ))
}

// ---------------------------------------------------------------- conversations

pub async fn list_conversations(State(st): State<St>, auth: Auth) -> AppResult<Json<Vec<ConversationSummary>>> {
    Ok(Json(
        st.db
            .call(move |c| {
                user_conversation_ids(c, &auth.user_id)?
                    .into_iter()
                    .map(|id| conv_summary(c, &id, &auth.user_id))
                    .collect::<AppResult<Vec<_>>>()
            })
            .await?,
    ))
}

pub async fn get_conversation(State(st): State<St>, auth: Auth, Path(id): Path<String>) -> AppResult<Json<ConversationSummary>> {
    Ok(Json(
        st.db
            .call(move |c| {
                member_conv(c, &id, &auth.user_id)?;
                conv_summary(c, &id, &auth.user_id)
            })
            .await?,
    ))
}

/// Sends the up-to-date conversation to each member.
pub fn upsert_for_members(conn: &Connection, em: &mut Emitter, conv_id: &str) -> AppResult<()> {
    for m in member_ids(conn, conv_id)? {
        let s = conv_summary(conn, conv_id, &m)?;
        em.emit(conn, &m, EventData::ConversationUpsert { conversation: s })?;
    }
    Ok(())
}

pub async fn create_conversation(State(st): State<St>, auth: Auth, Json(req): Json<CreateConversation>) -> AppResult<Json<ConversationSummary>> {
    if !matches!(req.kind, ConversationKind::Channel | ConversationKind::GroupDm) {
        return Err(AppError::validation("only channels and group DMs can be created"));
    }
    let name = req.name.trim().to_string();
    if name.is_empty() || name.chars().count() > 80 {
        return Err(AppError::validation("name must be 1-80 characters"));
    }
    if req.topic.as_deref().map(|t| t.chars().count() > 500).unwrap_or(false) {
        return Err(AppError::validation("topic too long"));
    }
    let hub = st.hub.clone();
    Ok(Json(
        st.db
            .call(move |c| {
                if req.kind == ConversationKind::Channel && !auth.is_admin() && org::load_org_settings(c)?.channel_creation == ChannelCreation::Admins {
                    return Err(AppError::forbidden("only admins can create channels"));
                }
                // Admins may create a conversation for others without joining it; everyone else is always a member.
                let include_creator = !auth.is_admin() || req.include_creator.unwrap_or(true);
                let tx = c.transaction()?;
                let id = new_id();
                let now = now_ms();
                tx.execute(
                    "INSERT INTO conversations(id,kind,name,topic,created_by,created_ms) VALUES(?1,?2,?3,?4,?5,?6)",
                    params![id, req.kind.as_str(), name, req.topic, auth.user_id, now],
                )?;
                let mut seen = std::collections::HashSet::new();
                if include_creator {
                    seen.insert(auth.user_id.clone());
                    tx.execute("INSERT INTO members(conv_id,user_id,role,joined_ms) VALUES(?1,?2,'owner',?3)", params![id, auth.user_id, now])?;
                }
                for uid in &req.members {
                    if !seen.insert(uid.clone()) {
                        continue;
                    }
                    let u = get_user(&tx, uid)?;
                    if u.disabled || u.deleted {
                        return Err(AppError::validation("cannot add a disabled user"));
                    }
                    if u.is_bot && !auth.is_admin() {
                        return Err(AppError::validation("bots are added by an admin from the admin panel"));
                    }
                    tx.execute(
                        "INSERT INTO members(conv_id,user_id,role,joined_ms) VALUES(?1,?2,'member',?3)",
                        params![id, uid, now],
                    )?;
                }
                if req.kind == ConversationKind::GroupDm && seen.len() < 2 {
                    return Err(AppError::validation("a group DM needs at least two members"));
                }
                let mut em = Emitter::new();
                upsert_for_members(&tx, &mut em, &id)?;
                let s = conv_summary(&tx, &id, &auth.user_id).or_else(|_| {
                    // The creator may not be a member.
                    let conversation = load_conversation(&tx, &id)?;
                    Ok::<_, AppError>(ConversationSummary { conversation, last_read_seq: 0, marked_unread: false, unread: 0, mentions: 0 })
                })?;
                tx.commit()?;
                em.flush(&hub);
                Ok(s)
            })
            .await?,
    ))
}

pub async fn create_dm(State(st): State<St>, auth: Auth, Json(req): Json<CreateDm>) -> AppResult<Json<ConversationSummary>> {
    let hub = st.hub.clone();
    Ok(Json(
        st.db
            .call(move |c| {
                let other = get_user(c, &req.user_id)?;
                if other.disabled || other.deleted {
                    return Err(AppError::validation("that user is not available"));
                }
                if other.id == auth.user_id {
                    let id = crate::users::ensure_self_conversation(c, &auth.user_id)?;
                    return conv_summary(c, &id, &auth.user_id);
                }
                let mut ids = [auth.user_id.clone(), other.id.clone()];
                ids.sort();
                let key = format!("dm:{}:{}", ids[0], ids[1]);
                if let Some(id) = c
                    .query_row("SELECT id FROM conversations WHERE dm_key=?1", [&key], |r| r.get::<_, String>(0))
                    .optional()?
                {
                    return conv_summary(c, &id, &auth.user_id);
                }
                let tx = c.transaction()?;
                let id = new_id();
                let now = now_ms();
                tx.execute(
                    "INSERT INTO conversations(id,kind,created_by,created_ms,dm_key) VALUES(?1,'dm',?2,?3,?4)",
                    params![id, auth.user_id, now, key],
                )?;
                for u in &ids {
                    tx.execute(
                        "INSERT INTO members(conv_id,user_id,role,joined_ms) VALUES(?1,?2,'member',?3)",
                        params![id, u, now],
                    )?;
                }
                let mut em = Emitter::new();
                upsert_for_members(&tx, &mut em, &id)?;
                let s = conv_summary(&tx, &id, &auth.user_id)?;
                tx.commit()?;
                em.flush(&hub);
                Ok(s)
            })
            .await?,
    ))
}

pub async fn create_self(State(st): State<St>, auth: Auth) -> AppResult<Json<ConversationSummary>> {
    Ok(Json(
        st.db
            .call(move |c| {
                let id = crate::users::ensure_self_conversation(c, &auth.user_id)?;
                conv_summary(c, &id, &auth.user_id)
            })
            .await?,
    ))
}

/// Admins and the conversation's managers (conversation role `owner`) change name, topic, and archive state.
pub async fn update_conversation(
    State(st): State<St>,
    auth: Auth,
    Path(id): Path<String>,
    Json(req): Json<UpdateConversation>,
) -> AppResult<Json<ConversationSummary>> {
    let hub = st.hub.clone();
    Ok(Json(
        st.db
            .call(move |c| {
                let cr = conv_row(c, &id)?;
                let role = org::member_role(c, &id, &auth.user_id)?;
                if !auth.is_admin() {
                    match role.as_deref() {
                        None => return Err(AppError::not_found("conversation not found")),
                        Some("owner") => {}
                        Some(_) => return Err(AppError::forbidden("only channel managers and admins can change this conversation")),
                    }
                }
                if !matches!(cr.kind, ConversationKind::Channel | ConversationKind::GroupDm) {
                    return Err(AppError::validation("only channels and group DMs can be changed"));
                }
                let tx = c.transaction()?;
                if let Some(n) = &req.name {
                    let n = n.trim();
                    if n.is_empty() || n.chars().count() > 80 {
                        return Err(AppError::validation("name must be 1-80 characters"));
                    }
                    tx.execute("UPDATE conversations SET name=?2 WHERE id=?1", params![id, n])?;
                }
                if let Some(t) = &req.topic {
                    if t.chars().count() > 500 {
                        return Err(AppError::validation("topic too long"));
                    }
                    tx.execute("UPDATE conversations SET topic=?2 WHERE id=?1", params![id, t])?;
                }
                if let Some(a) = req.archived {
                    tx.execute("UPDATE conversations SET archived=?2 WHERE id=?1", params![id, a as i64])?;
                }
                let mut em = Emitter::new();
                upsert_for_members(&tx, &mut em, &id)?;
                let s = conv_summary(&tx, &id, &auth.user_id).or_else(|_| {
                    let conversation = load_conversation(&tx, &id)?;
                    Ok::<_, AppError>(ConversationSummary { conversation, last_read_seq: 0, marked_unread: false, unread: 0, mentions: 0 })
                })?;
                tx.commit()?;
                em.flush(&hub);
                Ok(s)
            })
            .await?,
    ))
}

// ---------------------------------------------------------------- leave

/// Leaves a channel or group DM (not a DM or notes to self). The other members see "X left".
pub async fn leave_conversation(State(st): State<St>, auth: Auth, Path(id): Path<String>) -> AppResult<Json<serde_json::Value>> {
    let hub = st.hub.clone();
    st.db
        .call(move |c| {
            let cr = member_conv(c, &id, &auth.user_id)?;
            if !matches!(cr.kind, ConversationKind::Channel | ConversationKind::GroupDm) {
                return Err(AppError::validation("only channels and group DMs can be left"));
            }
            crate::routes::social::remove_member_tx(c, &hub, &auth, &id, &auth.user_id)?;
            Ok(())
        })
        .await?;
    Ok(Json(serde_json::json!({"ok": true})))
}

// ---------------------------------------------------------------- organisation settings

pub async fn get_org_settings(State(st): State<St>, _auth: Auth) -> AppResult<Json<OrgSettings>> {
    Ok(Json(st.db.call(|c| org::load_org_settings(c)).await?))
}

pub async fn put_org_settings(State(st): State<St>, auth: Auth, Json(req): Json<UpdateOrgSettings>) -> AppResult<Json<OrgSettings>> {
    auth.require_admin()?;
    let hub = st.hub.clone();
    Ok(Json(
        st.db
            .call(move |c| {
                let mut cur = org::load_org_settings(c)?;
                if let Some(v) = req.channel_creation {
                    cur.channel_creation = v;
                }
                if let Some(v) = req.channel_pins {
                    cur.channel_pins = v;
                }
                let tx = c.transaction()?;
                org::save_org_settings(&tx, &cur)?;
                let mut em = Emitter::new();
                org::emit_org_settings(&tx, &mut em, &cur)?;
                tx.commit()?;
                em.flush(&hub);
                Ok(cur)
            })
            .await?,
    ))
}

// ---------------------------------------------------------------- messages

#[derive(Deserialize)]
pub struct HistoryParams {
    before: Option<i64>,
    after: Option<i64>,
    /// Unix milliseconds: return a window around the first message at or after this time.
    around_ts: Option<i64>,
    /// Message id: return a window around this message (permalinks, pins).
    around_id: Option<String>,
    limit: Option<i64>,
}

pub async fn get_history(
    State(st): State<St>,
    auth: Auth,
    Path(id): Path<String>,
    Query(q): Query<HistoryParams>,
) -> AppResult<Json<HistoryResponse>> {
    Ok(Json(
        st.db
            .call(move |c| {
                member_conv(c, &id, &auth.user_id)?;
                let around_seq = match &q.around_id {
                    Some(mid) => Some(
                        c.query_row("SELECT seq FROM messages WHERE id=?1 AND conv_id=?2", params![mid, id], |r| r.get::<_, i64>(0))
                            .optional()?
                            .ok_or_else(|| AppError::not_found("message not found"))?,
                    ),
                    None => None,
                };
                history(
                    c,
                    &id,
                    &auth.user_id,
                    &HistoryQuery { before_seq: q.before, after_seq: q.after, around_ts: q.around_ts, around_seq, limit: q.limit.unwrap_or(50) },
                )
            })
            .await?,
    ))
}

pub async fn send_message(
    State(st): State<St>,
    auth: Auth,
    Path(id): Path<String>,
    Json(req): Json<SendMessage>,
) -> AppResult<(StatusCode, Json<Message>)> {
    let (created, msg) = post_as(&st, &auth.user_id, &id, req, None).await?;
    Ok((if created { StatusCode::CREATED } else { StatusCode::OK }, Json(msg)))
}

/// Posts a message as `sender` (a user, a bot, a webhook or the scheduler). Returns whether a new
/// message was created (false for an idempotent retry).
pub async fn post_as(st: &St, sender: &str, conv_id: &str, req: SendMessage, form: Option<Form>) -> AppResult<(bool, Message)> {
    let hub = st.hub.clone();
    let (sender, conv_id) = (sender.to_string(), conv_id.to_string());
    let (created, msg) = st
        .db
        .call(move |c| insert_message(c, &hub, &sender, &conv_id, &req, form.as_ref()))
        .await?;
    if created {
        crate::push::spawn_notify(st, msg.clone());
        crate::hooks::wake();
        crate::bots::wake();
    }
    Ok((created, msg))
}

/// The one place where a message row is created.
pub fn insert_message(
    c: &mut Connection,
    hub: &crate::hub::Hub,
    sender: &str,
    id: &str,
    req: &SendMessage,
    form: Option<&Form>,
) -> AppResult<(bool, Message)> {
    let cr = member_conv(c, id, sender)?;
    if req.client_id.trim().is_empty() || req.client_id.len() > 80 {
        return Err(AppError::validation("client_id is required"));
    }
    // Idempotent retry: same sender + client id returns the stored message unchanged.
    if let Some(mid) = c
        .query_row("SELECT id FROM messages WHERE sender_id=?1 AND client_id=?2", params![sender, req.client_id], |r| r.get::<_, String>(0))
        .optional()?
    {
        return Ok((false, load_message(c, &mid)?));
    }
    if cr.archived {
        return Err(AppError::forbidden("this conversation is archived"));
    }
    let body = req.body.trim_end().to_string();
    if body.chars().count() > MAX_BODY_CHARS {
        return Err(AppError::validation("message too long"));
    }
    if body.trim().is_empty() && req.attachment_ids.is_empty() && form.is_none() {
        return Err(AppError::validation("message is empty"));
    }
    if let Some(f) = form {
        f.check_shape().map_err(AppError::validation)?;
    }
    if req.also_send_to_channel && req.thread_root.is_none() {
        return Err(AppError::validation("also_send_to_channel needs a thread_root"));
    }
    let tx = c.transaction()?;
    if let Some(r) = &req.reply_to {
        let ok: i64 = tx.query_row("SELECT COUNT(*) FROM messages WHERE id=?1 AND conv_id=?2", params![r, id], |r| r.get(0))?;
        if ok == 0 {
            return Err(AppError::validation("referenced message not found in this conversation"));
        }
    }
    // A reply inside a thread always points at the thread root.
    let thread_root = match &req.thread_root {
        Some(t) => Some(org::resolve_thread_root(&tx, id, t)?),
        None => None,
    };
    for a in &req.attachment_ids {
        let ok: i64 = tx.query_row(
            "SELECT COUNT(*) FROM attachments a WHERE a.id=?1 AND a.uploader_id=?2 AND a.removed=0 AND a.purpose='file'
             AND NOT EXISTS (SELECT 1 FROM message_attachments ma WHERE ma.attachment_id=a.id)",
            params![a, sender],
            |r| r.get(0),
        )?;
        if ok == 0 {
            return Err(AppError::validation("attachment not found or already used"));
        }
    }
    let members = member_ids(&tx, id)?;
    // M-16 / M-17 / M-18: structured mentions incl. @channel, @here and user groups.
    let mentions = org::expand_mentions(&tx, hub, id, sender, &body, &req.mentions)?;
    let seq: i64 = tx.query_row(
        "UPDATE conversations SET last_seq=last_seq+1, last_ts=?2 WHERE id=?1 RETURNING last_seq",
        params![id, now_ms()],
        |r| r.get(0),
    )?;
    let mid = new_id();
    let now = now_ms();
    let kind = if body.trim().is_empty() && form.is_none() { "file" } else { "text" };
    let form_json = form.map(serde_json::to_string).transpose()?;
    tx.execute(
        "INSERT INTO messages(id,conv_id,seq,sender_id,client_id,body,kind,ts,thread_root,reply_to,mentions,form,also_channel)
         VALUES(?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,?13)",
        params![mid, id, seq, sender, req.client_id, body, kind, now, thread_root, req.reply_to, serde_json::to_string(&mentions)?, form_json, (req.also_send_to_channel && thread_root.is_some()) as i64],
    )?;
    for a in &req.attachment_ids {
        tx.execute("INSERT INTO message_attachments(message_id, attachment_id) VALUES(?1,?2)", params![mid, a])?;
    }
    // Sending counts as reading up to this message.
    tx.execute(
        "INSERT INTO read_state(user_id,conv_id,last_read_seq,marked_unread) VALUES(?1,?2,?3,0)
         ON CONFLICT(user_id,conv_id) DO UPDATE SET last_read_seq=MAX(last_read_seq, excluded.last_read_seq), marked_unread=0",
        params![sender, id, seq],
    )?;
    let message = load_message(&tx, &mid)?;
    let mut em = Emitter::new();
    for m in &members {
        em.emit(&tx, m, EventData::MessageNew { message: message.clone() })?;
    }
    if let Some(root) = &thread_root {
        org::on_thread_reply(&tx, &mut em, root, id, sender, seq, &mentions)?;
    }
    let s = conv_summary(&tx, id, sender)?;
    em.emit(
        &tx,
        sender,
        EventData::ReadUpdate { conv_id: id.to_string(), last_read_seq: s.last_read_seq, marked_unread: false, unread: s.unread, mentions: s.mentions },
    )?;
    crate::bots::on_message(&tx, "message", &message, cr.kind)?;
    crate::hooks::on_message(&tx, "message.created", &message, cr.kind)?;
    tx.commit()?;
    em.flush(hub);
    Ok((true, message))
}

struct MsgInfo {
    conv_id: String,
    seq: i64,
    sender_id: String,
    deleted: bool,
}

fn msg_info(conn: &Connection, id: &str) -> AppResult<MsgInfo> {
    conn.query_row("SELECT conv_id,seq,sender_id,deleted FROM messages WHERE id=?1", [id], |r| {
        Ok(MsgInfo { conv_id: r.get(0)?, seq: r.get(1)?, sender_id: r.get(2)?, deleted: r.get::<_, i64>(3)? != 0 })
    })
    .optional()?
    .ok_or_else(|| AppError::not_found("message not found"))
}

/// True when someone other than the author posted after `seq`.
pub fn other_posted_after(conn: &Connection, conv_id: &str, seq: i64, author: &str) -> AppResult<bool> {
    Ok(conn
        .query_row(
            "SELECT 1 FROM messages WHERE conv_id=?1 AND seq>?2 AND sender_id!=?3 LIMIT 1",
            params![conv_id, seq, author],
            |_| Ok(()),
        )
        .optional()?
        .is_some())
}

pub async fn edit_message(
    State(st): State<St>,
    auth: Auth,
    Path(id): Path<String>,
    Json(req): Json<EditMessage>,
) -> AppResult<Json<Message>> {
    let hub = st.hub.clone();
    Ok(Json(
        st.db
            .call(move |c| {
                let info = msg_info(c, &id)?;
                let cr = member_conv(c, &info.conv_id, &auth.user_id)?;
                if info.sender_id != auth.user_id {
                    return Err(AppError::forbidden("only the author can edit a message"));
                }
                if info.deleted {
                    return Err(AppError::validation("deleted messages cannot be edited"));
                }
                if cr.archived {
                    return Err(AppError::forbidden("this conversation is archived"));
                }
                let posted = other_posted_after(c, &info.conv_id, info.seq, &info.sender_id)?;
                if !rules::edit_allowed(cr.kind, posted) {
                    return Err(AppError::new(
                        ErrorCode::EditWindowClosed,
                        "this message can no longer be edited because someone replied",
                    ));
                }
                let body = req.body.trim_end().to_string();
                if body.trim().is_empty() {
                    return Err(AppError::validation("message is empty"));
                }
                if body.chars().count() > MAX_BODY_CHARS {
                    return Err(AppError::validation("message too long"));
                }
                let tx = c.transaction()?;
                let (old_body, version): (String, i64) =
                    tx.query_row("SELECT body, version FROM messages WHERE id=?1", [&id], |r| Ok((r.get(0)?, r.get(1)?)))?;
                let now = now_ms();
                tx.execute(
                    "INSERT OR REPLACE INTO message_edits(message_id,version,body,ts) VALUES(?1,?2,?3,?4)",
                    params![id, version, old_body, now],
                )?;
                let mentions = org::expand_mentions(&tx, &hub, &info.conv_id, &auth.user_id, &body, &req.mentions)?;
                tx.execute(
                    "UPDATE messages SET body=?2, version=version+1, edited_ms=?3, mentions=?4 WHERE id=?1",
                    params![id, body, now, serde_json::to_string(&mentions)?],
                )?;
                let message = load_message(&tx, &id)?;
                let mut em = Emitter::new();
                for u in recipients(&tx, &info.conv_id, &id)? {
                    em.emit(&tx, &u, EventData::MessageEdit { message: message.clone() })?;
                }
                crate::bots::on_message(&tx, "edited_message", &message, cr.kind)?;
                crate::hooks::on_message(&tx, "message.updated", &message, cr.kind)?;
                tx.commit()?;
                em.flush(&hub);
                Ok(message)
            })
            .await?,
    ))
}

/// Deletes the message for everyone (tombstone) and garbage-collects unreferenced files.
/// Returns storage keys that must be removed from the backend after commit.
pub fn tombstone_message(conn: &Connection, em: &mut Emitter, id: &str) -> AppResult<Vec<String>> {
    let info = msg_info(conn, id)?;
    if info.deleted {
        return Ok(vec![]);
    }
    conn.execute("UPDATE messages SET deleted=1, body='' WHERE id=?1", [id])?;
    let keys = release_attachments(conn, id)?;
    conn.execute("DELETE FROM reactions WHERE message_id=?1", [id])?;
    let message = load_message(conn, id)?;
    for u in recipients(conn, &info.conv_id, id)? {
        em.emit(conn, &u, EventData::MessageDelete { message: message.clone() })?;
    }
    let kind = conv_row(conn, &info.conv_id)?.kind;
    crate::hooks::on_message(conn, "message.deleted", &message, kind)?;
    Ok(keys)
}

/// Drops the message->attachment references and returns keys of attachments that have no
/// references left and are not in a backup (their objects can be deleted).
pub fn release_attachments(conn: &Connection, message_id: &str) -> AppResult<Vec<String>> {
    let mut st = conn.prepare(
        "SELECT a.id, a.storage_key, a.backup_id FROM message_attachments ma JOIN attachments a ON a.id=ma.attachment_id WHERE ma.message_id=?1",
    )?;
    let atts = st
        .query_map([message_id], |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?, r.get::<_, Option<String>>(2)?)))?
        .collect::<Result<Vec<_>, _>>()?;
    conn.execute("DELETE FROM message_attachments WHERE message_id=?1", [message_id])?;
    let mut keys = vec![];
    for (aid, key, backup) in atts {
        let refs: i64 =
            conn.query_row("SELECT COUNT(*) FROM message_attachments WHERE attachment_id=?1", [&aid], |r| r.get(0))?;
        if refs == 0 && backup.is_none() {
            conn.execute("DELETE FROM attachments WHERE id=?1", [&aid])?;
            keys.push(key);
        }
    }
    Ok(keys)
}

pub async fn delete_message(State(st): State<St>, auth: Auth, Path(id): Path<String>) -> AppResult<Json<Message>> {
    let hub = st.hub.clone();
    let (msg, keys) = st
        .db
        .call(move |c| {
            let info = msg_info(c, &id)?;
            member_conv(c, &info.conv_id, &auth.user_id)?;
            if info.sender_id != auth.user_id {
                return Err(AppError::forbidden("you can only delete your own messages"));
            }
            let tx = c.transaction()?;
            let mut em = Emitter::new();
            let keys = tombstone_message(&tx, &mut em, &id)?;
            let message = load_message(&tx, &id)?;
            tx.commit()?;
            em.flush(&hub);
            Ok((message, keys))
        })
        .await?;
    let storage = st.storage();
    for k in keys {
        if let Err(e) = storage.delete(&k).await {
            tracing::warn!("could not delete object {k}: {e}");
        }
    }
    Ok(Json(msg))
}

/// "Delete for me": hides any message of a conversation the caller is in, for the caller only, on every device.
/// Other people keep seeing it. Idempotent. The caller's devices get `message_hide {conv_id, message_ids}`, followed by
/// `read_update` (counts) and, for threads, `thread_update` (a hidden thread root removes the thread from the list).
pub async fn hide_message(State(st): State<St>, auth: Auth, Path(id): Path<String>) -> AppResult<Json<HideMessageResponse>> {
    let hub = st.hub.clone();
    Ok(Json(
        st.db
            .call(move |c| {
                let info = msg_info(c, &id)?;
                member_conv(c, &info.conv_id, &auth.user_id)?;
                let tx = c.transaction()?;
                let added = tx.execute("INSERT OR IGNORE INTO message_hidden(user_id,message_id) VALUES(?1,?2)", params![auth.user_id, id])?;
                if added > 0 {
                    let mut em = Emitter::new();
                    em.emit(&tx, &auth.user_id, EventData::MessageHide { conv_id: info.conv_id.clone(), message_ids: vec![id.clone()] })?;
                    let s = conv_summary(&tx, &info.conv_id, &auth.user_id)?;
                    em.emit(
                        &tx,
                        &auth.user_id,
                        EventData::ReadUpdate { conv_id: info.conv_id.clone(), last_read_seq: s.last_read_seq, marked_unread: s.marked_unread, unread: s.unread, mentions: s.mentions },
                    )?;
                    let thread_root: Option<String> = tx.query_row("SELECT thread_root FROM messages WHERE id=?1", [&id], |r| r.get(0))?;
                    let followed = |root: &str| -> AppResult<bool> {
                        Ok(tx.query_row("SELECT 1 FROM thread_follows WHERE user_id=?1 AND thread_root=?2 AND following=1", params![auth.user_id, root], |_| Ok(())).optional()?.is_some())
                    };
                    match thread_root {
                        Some(root) if followed(&root)? => org::emit_thread(&tx, &mut em, &auth.user_id, &root)?,
                        Some(_) => {}
                        None if followed(&id)? => {
                            let mut t = org::thread_summary(&tx, &id, &auth.user_id)?;
                            t.following = false;
                            em.emit(&tx, &auth.user_id, EventData::ThreadUpdate { thread: t })?;
                        }
                        None => {}
                    }
                    tx.commit()?;
                    em.flush(&hub);
                }
                Ok(HideMessageResponse { message_id: id, conv_id: info.conv_id })
            })
            .await?,
    ))
}

fn emit_reactions(conn: &Connection, em: &mut Emitter, conv_id: &str, id: &str) -> AppResult<()> {
    let message = load_message(conn, id)?;
    for u in recipients(conn, conv_id, id)? {
        em.emit(
            conn,
            &u,
            EventData::ReactionUpdate { conv_id: conv_id.to_string(), message_id: id.to_string(), reactions: message.reactions.clone() },
        )?;
    }
    Ok(())
}

pub async fn add_reaction(State(st): State<St>, auth: Auth, Path((id, emoji)): Path<(String, String)>) -> AppResult<Json<Message>> {
    react(st, auth, id, emoji, true).await
}

pub async fn remove_reaction(State(st): State<St>, auth: Auth, Path((id, emoji)): Path<(String, String)>) -> AppResult<Json<Message>> {
    react(st, auth, id, emoji, false).await
}

async fn react(st: St, auth: Auth, id: String, emoji: String, add: bool) -> AppResult<Json<Message>> {
    if emoji.trim().is_empty() || emoji.len() > 64 {
        return Err(AppError::validation("invalid emoji"));
    }
    let hub = st.hub.clone();
    Ok(Json(
        st.db
            .call(move |c| {
                let info = msg_info(c, &id)?;
                let cr = member_conv(c, &info.conv_id, &auth.user_id)?;
                if info.deleted {
                    return Err(AppError::validation("message was deleted"));
                }
                if cr.archived {
                    return Err(AppError::forbidden("this conversation is archived"));
                }
                let tx = c.transaction()?;
                let changed = if add {
                    tx.execute(
                        "INSERT OR IGNORE INTO reactions(message_id,user_id,emoji,ts) VALUES(?1,?2,?3,?4)",
                        params![id, auth.user_id, emoji, now_ms()],
                    )?
                } else {
                    tx.execute(
                        "DELETE FROM reactions WHERE message_id=?1 AND user_id=?2 AND emoji=?3",
                        params![id, auth.user_id, emoji],
                    )?
                };
                let mut em = Emitter::new();
                if changed > 0 {
                    emit_reactions(&tx, &mut em, &info.conv_id, &id)?;
                }
                let message = load_message(&tx, &id)?;
                tx.commit()?;
                em.flush(&hub);
                Ok(message)
            })
            .await?,
    ))
}

pub async fn mark_read(State(st): State<St>, auth: Auth, Path(id): Path<String>, Json(req): Json<MarkReadRequest>) -> AppResult<Json<ConversationSummary>> {
    set_read(st, auth, id, Some(req.seq)).await
}

pub async fn mark_unread(State(st): State<St>, auth: Auth, Path(id): Path<String>) -> AppResult<Json<ConversationSummary>> {
    set_read(st, auth, id, None).await
}

async fn set_read(st: St, auth: Auth, id: String, seq: Option<i64>) -> AppResult<Json<ConversationSummary>> {
    let hub = st.hub.clone();
    Ok(Json(
        st.db
            .call(move |c| {
                member_conv(c, &id, &auth.user_id)?;
                let tx = c.transaction()?;
                let last: i64 = tx.query_row("SELECT last_seq FROM conversations WHERE id=?1", [&id], |r| r.get(0))?;
                let prev_read: i64 = tx.query_row("SELECT last_read_seq FROM read_state WHERE user_id=?1 AND conv_id=?2", params![auth.user_id, id], |r| r.get(0)).optional()?.unwrap_or(0);
                match seq {
                    Some(s) => {
                        tx.execute(
                            "INSERT INTO read_state(user_id,conv_id,last_read_seq,marked_unread) VALUES(?1,?2,?3,0)
                             ON CONFLICT(user_id,conv_id) DO UPDATE SET last_read_seq=MAX(last_read_seq, excluded.last_read_seq), marked_unread=0",
                            params![auth.user_id, id, s.clamp(0, last)],
                        )?;
                    }
                    None => {
                        tx.execute(
                            "INSERT INTO read_state(user_id,conv_id,last_read_seq,marked_unread) VALUES(?1,?2,0,1)
                             ON CONFLICT(user_id,conv_id) DO UPDATE SET marked_unread=1",
                            params![auth.user_id, id],
                        )?;
                    }
                }
                let s = conv_summary(&tx, &id, &auth.user_id)?;
                let mut em = Emitter::new();
                // Read receipts: tell the other members of a DM or group DM how far this person has read.
                if seq.is_some() && s.last_read_seq > prev_read && matches!(s.conversation.kind, ConversationKind::Dm | ConversationKind::GroupDm) && get_user(&tx, &auth.user_id)?.read_receipts {
                    for m in &s.conversation.members {
                        if m.user_id != auth.user_id {
                            em.emit(&tx, &m.user_id, EventData::ReadReceipt { conv_id: id.clone(), user_id: auth.user_id.clone(), last_read_seq: s.last_read_seq })?;
                        }
                    }
                }
                em.emit(
                    &tx,
                    &auth.user_id,
                    EventData::ReadUpdate {
                        conv_id: id.clone(),
                        last_read_seq: s.last_read_seq,
                        marked_unread: s.marked_unread,
                        unread: s.unread,
                        mentions: s.mentions,
                    },
                )?;
                tx.commit()?;
                em.flush(&hub);
                Ok(s)
            })
            .await?,
    ))
}

