//! Meta, setup, login, refresh, profile and own sessions.

use crate::auth::*;
use crate::db::{now_ms, set_setting};
use crate::emit::Emitter;
use crate::error::{AppError, AppResult};
use crate::queries::*;
use crate::state::St;
use crate::storage::build_storage;
use crate::users::*;
use axum::extract::{Path, State};
use axum::http::request::Parts;
use axum::Json;
use proto::*;
use rusqlite::{params, OptionalExtension};

pub async fn health() -> &'static str {
    "ok"
}

pub async fn meta(State(st): State<St>) -> AppResult<Json<ServerMeta>> {
    let (count, push, ai): (i64, PushSettings, bool) = st
        .db
        .call(|c| Ok((c.query_row("SELECT COUNT(*) FROM users", [], |r| r.get(0))?, crate::push::load_settings(c)?, crate::ai::load(c)?.ready())))
        .await?;
    Ok(Json(ServerMeta {
        name: st.cfg.server_name.clone(),
        api_version: API_VERSION,
        min_client_version: MIN_CLIENT_VERSION.into(),
        server_version: VERSION.into(),
        setup_required: count == 0,
        max_upload_bytes: st.cfg.max_upload_bytes,
        ai_search_enabled: ai,
        ntfy_url: crate::push::effective_ntfy(&st, &push),
        fcm_enabled: push.fcm_service_account.is_some(),
    }))
}

/// First-run setup: creates the owner account and chooses the storage backend.
pub async fn setup(State(st): State<St>, parts: Parts, Json(req): Json<SetupRequest>) -> AppResult<Json<LoginResponse>> {
    validate_password(&req.password)?;
    let existing: i64 = st.db.call(|c| Ok(c.query_row("SELECT COUNT(*) FROM users", [], |r| r.get(0))?)).await?;
    if existing > 0 {
        return Err(AppError::new(ErrorCode::SetupDone, "setup already completed"));
    }
    if let Some(cfg) = &req.storage {
        let storage = build_storage(cfg, &st.cfg.data_dir)?;
        storage.check().await?;
        crate::admin::set_storage(&st, cfg.clone(), storage).await?;
    }
    let hash = hash_pw_async(req.password.clone()).await?;
    let ip = client_ip(&parts);
    let st2 = st.clone();
    let resp = st
        .db
        .call(move |c| {
            let tx = c.transaction()?;
            let n: i64 = tx.query_row("SELECT COUNT(*) FROM users", [], |r| r.get(0))?;
            if n > 0 {
                return Err(AppError::new(ErrorCode::SetupDone, "setup already completed"));
            }
            let mut em = Emitter::new();
            let row = create_user(
                &tx,
                &st2.hub,
                NewUser {
                    username: req.username.clone(),
                    display_name: req.display_name.clone(),
                    password_hash: hash,
                    password_set: true,
                    role: Role::Owner,
                    email: req.email.clone(),
                    title: None,
                    phone: None,
                },
                &mut em,
            )?;
            let (sid, tokens) = create_session(&tx, &st2, &row.id, "Setup", "", &ip)?;
            tx.commit()?;
            em.flush(&st2.hub);
            let row = get_user(c, &row.id)?;
            Ok(LoginResponse { tokens, session_id: sid, me: to_me(&row, &st2.hub) })
        })
        .await?;
    Ok(Json(resp))
}

async fn finish_login(st: &St, user_id: String, device: String, version: String, ip: String) -> AppResult<LoginResponse> {
    let st2 = st.clone();
    st.db
        .call(move |c| {
            let (sid, tokens) = create_session(c, &st2, &user_id, &device, &version, &ip)?;
            let now = now_ms();
            c.execute("UPDATE users SET last_active_ms=?1 WHERE id=?2", params![now, user_id])?;
            let row = get_user(c, &user_id)?;
            Ok(LoginResponse { tokens, session_id: sid, me: to_me(&row, &st2.hub) })
        })
        .await
}

pub async fn login(State(st): State<St>, parts: Parts, Json(req): Json<LoginRequest>) -> AppResult<Json<LoginResponse>> {
    check_client_version(&parts)?;
    let ip = client_ip(&parts);
    let ukey = format!("u:{}", req.username.to_lowercase());
    let ikey = format!("ip:{ip}");
    if let Some(secs) = st.lockouts.locked(&[ukey.clone(), ikey.clone()]) {
        return Err(AppError::new(ErrorCode::LockedOut, format!("too many failed attempts, try again in {secs}s")));
    }
    let uname = req.username.clone();
    let found: Option<(String, String, bool, bool)> = st
        .db
        .call(move |c| {
            Ok(c.query_row(
                "SELECT id, password_hash, disabled, deleted FROM users WHERE username=?1",
                [&uname],
                |r| Ok((r.get(0)?, r.get(1)?, r.get::<_, i64>(2)? != 0, r.get::<_, i64>(3)? != 0)),
            )
            .optional()?)
        })
        .await?;
    // Always run one hash verification so timing does not reveal whether the user exists.
    let hash = found.as_ref().map(|f| f.1.clone()).unwrap_or_else(|| st.dummy_hash.clone());
    let pw = req.password.clone();
    let ok = tokio::task::spawn_blocking(move || verify_password(&pw, &hash))
        .await
        .map_err(|e| AppError::internal(e.to_string()))?;
    let Some((uid, _, disabled, deleted)) = found.filter(|_| ok) else {
        st.lockouts.fail(&[(ukey, 1), (ikey, 4)]);
        return Err(AppError::new(ErrorCode::InvalidCredentials, "wrong username or password"));
    };
    if disabled || deleted {
        return Err(AppError::new(ErrorCode::AccountDisabled, "this account is disabled"));
    }
    st.lockouts.success(&ukey);
    Ok(Json(finish_login(&st, uid, req.device_name, req.client_version, ip).await?))
}

pub async fn login_link(State(st): State<St>, parts: Parts, Json(req): Json<LinkLoginRequest>) -> AppResult<Json<LoginResponse>> {
    check_client_version(&parts)?;
    let ip = client_ip(&parts);
    let ikey = format!("ip:{ip}");
    if let Some(secs) = st.lockouts.locked(std::slice::from_ref(&ikey)) {
        return Err(AppError::new(ErrorCode::LockedOut, format!("too many failed attempts, try again in {secs}s")));
    }
    let parsed = rules::parse_login_link(&req.token)
        .map(|p| p.token)
        .ok_or_else(|| AppError::new(ErrorCode::InvalidCredentials, "that link or code is not valid"))?;
    let th = sha256_hex(&parsed);
    let now = now_ms();
    let uid: Option<String> = st
        .db
        .call(move |c| {
            let tx = c.transaction()?;
            let row: Option<(String, i64, Option<i64>)> = tx
                .query_row(
                    "SELECT user_id, expires_ms, used_ms FROM login_links WHERE token_hash=?1",
                    [&th],
                    |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
                )
                .optional()?;
            let out = match row {
                Some((uid, exp, None)) if exp > now => {
                    tx.execute("UPDATE login_links SET used_ms=?2 WHERE token_hash=?1", params![th, now])?;
                    Some(uid)
                }
                _ => None,
            };
            tx.commit()?;
            Ok(out)
        })
        .await?;
    let Some(uid) = uid else {
        st.lockouts.fail(&[(ikey, 2)]);
        return Err(AppError::new(ErrorCode::InvalidCredentials, "that link or code is invalid, expired or already used"));
    };
    let u2 = uid.clone();
    let (disabled, deleted): (bool, bool) = st
        .db
        .call(move |c| {
            let r = get_user(c, &u2)?;
            Ok((r.disabled, r.deleted))
        })
        .await?;
    if disabled || deleted {
        return Err(AppError::new(ErrorCode::AccountDisabled, "this account is disabled"));
    }
    Ok(Json(finish_login(&st, uid, req.device_name, req.client_version, ip).await?))
}

pub async fn refresh(State(st): State<St>, Json(req): Json<RefreshRequest>) -> AppResult<Json<TokenPair>> {
    let rh = sha256_hex(&req.refresh_token);
    let st2 = st.clone();
    let tokens = st
        .db
        .call(move |c| {
            let row: Option<(String, String, i64, i64, Option<String>)> = c
                .query_row(
                    "SELECT id, user_id, refresh_exp_ms, revoked, revoke_reason FROM sessions WHERE refresh_hash=?1",
                    [&rh],
                    |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?)),
                )
                .optional()?;
            let Some((sid, uid, exp, revoked, reason)) = row else {
                return Err(AppError::new(ErrorCode::Unauthorized, "unknown refresh token"));
            };
            if revoked != 0 {
                return Err(AppError::new(revoke_code(reason.as_deref().unwrap_or("")), "session ended"));
            }
            let u = get_user(c, &uid)?;
            if u.disabled || u.deleted {
                return Err(AppError::new(ErrorCode::AccountDisabled, "account disabled"));
            }
            if exp < now_ms() {
                return Err(AppError::new(ErrorCode::SessionRevoked, "refresh token expired"));
            }
            let access = random_token("at_", 32);
            let now = now_ms();
            c.execute(
                "UPDATE sessions SET access_hash=?2, access_exp_ms=?3, last_seen_ms=?4 WHERE id=?1",
                params![sid, sha256_hex(&access), now + st2.cfg.access_ttl_secs * 1000, now],
            )?;
            Ok(TokenPair { access_token: access, refresh_token: String::new(), expires_in: st2.cfg.access_ttl_secs })
        })
        .await?;
    Ok(Json(TokenPair { refresh_token: req.refresh_token, ..tokens }))
}

/// Self-service sign-out is only for admins and for members whose admin enabled "Privacy options".
fn ensure_signout_allowed(c: &rusqlite::Connection, auth: &Auth) -> AppResult<()> {
    if auth.is_admin() {
        return Ok(());
    }
    if get_user(c, &auth.user_id)?.privacy_controls_enabled {
        return Ok(());
    }
    Err(AppError::forbidden("sign-out is not allowed for this account"))
}

pub async fn logout(State(st): State<St>, auth: Auth) -> AppResult<Json<serde_json::Value>> {
    let st2 = st.clone();
    st.db
        .call(move |c| {
            ensure_signout_allowed(c, &auth)?;
            c.execute(
                "UPDATE sessions SET revoked=1, revoke_reason='logout' WHERE id=?1",
                [&auth.session_id],
            )?;
            st2.hub.kick_session(&auth.session_id, ErrorCode::SessionRevoked, close::SESSION_ENDED, "logout");
            Ok(())
        })
        .await?;
    Ok(Json(serde_json::json!({"ok": true})))
}

pub async fn get_me(State(st): State<St>, auth: Auth) -> AppResult<Json<Me>> {
    let st2 = st.clone();
    Ok(Json(
        st.db
            .call(move |c| Ok(to_me(&get_user(c, &auth.user_id)?, &st2.hub)))
            .await?,
    ))
}

pub async fn update_me(State(st): State<St>, auth: Auth, Json(req): Json<UpdateMeRequest>) -> AppResult<Json<Me>> {
    let st2 = st.clone();
    let me = st
        .db
        .call(move |c| {
            let row = get_user(c, &auth.user_id)?;
            let tx = c.transaction()?;
            if let Some(n) = &req.display_name {
                let n = n.trim();
                if n.is_empty() || n.chars().count() > 80 {
                    return Err(AppError::validation("display name must be 1-80 characters"));
                }
                tx.execute("UPDATE users SET display_name=?2 WHERE id=?1", params![auth.user_id, n])?;
            }
            if req.title.is_some() {
                let t = profile_field(&req.title, 80, "job title")?;
                tx.execute("UPDATE users SET title=?2 WHERE id=?1", params![auth.user_id, t])?;
            }
            if req.phone.is_some() {
                let t = profile_field(&req.phone, 40, "phone number")?;
                tx.execute("UPDATE users SET phone=?2 WHERE id=?1", params![auth.user_id, t])?;
            }
            if let Some(r) = req.read_receipts {
                tx.execute("UPDATE users SET read_receipts=?2 WHERE id=?1", params![auth.user_id, r as i64])?;
            }
            if let Some(p) = &req.presence_pref {
                if !matches!(p.as_str(), "auto" | "away" | "dnd") {
                    return Err(AppError::validation("presence_pref must be auto, away or dnd"));
                }
                tx.execute("UPDATE users SET presence_pref=?2 WHERE id=?1", params![auth.user_id, p])?;
            }
            if let Some(h) = req.hide_last_active {
                if !row.privacy_controls_enabled {
                    return Err(AppError::forbidden("privacy controls are not enabled for this account"));
                }
                tx.execute("UPDATE users SET hide_last_active=?2 WHERE id=?1", params![auth.user_id, h as i64])?;
            }
            if let Some(a) = &req.avatar {
                if a.is_empty() {
                    tx.execute("UPDATE users SET avatar_attachment=NULL WHERE id=?1", [&auth.user_id])?;
                } else {
                    let ok: i64 = tx.query_row(
                        "SELECT COUNT(*) FROM attachments WHERE id=?1 AND uploader_id=?2 AND purpose='avatar' AND removed=0",
                        params![a, auth.user_id],
                        |r| r.get(0),
                    )?;
                    if ok == 0 {
                        return Err(AppError::validation("unknown avatar upload"));
                    }
                    tx.execute("UPDATE users SET avatar_attachment=?2 WHERE id=?1", params![auth.user_id, a])?;
                }
            }
            let mut em = Emitter::new();
            announce_user(&tx, &st2.hub, &auth.user_id, &mut em)?;
            let row = get_user(&tx, &auth.user_id)?;
            let me = to_me(&row, &st2.hub);
            em.emit(&tx, &auth.user_id, EventData::MeUpdate { me: me.clone() })?;
            tx.commit()?;
            em.flush(&st2.hub);
            crate::routes::ws::broadcast_presence(c, &st2, &auth.user_id)?;
            Ok(me)
        })
        .await?;
    Ok(Json(me))
}

pub async fn change_password(State(st): State<St>, auth: Auth, Json(req): Json<ChangePasswordRequest>) -> AppResult<Json<serde_json::Value>> {
    validate_password(&req.new_password)?;
    let uid = auth.user_id.clone();
    let (hash, set): (String, bool) = st
        .db
        .call(move |c| {
            let r = get_user(c, &uid)?;
            let h: String = c.query_row("SELECT password_hash FROM users WHERE id=?1", [&uid], |r| r.get(0))?;
            Ok((h, r.password_set))
        })
        .await?;
    if set {
        let cur = req.current_password.clone().unwrap_or_default();
        let ok = tokio::task::spawn_blocking(move || verify_password(&cur, &hash)).await.map_err(|e| AppError::internal(e.to_string()))?;
        if !ok {
            return Err(AppError::new(ErrorCode::InvalidCredentials, "current password is wrong"));
        }
    }
    let new_hash = hash_pw_async(req.new_password.clone()).await?;
    let st2 = st.clone();
    st.db
        .call(move |c| {
            c.execute(
                "UPDATE users SET password_hash=?2, password_set=1 WHERE id=?1",
                params![auth.user_id, new_hash],
            )?;
            revoke_sessions(c, &st2.hub, &auth.user_id, "session_revoked", Some(&auth.session_id))?;
            Ok(())
        })
        .await?;
    Ok(Json(serde_json::json!({"ok": true})))
}

pub async fn list_sessions(State(st): State<St>, auth: Auth) -> AppResult<Json<Vec<SessionInfo>>> {
    Ok(Json(
        st.db
            .call(move |c| {
                let mut stmt = c.prepare(
                    "SELECT id,user_id,device_name,client_version,ip,created_ms,last_seen_ms,revoked FROM sessions WHERE user_id=?1 AND revoked=0 ORDER BY last_seen_ms DESC",
                )?;
                let v = stmt
                    .query_map([&auth.user_id], |r| {
                        let id: String = r.get(0)?;
                        Ok(SessionInfo {
                            current: id == auth.session_id,
                            id,
                            user_id: r.get(1)?,
                            username: String::new(),
                            device_name: r.get(2)?,
                            client_version: r.get(3)?,
                            ip: r.get(4)?,
                            created_ms: r.get(5)?,
                            last_seen_ms: r.get(6)?,
                            revoked: r.get::<_, i64>(7)? != 0,
                        })
                    })?
                    .collect::<Result<Vec<_>, _>>()?;
                Ok(v)
            })
            .await?,
    ))
}

pub async fn revoke_session(State(st): State<St>, auth: Auth, Path(sid): Path<String>) -> AppResult<Json<serde_json::Value>> {
    let st2 = st.clone();
    st.db
        .call(move |c| {
            let owner: Option<String> =
                c.query_row("SELECT user_id FROM sessions WHERE id=?1", [&sid], |r| r.get(0)).optional()?;
            match owner {
                Some(ref o) if *o == auth.user_id || auth.is_admin() => {}
                _ => return Err(AppError::not_found("session not found")),
            }
            if owner.as_deref() == Some(auth.user_id.as_str()) {
                ensure_signout_allowed(c, &auth)?;
            }
            c.execute("UPDATE sessions SET revoked=1, revoke_reason='session_revoked' WHERE id=?1", [&sid])?;
            st2.hub.kick_session(&sid, ErrorCode::SessionRevoked, close::SESSION_ENDED, "session revoked");
            Ok(())
        })
        .await?;
    Ok(Json(serde_json::json!({"ok": true})))
}

pub async fn revoke_other_sessions(State(st): State<St>, auth: Auth) -> AppResult<Json<serde_json::Value>> {
    let st2 = st.clone();
    let n = st
        .db
        .call(move |c| {
            ensure_signout_allowed(c, &auth)?;
            revoke_sessions(c, &st2.hub, &auth.user_id, "session_revoked", Some(&auth.session_id))
        })
        .await?;
    Ok(Json(serde_json::json!({"revoked": n})))
}

/// Signs the caller out everywhere, including this device.
pub async fn revoke_all_sessions(State(st): State<St>, auth: Auth) -> AppResult<Json<serde_json::Value>> {
    let st2 = st.clone();
    let n = st
        .db
        .call(move |c| {
            ensure_signout_allowed(c, &auth)?;
            revoke_sessions(c, &st2.hub, &auth.user_id, "logout", None)
        })
        .await?;
    Ok(Json(serde_json::json!({"revoked": n})))
}

pub fn ensure_secret(conn: &rusqlite::Connection) -> AppResult<Vec<u8>> {
    if let Some(s) = crate::db::get_setting(conn, "server_secret")? {
        return Ok(hex::decode(s).unwrap_or_default());
    }
    let tok = random_token("", 32);
    let bytes = tok.into_bytes();
    set_setting(conn, "server_secret", &hex::encode(&bytes))?;
    Ok(bytes)
}
