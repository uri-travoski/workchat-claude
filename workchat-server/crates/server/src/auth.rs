use crate::db::{new_id, now_ms};
use crate::error::{AppError, AppResult};
use crate::state::St;
use argon2::password_hash::{PasswordHash, PasswordHasher, PasswordVerifier, SaltString};
use argon2::Argon2;
use axum::extract::FromRequestParts;
use axum::http::request::Parts;
use base64::Engine;
use proto::{ErrorCode, Role, TokenPair};
use rand::RngCore;
use rusqlite::{params, Connection};
use sha2::{Digest, Sha256};

pub fn sha256_hex(s: &str) -> String {
    hex::encode(Sha256::digest(s.as_bytes()))
}

pub fn random_token(prefix: &str, bytes: usize) -> String {
    let mut b = vec![0u8; bytes];
    rand::thread_rng().fill_bytes(&mut b);
    format!("{prefix}{}", base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(b))
}

pub fn hash_password(pw: &str) -> AppResult<String> {
    let mut salt_bytes = [0u8; 16];
    rand::thread_rng().fill_bytes(&mut salt_bytes);
    let salt = SaltString::encode_b64(&salt_bytes).map_err(|e| AppError::internal(e.to_string()))?;
    Argon2::default()
        .hash_password(pw.as_bytes(), &salt)
        .map(|h| h.to_string())
        .map_err(|e| AppError::internal(e.to_string()))
}

pub fn verify_password(pw: &str, hash: &str) -> bool {
    match PasswordHash::new(hash) {
        Ok(h) => Argon2::default().verify_password(pw.as_bytes(), &h).is_ok(),
        Err(_) => false,
    }
}

pub fn validate_password(pw: &str) -> AppResult<()> {
    if pw.chars().count() < 8 {
        return Err(AppError::validation("password must be at least 8 characters"));
    }
    if pw.len() > 256 {
        return Err(AppError::validation("password too long"));
    }
    Ok(())
}

/// Creates a device session and returns the token pair.
pub fn create_session(
    conn: &Connection,
    st: &St,
    user_id: &str,
    device_name: &str,
    client_version: &str,
    ip: &str,
) -> AppResult<(String, TokenPair)> {
    let sid = new_id();
    let access = random_token("at_", 32);
    let refresh = random_token("rt_", 32);
    let now = now_ms();
    conn.execute(
        "INSERT INTO sessions(id,user_id,device_name,client_version,ip,access_hash,access_exp_ms,refresh_hash,refresh_exp_ms,created_ms,last_seen_ms)
         VALUES(?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?10)",
        params![
            sid,
            user_id,
            clip(device_name, 80),
            clip(client_version, 40),
            clip(ip, 64),
            sha256_hex(&access),
            now + st.cfg.access_ttl_secs * 1000,
            sha256_hex(&refresh),
            now + st.cfg.refresh_ttl_secs * 1000,
            now
        ],
    )?;
    Ok((sid, TokenPair { access_token: access, refresh_token: refresh, expires_in: st.cfg.access_ttl_secs }))
}

pub fn clip(s: &str, n: usize) -> String {
    s.chars().take(n).collect()
}

#[derive(Clone, Debug)]
pub struct Auth {
    pub user_id: String,
    pub session_id: String,
    pub role: Role,
}

impl Auth {
    pub fn require_admin(&self) -> AppResult<()> {
        if self.role.is_admin() {
            Ok(())
        } else {
            Err(AppError::forbidden("admin only"))
        }
    }
    pub fn is_admin(&self) -> bool {
        self.role.is_admin()
    }
}

pub fn revoke_code(reason: &str) -> ErrorCode {
    match reason {
        "account_disabled" => ErrorCode::AccountDisabled,
        "password_reset" => ErrorCode::PasswordReset,
        "history_cleared" => ErrorCode::HistoryCleared,
        _ => ErrorCode::SessionRevoked,
    }
}

pub fn client_ip(parts: &Parts) -> String {
    for h in ["cf-connecting-ip", "x-real-ip", "x-forwarded-for"] {
        if let Some(v) = parts.headers.get(h).and_then(|v| v.to_str().ok()) {
            if let Some(first) = v.split(',').next() {
                let t = first.trim();
                if !t.is_empty() {
                    return t.to_string();
                }
            }
        }
    }
    parts
        .extensions
        .get::<axum::extract::ConnectInfo<std::net::SocketAddr>>()
        .map(|c| c.0.ip().to_string())
        .unwrap_or_default()
}

pub fn check_client_version(parts: &Parts) -> AppResult<()> {
    if let Some(v) = parts.headers.get("x-client-version").and_then(|v| v.to_str().ok()) {
        if proto::rules::version_lt(v, proto::MIN_CLIENT_VERSION) {
            return Err(AppError::new(
                ErrorCode::ClientTooOld,
                format!("client {v} is older than the minimum {}", proto::MIN_CLIENT_VERSION),
            ));
        }
    }
    Ok(())
}

/// Looks up an access token. Used by the extractor and the WebSocket hello.
pub fn authenticate(conn: &Connection, token: &str) -> AppResult<Auth> {
    let now = now_ms();
    let row = conn.query_row(
        "SELECT s.id, s.user_id, s.access_exp_ms, s.revoked, s.revoke_reason, s.last_seen_ms, u.role, u.disabled, u.deleted
         FROM sessions s JOIN users u ON u.id = s.user_id WHERE s.access_hash = ?1",
        [sha256_hex(token)],
        |r| {
            Ok((
                r.get::<_, String>(0)?,
                r.get::<_, String>(1)?,
                r.get::<_, i64>(2)?,
                r.get::<_, i64>(3)? != 0,
                r.get::<_, Option<String>>(4)?,
                r.get::<_, i64>(5)?,
                r.get::<_, String>(6)?,
                r.get::<_, i64>(7)? != 0,
                r.get::<_, i64>(8)? != 0,
            ))
        },
    );
    let (sid, uid, exp, revoked, reason, last_seen, role, disabled, deleted) = match row {
        Ok(r) => r,
        Err(rusqlite::Error::QueryReturnedNoRows) => {
            return Err(AppError::new(ErrorCode::Unauthorized, "invalid token"))
        }
        Err(e) => return Err(e.into()),
    };
    if revoked {
        let code = revoke_code(reason.as_deref().unwrap_or(""));
        return Err(AppError::new(code, "session ended"));
    }
    if disabled || deleted {
        return Err(AppError::new(ErrorCode::AccountDisabled, "account disabled"));
    }
    if exp < now {
        return Err(AppError::new(ErrorCode::TokenExpired, "access token expired"));
    }
    if now - last_seen > 60_000 {
        conn.execute("UPDATE sessions SET last_seen_ms=?1 WHERE id=?2", params![now, sid])?;
        conn.execute("UPDATE users SET last_active_ms=?1 WHERE id=?2", params![now, uid])?;
    }
    Ok(Auth { user_id: uid, session_id: sid, role: Role::parse(&role).unwrap_or(Role::Member) })
}

impl FromRequestParts<St> for Auth {
    type Rejection = AppError;

    async fn from_request_parts(parts: &mut Parts, st: &St) -> Result<Self, Self::Rejection> {
        check_client_version(parts)?;
        let token = parts
            .headers
            .get("authorization")
            .and_then(|v| v.to_str().ok())
            .and_then(|v| v.strip_prefix("Bearer "))
            .map(str::trim)
            .map(str::to_string)
            .ok_or_else(|| AppError::new(ErrorCode::Unauthorized, "missing bearer token"))?;
        let auth = st.db.call(move |c| authenticate(c, &token)).await?;
        if !st.limiter.check(&auth.user_id) {
            return Err(AppError::new(ErrorCode::RateLimited, "too many requests"));
        }
        Ok(auth)
    }
}

/// Extractor for admin-only routes.
pub struct Admin(pub Auth);

impl FromRequestParts<St> for Admin {
    type Rejection = AppError;
    async fn from_request_parts(parts: &mut Parts, st: &St) -> Result<Self, Self::Rejection> {
        let a = Auth::from_request_parts(parts, st).await?;
        a.require_admin()?;
        Ok(Admin(a))
    }
}

/// Used by the one-time link flow: 24 chars, URL safe.
pub fn make_link_token() -> String {
    random_token("", 24)
}
