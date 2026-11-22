//! Endpoints: membership management for owners/admins, Save for later + labels, Activity feed.

use crate::auth::Auth;
use crate::db::{new_id, now_ms};
use crate::emit::Emitter;
use crate::error::{AppError, AppResult};
use crate::queries::*;
use crate::routes::chat::upsert_for_members;
use crate::saved;
use crate::state::St;
use axum::extract::{Path, Query, State};
use axum::Json;
use proto::*;
use rusqlite::{params, Connection, OptionalExtension};
use serde::Deserialize;

type J<T> = AppResult<Json<T>>;

// ------------------------------------------------------------------ membership (owners and admins)

/// Server owners and admins, and conversation owners, may manage members. Other members get `forbidden`; people outside the
/// conversation (who must not learn it exists) get `not_found`.
fn require_manager(c: &Connection, auth: &Auth, conv_id: &str) -> AppResult<ConvRow> {
    let cr = conv_row(c, conv_id)?;
    let member_role: Option<String> = c.query_row("SELECT role FROM members WHERE conv_id=?1 AND user_id=?2", params![conv_id, auth.user_id], |r| r.get(0)).optional()?;
    let is_owner = member_role.as_deref() == Some("owner");
    if !auth.is_admin() && !is_owner {
        return Err(if member_role.is_some() { AppError::forbidden("only owners and admins can manage members") } else { AppError::not_found("conversation not found") });
    }
    if !matches!(cr.kind, ConversationKind::Channel | ConversationKind::GroupDm) {
        return Err(AppError::validation("members can only be managed on channels and group DMs"));
    }
    Ok(cr)
}

/// Appends a system message ("Alice added Bob") and tells every member.
pub fn post_system(tx: &Connection, em: &mut Emitter, conv_id: &str, actor: &str, body: &str) -> AppResult<Message> {
    let seq: i64 = tx.query_row("UPDATE conversations SET last_seq=last_seq+1, last_ts=?2 WHERE id=?1 RETURNING last_seq", params![conv_id, now_ms()], |r| r.get(0))?;
    let mid = new_id();
    tx.execute(
        "INSERT INTO messages(id,conv_id,seq,sender_id,client_id,body,kind,ts,mentions) VALUES(?1,?2,?3,?4,NULL,?5,'system',?6,'[]')",
        params![mid, conv_id, seq, actor, body, now_ms()],
    )?;
    let message = load_message(tx, &mid)?;
    for m in member_ids(tx, conv_id)? {
        em.emit(tx, &m, EventData::MessageNew { message: message.clone() })?;
    }
    Ok(message)
}

fn names(c: &Connection, ids: &[String]) -> AppResult<String> {
    let mut v = vec![];
    for id in ids {
        v.push(get_user(c, id)?.display_name);
    }
    Ok(match v.len() {
        0 => String::new(),
        1 => v[0].clone(),
        2 => format!("{} and {}", v[0], v[1]),
        n => format!("{} and {} others", v[..2].join(", "), n - 2),
    })
}

/// Adds people to a channel or group DM. Returns the conversation (as the caller sees it).
pub fn add_members_tx(c: &mut Connection, hub: &crate::hub::Hub, auth: &Auth, conv_id: &str, user_ids: &[String]) -> AppResult<Conversation> {
    require_manager(c, auth, conv_id)?;
    let tx = c.transaction()?;
    let mut added = vec![];
    for uid in user_ids {
        let u = get_user(&tx, uid)?;
        if u.disabled || u.deleted {
            return Err(AppError::validation("cannot add a disabled user"));
        }
        if u.is_bot {
            return Err(AppError::validation("bots are added by an admin from the admin panel"));
        }
        let n = tx.execute("INSERT OR IGNORE INTO members(conv_id,user_id,role,joined_ms) VALUES(?1,?2,'member',?3)", params![conv_id, uid, now_ms()])?;
        if n > 0 {
            added.push(uid.clone());
        }
    }
    let mut em = Emitter::new();
    if !added.is_empty() {
        let actor = get_user(&tx, &auth.user_id)?.display_name;
        let body = format!("{actor} added {}", names(&tx, &added)?);
        post_system(&tx, &mut em, conv_id, &auth.user_id, &body)?;
        upsert_for_members(&tx, &mut em, conv_id)?;
    }
    tx.commit()?;
    em.flush(hub);
    load_conversation(c, conv_id)
}

/// Removes a member. Managers and admins remove anyone; any member may remove themselves (leave).
pub fn remove_member_tx(c: &mut Connection, hub: &crate::hub::Hub, auth: &Auth, conv_id: &str, uid: &str) -> AppResult<Conversation> {
    if uid == auth.user_id && is_member(c, conv_id, uid)? {
        if !matches!(conv_row(c, conv_id)?.kind, ConversationKind::Channel | ConversationKind::GroupDm) {
            return Err(AppError::validation("members can only be managed on channels and group DMs"));
        }
    } else {
        require_manager(c, auth, conv_id)?;
    }
    let role: Option<String> = c.query_row("SELECT role FROM members WHERE conv_id=?1 AND user_id=?2", params![conv_id, uid], |r| r.get(0)).optional()?;
    if role.as_deref() == Some("owner") && !auth.is_admin() && uid != auth.user_id {
        return Err(AppError::forbidden("only admins can remove an owner"));
    }
    let tx = c.transaction()?;
    let n = tx.execute("DELETE FROM members WHERE conv_id=?1 AND user_id=?2", params![conv_id, uid])?;
    let mut em = Emitter::new();
    if n > 0 {
        em.emit(&tx, uid, EventData::ConversationRemove { conv_id: conv_id.to_string() })?;
        let actor = get_user(&tx, &auth.user_id)?.display_name;
        let name = get_user(&tx, uid)?.display_name;
        let body = if uid == auth.user_id { format!("{actor} left") } else { format!("{actor} removed {name}") };
        post_system(&tx, &mut em, conv_id, &auth.user_id, &body)?;
        upsert_for_members(&tx, &mut em, conv_id)?;
    }
    tx.commit()?;
    em.flush(hub);
    load_conversation(c, conv_id)
}

pub async fn add_members(State(st): State<St>, auth: Auth, Path(id): Path<String>, Json(req): Json<MembersRequest>) -> J<Conversation> {
    let hub = st.hub.clone();
    Ok(Json(st.db.call(move |c| add_members_tx(c, &hub, &auth, &id, &req.user_ids)).await?))
}

pub async fn remove_member(State(st): State<St>, auth: Auth, Path((id, uid)): Path<(String, String)>) -> J<Conversation> {
    let hub = st.hub.clone();
    Ok(Json(st.db.call(move |c| remove_member_tx(c, &hub, &auth, &id, &uid)).await?))
}

// ------------------------------------------------------------------ saved items and labels

pub async fn list_saved(State(st): State<St>, auth: Auth) -> J<Vec<SavedItem>> {
    Ok(Json(st.db.call(move |c| saved::load_saved(c, &auth.user_id)).await?))
}

/// The saved items with their full message and conversation (same visibility rules as `GET /saved`), newest saved first.
pub async fn list_saved_messages(State(st): State<St>, auth: Auth) -> J<Vec<SavedMessage>> {
    Ok(Json(
        st.db
            .call(move |c| {
                let mut out = vec![];
                for item in saved::load_saved(c, &auth.user_id)? {
                    let message = load_message(c, &item.message_id)?;
                    let conversation = conv_summary(c, &item.conv_id, &auth.user_id)?;
                    out.push(SavedMessage { item, message, conversation });
                }
                Ok(out)
            })
            .await?,
    ))
}

pub async fn save_message(State(st): State<St>, auth: Auth, Path(id): Path<String>, body: Option<Json<SaveRequest>>) -> J<SavedItem> {
    let hub = st.hub.clone();
    let req = body.map(|b| b.0).unwrap_or_default();
    Ok(Json(st.db.call(move |c| saved::save(c, &hub, &auth.user_id, &id, req.label_ids.as_deref())).await?))
}

pub async fn unsave_message(State(st): State<St>, auth: Auth, Path(id): Path<String>) -> J<serde_json::Value> {
    let hub = st.hub.clone();
    st.db.call(move |c| saved::unsave(c, &hub, &auth.user_id, &id)).await?;
    Ok(Json(serde_json::json!({"ok": true})))
}

pub async fn set_saved_labels(State(st): State<St>, auth: Auth, Path(id): Path<String>, Json(req): Json<SavedLabelsRequest>) -> J<SavedItem> {
    let hub = st.hub.clone();
    Ok(Json(st.db.call(move |c| saved::set_item_labels(c, &hub, &auth.user_id, &id, &req.label_ids)).await?))
}

pub async fn list_labels(State(st): State<St>, auth: Auth) -> J<Vec<Label>> {
    Ok(Json(st.db.call(move |c| saved::load_labels(c, &auth.user_id)).await?))
}

pub async fn create_label(State(st): State<St>, auth: Auth, Json(req): Json<LabelRequest>) -> J<Vec<Label>> {
    let hub = st.hub.clone();
    Ok(Json(st.db.call(move |c| saved::create_label(c, &hub, &auth.user_id, &req)).await?))
}

pub async fn update_label(State(st): State<St>, auth: Auth, Path(id): Path<String>, Json(req): Json<LabelRequest>) -> J<Vec<Label>> {
    let hub = st.hub.clone();
    Ok(Json(st.db.call(move |c| saved::update_label(c, &hub, &auth.user_id, &id, &req)).await?))
}

pub async fn delete_label(State(st): State<St>, auth: Auth, Path(id): Path<String>) -> J<Vec<Label>> {
    let hub = st.hub.clone();
    Ok(Json(st.db.call(move |c| saved::delete_label(c, &hub, &auth.user_id, &id)).await?))
}

// ------------------------------------------------------------------ activity

#[derive(Deserialize)]
pub struct ActivityQuery {
    limit: Option<i64>,
}

pub async fn activity(State(st): State<St>, auth: Auth, Query(q): Query<ActivityQuery>) -> J<ActivityFeed> {
    let limit = q.limit.unwrap_or(100).clamp(1, 300);
    Ok(Json(st.db.call(move |c| saved::load_activity(c, &auth.user_id, limit)).await?))
}

#[derive(Deserialize, Default)]
pub struct ReadBody {
    #[serde(default)]
    up_to_ms: Option<i64>,
}

pub async fn read_activity(State(st): State<St>, auth: Auth, body: Option<Json<ReadBody>>) -> J<ActivityFeed> {
    let hub = st.hub.clone();
    let b = body.map(|b| b.0).unwrap_or_default();
    Ok(Json(
        st.db
            .call(move |c| {
                saved::mark_activity_read(c, &hub, &auth.user_id, b.up_to_ms)?;
                saved::load_activity(c, &auth.user_id, 100)
            })
            .await?,
    ))
}
