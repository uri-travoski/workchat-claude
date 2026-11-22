//! User lifecycle shared by setup, the bootstrap admin and the admin API.

use crate::auth::hash_password;
use crate::db::{new_id, now_ms};
use crate::emit::Emitter;
use crate::error::{AppError, AppResult};
use crate::hub::Hub;
use crate::queries::*;
use proto::*;
use rusqlite::{params, Connection};

pub async fn hash_pw_async(pw: String) -> AppResult<String> {
    tokio::task::spawn_blocking(move || hash_password(&pw))
        .await
        .map_err(|e| AppError::internal(e.to_string()))?
}

pub struct NewUser {
    pub username: String,
    pub display_name: String,
    pub password_hash: String,
    pub password_set: bool,
    pub role: Role,
    pub email: Option<String>,
    pub title: Option<String>,
    pub phone: Option<String>,
}

/// Cleans a free-text profile field: trimmed, empty -> None, length limit.
pub fn profile_field(v: &Option<String>, max: usize, what: &str) -> AppResult<Option<String>> {
    match v.as_deref().map(str::trim) {
        None | Some("") => Ok(None),
        Some(t) if t.chars().count() > max || t.chars().any(|c| c.is_control()) => Err(AppError::validation(format!("{what} must be at most {max} characters"))),
        Some(t) => Ok(Some(t.to_string())),
    }
}

/// Creates the account and its notes-to-self conversation and announces the user to everyone.
pub fn create_user(conn: &Connection, hub: &Hub, nu: NewUser, em: &mut Emitter) -> AppResult<UserRow> {
    if !rules::valid_username(&nu.username) {
        return Err(AppError::validation("username must be 2-32 characters: letters, digits, . _ -"));
    }
    let display = nu.display_name.trim();
    if display.is_empty() || display.chars().count() > 80 {
        return Err(AppError::validation("display name must be 1-80 characters"));
    }
    let exists: i64 =
        conn.query_row("SELECT COUNT(*) FROM users WHERE username=?1", [&nu.username], |r| r.get(0))?;
    if exists > 0 {
        return Err(AppError::conflict("username already taken"));
    }
    let title = profile_field(&nu.title, 80, "job title")?;
    let phone = profile_field(&nu.phone, 40, "phone number")?;
    let id = new_id();
    let now = now_ms();
    conn.execute(
        "INSERT INTO users(id,username,display_name,email,password_hash,password_set,role,created_ms,title,phone) VALUES(?1,?2,?3,?4,?5,?6,?7,?8,?9,?10)",
        params![id, nu.username, display, nu.email, nu.password_hash, nu.password_set as i64, nu.role.as_str(), now, title, phone],
    )?;
    ensure_self_conversation(conn, &id)?;
    let row = get_user(conn, &id)?;
    let user = to_user(&row, hub);
    for uid in user_ids_all_active(conn)? {
        em.emit(conn, &uid, EventData::UserUpsert { user: user.clone() })?;
    }
    Ok(row)
}

pub fn ensure_self_conversation(conn: &Connection, user_id: &str) -> AppResult<String> {
    let key = format!("self:{user_id}");
    if let Ok(id) = conn.query_row("SELECT id FROM conversations WHERE dm_key=?1", [&key], |r| r.get::<_, String>(0)) {
        return Ok(id);
    }
    let id = new_id();
    let now = now_ms();
    conn.execute(
        "INSERT INTO conversations(id,kind,name,created_by,created_ms,dm_key) VALUES(?1,'self',NULL,?2,?3,?4)",
        params![id, user_id, now, key],
    )?;
    conn.execute(
        "INSERT INTO members(conv_id,user_id,role,joined_ms) VALUES(?1,?2,'owner',?3)",
        params![id, user_id, now],
    )?;
    Ok(id)
}

/// Revokes every session of the user (optionally except one) and closes live connections.
pub fn revoke_sessions(conn: &Connection, hub: &Hub, user_id: &str, reason: &str, except: Option<&str>) -> AppResult<i64> {
    let mut st = conn.prepare("SELECT id FROM sessions WHERE user_id=?1 AND revoked=0")?;
    let ids = st.query_map([user_id], |r| r.get::<_, String>(0))?.collect::<Result<Vec<_>, _>>()?;
    let mut n = 0;
    for sid in ids {
        if Some(sid.as_str()) == except {
            continue;
        }
        conn.execute("UPDATE sessions SET revoked=1, revoke_reason=?2 WHERE id=?1", params![sid, reason])?;
        let code = crate::auth::revoke_code(reason);
        hub.kick_session(&sid, code, proto::close::SESSION_ENDED, reason);
        n += 1;
    }
    Ok(n)
}

pub fn admin_user(conn: &Connection, hub: &Hub, row: &UserRow) -> AppResult<AdminUser> {
    let session_count: i64 = conn.query_row(
        "SELECT COUNT(*) FROM sessions WHERE user_id=?1 AND revoked=0",
        [&row.id],
        |r| r.get(0),
    )?;
    let mut user = to_user(row, hub);
    user.last_active_ms = row.last_active_ms;
    Ok(AdminUser {
        user,
        email: row.email.clone(),
        created_ms: row.created_ms,
        privacy_controls_enabled: row.privacy_controls_enabled,
        hide_last_active: row.hide_last_active,
        password_set: row.password_set,
        deleted: row.deleted,
        session_count,
        last_active_real_ms: row.last_active_ms,
    })
}

/// Broadcasts the current public view of a user to every active user.
pub fn announce_user(conn: &Connection, hub: &Hub, user_id: &str, em: &mut Emitter) -> AppResult<()> {
    let row = get_user(conn, user_id)?;
    let user = to_user(&row, hub);
    for uid in user_ids_all_active(conn)? {
        em.emit(conn, &uid, EventData::UserUpsert { user: user.clone() })?;
    }
    if row.disabled || row.deleted {
        em.emit(conn, &row.id, EventData::UserUpsert { user })?;
    }
    Ok(())
}
