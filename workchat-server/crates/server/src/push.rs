//! SV-13: content-free wake-up pushes through ntfy, FCM (HTTP v1) and APNs (token auth).
//!
//! A push is sent only for a new message, only to members without a live WebSocket, and only
//! when Do Not Disturb, mute and the per-conversation notification level allow it. The payload
//! never contains message text, names or ids: the app wakes up, syncs, then posts its own
//! local notification.

use crate::db::{get_setting, new_id, now_ms, set_setting};
use crate::error::{AppError, AppResult};
use crate::org::pref_for;
use crate::queries::*;
use crate::state::St;
use jsonwebtoken::{Algorithm, EncodingKey, Header};
use proto::*;
use rusqlite::{params, Connection};
use serde::Deserialize;
use std::sync::Mutex;

pub const PUSH_KEY: &str = "push_settings";

pub fn load_settings(conn: &Connection) -> AppResult<PushSettings> {
    Ok(get_setting(conn, PUSH_KEY)?.and_then(|s| serde_json::from_str(&s).ok()).unwrap_or_default())
}

pub fn save_settings(conn: &Connection, s: &PushSettings) -> AppResult<()> {
    set_setting(conn, PUSH_KEY, &serde_json::to_string(s)?)
}

/// The effective ntfy URL: admin setting first, then `WORKCHAT_NTFY_URL`.
pub fn effective_ntfy(st: &St, s: &PushSettings) -> Option<String> {
    s.ntfy_url.clone().filter(|u| !u.is_empty()).or_else(|| st.cfg.ntfy_url.clone())
}

#[derive(Debug, Clone)]
pub struct Target {
    pub user_id: String,
    pub reg: PushRegistration,
}

/// Who gets a wake-up for this message (pure database logic, unit tested through the flow tests).
pub fn wake_targets(conn: &Connection, hub: &crate::hub::Hub, msg: &Message) -> AppResult<Vec<Target>> {
    let now = now_ms();
    let mut out = vec![];
    for uid in member_ids(conn, &msg.conv_id)? {
        if uid == msg.sender_id || hub.is_online(&uid) {
            continue;
        }
        let u = get_user(conn, &uid)?;
        if u.disabled || u.deleted || u.is_bot {
            continue;
        }
        let hidden: i64 = conn.query_row("SELECT COUNT(*) FROM message_hidden WHERE user_id=?1 AND message_id=?2", params![uid, msg.id], |r| r.get(0))?;
        if hidden > 0 {
            continue;
        }
        let pref = pref_for(conn, &uid, &msg.conv_id)?;
        let mentioned = msg.mentions.contains(&uid);
        if !rules::should_notify(pref.as_ref(), now, &u.presence_pref, mentioned) {
            continue;
        }
        let mut st = conn.prepare(
            "SELECT r.id,r.kind,r.target,r.ntfy_url,r.device_name,r.created_ms FROM push_registrations r
             JOIN sessions s ON s.id=r.session_id AND s.revoked=0 WHERE r.user_id=?1",
        )?;
        let regs = st
            .query_map([&uid], |r| {
                Ok(PushRegistration { id: r.get(0)?, kind: r.get(1)?, target: r.get(2)?, ntfy_url: r.get(3)?, device_name: r.get(4)?, created_ms: r.get(5)? })
            })?
            .collect::<Result<Vec<_>, _>>()?;
        for reg in regs {
            out.push(Target { user_id: uid.clone(), reg });
        }
    }
    Ok(out)
}

pub fn spawn_notify(st: &St, msg: Message) {
    let st = st.clone();
    tokio::spawn(async move {
        if let Err(e) = notify_message(&st, &msg).await {
            tracing::warn!("push: {e}");
        }
    });
}

pub async fn notify_message(st: &St, msg: &Message) -> AppResult<usize> {
    let (hub, m2) = (st.hub.clone(), msg.clone());
    let (targets, settings) = st.db.call(move |c| Ok((wake_targets(c, &hub, &m2)?, load_settings(c)?))).await?;
    let mut sent = 0;
    for t in targets {
        match deliver(st, &settings, &t.reg).await {
            Ok(()) => sent += 1,
            Err(Gone) => {
                let id = t.reg.id.clone();
                let _ = st.db.call(move |c| Ok(c.execute("DELETE FROM push_registrations WHERE id=?1", [&id])?)).await;
            }
        }
    }
    Ok(sent)
}

/// A destination that no longer exists (token unregistered): the registration is dropped.
pub struct Gone;

async fn deliver(st: &St, s: &PushSettings, reg: &PushRegistration) -> Result<(), Gone> {
    let res = match reg.kind.as_str() {
        "ntfy" => send_ntfy(st, s, reg).await,
        "unifiedpush" => send_unifiedpush(st, reg).await,
        "fcm" => send_fcm(st, s, reg).await,
        "apns" => send_apns(st, s, reg).await,
        _ => Ok(()),
    };
    match res {
        Ok(()) => Ok(()),
        Err(PushErr::Gone) => Err(Gone),
        Err(PushErr::Other(e)) => {
            tracing::warn!("push via {} failed: {e}", reg.kind);
            Ok(())
        }
    }
}

enum PushErr {
    Gone,
    Other(String),
}

fn other(e: impl std::fmt::Display) -> PushErr {
    PushErr::Other(e.to_string())
}

async fn send_ntfy(st: &St, s: &PushSettings, reg: &PushRegistration) -> Result<(), PushErr> {
    let base = reg.ntfy_url.clone().filter(|u| !u.is_empty()).or_else(|| effective_ntfy(st, s)).ok_or_else(|| other("no ntfy server configured"))?;
    let topic: String = reg.target.chars().filter(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_')).collect();
    if topic.is_empty() {
        return Err(other("bad ntfy topic"));
    }
    let resp = st
        .http
        .post(format!("{}/{}", base.trim_end_matches('/'), topic))
        .header("Title", "WorkChat")
        .header("Priority", "default")
        .header("Tags", "wake")
        .body("wake")
        .send()
        .await
        .map_err(other)?;
    if resp.status().is_success() {
        Ok(())
    } else {
        Err(other(format!("ntfy HTTP {}", resp.status())))
    }
}

/// UnifiedPush: the app got a complete endpoint URL from its distributor (the ntfy app, NextPush, Gotify, ...), for example
/// `https://ntfy.example.com/upAbC123?up=1`. A plain POST with a body wakes the app; nothing else is needed.
async fn send_unifiedpush(st: &St, reg: &PushRegistration) -> Result<(), PushErr> {
    let resp = st.http.post(&reg.target).header("Content-Type", "text/plain").header("Title", "WorkChat").header("Tags", "wake").body("wake").send().await.map_err(other)?;
    let status = resp.status();
    if status.is_success() {
        Ok(())
    } else if status == reqwest::StatusCode::NOT_FOUND || status == reqwest::StatusCode::GONE {
        // The distributor forgot the endpoint (the app was unregistered).
        Err(PushErr::Gone)
    } else {
        Err(other(format!("UnifiedPush HTTP {status}")))
    }
}

// ---------------------------------------------------------------- FCM HTTP v1

#[derive(Deserialize)]
struct ServiceAccount {
    client_email: String,
    private_key: String,
    #[serde(default)]
    token_uri: Option<String>,
    #[serde(default)]
    project_id: Option<String>,
}

static FCM_TOKEN: Mutex<Option<(String, i64)>> = Mutex::new(None);

async fn fcm_access_token(st: &St, sa: &ServiceAccount, token_uri_override: Option<&str>) -> Result<String, PushErr> {
    let cached = FCM_TOKEN.lock().unwrap_or_else(|e| e.into_inner()).clone();
    if let Some((t, exp)) = cached {
        if exp > now_ms() + 60_000 {
            return Ok(t);
        }
    }
    let token_uri = token_uri_override.map(str::to_string).or_else(|| sa.token_uri.clone()).unwrap_or_else(|| "https://oauth2.googleapis.com/token".into());
    let now = now_ms() / 1000;
    let claims = serde_json::json!({
        "iss": sa.client_email,
        "scope": "https://www.googleapis.com/auth/firebase.messaging",
        "aud": token_uri,
        "iat": now,
        "exp": now + 3600,
    });
    let key = EncodingKey::from_rsa_pem(sa.private_key.as_bytes()).map_err(other)?;
    let jwt = jsonwebtoken::encode(&Header::new(Algorithm::RS256), &claims, &key).map_err(other)?;
    let resp = st
        .http
        .post(&token_uri)
        .form(&[("grant_type", "urn:ietf:params:oauth:grant-type:jwt-bearer"), ("assertion", jwt.as_str())])
        .send()
        .await
        .map_err(other)?;
    if !resp.status().is_success() {
        return Err(other(format!("FCM token HTTP {}", resp.status())));
    }
    #[derive(Deserialize)]
    struct Tok {
        access_token: String,
        #[serde(default)]
        expires_in: Option<i64>,
    }
    let t: Tok = resp.json().await.map_err(other)?;
    *FCM_TOKEN.lock().unwrap_or_else(|e| e.into_inner()) = Some((t.access_token.clone(), now_ms() + t.expires_in.unwrap_or(3000) * 1000));
    Ok(t.access_token)
}

async fn send_fcm(st: &St, s: &PushSettings, reg: &PushRegistration) -> Result<(), PushErr> {
    let sa_json = s.fcm_service_account.clone().ok_or_else(|| other("FCM is not configured"))?;
    let sa: ServiceAccount = serde_json::from_str(&sa_json).map_err(other)?;
    let project = s.fcm_project_id.clone().or_else(|| sa.project_id.clone()).ok_or_else(|| other("no FCM project id"))?;
    let base = s.fcm_api_base.clone().unwrap_or_else(|| "https://fcm.googleapis.com".into());
    // A test token endpoint lives next to the fake API base.
    let token_override = s.fcm_api_base.as_ref().map(|b| format!("{}/oauth2/token", b.trim_end_matches('/')));
    let access = fcm_access_token(st, &sa, token_override.as_deref()).await?;
    let body = serde_json::json!({
        "message": {
            "token": reg.target,
            "data": {"type": "wake"},
            "android": {"priority": "HIGH"},
        }
    });
    let resp = st
        .http
        .post(format!("{}/v1/projects/{}/messages:send", base.trim_end_matches('/'), project))
        .bearer_auth(access)
        .json(&body)
        .send()
        .await
        .map_err(other)?;
    let status = resp.status();
    if status.is_success() {
        Ok(())
    } else if status.as_u16() == 404 || status.as_u16() == 410 {
        Err(PushErr::Gone)
    } else {
        Err(other(format!("FCM HTTP {status}")))
    }
}

// ---------------------------------------------------------------- APNs token auth

static APNS_JWT: Mutex<Option<(String, i64)>> = Mutex::new(None);

fn apns_jwt(s: &PushSettings) -> Result<String, PushErr> {
    if let Some((t, exp)) = APNS_JWT.lock().unwrap_or_else(|e| e.into_inner()).clone() {
        if exp > now_ms() {
            return Ok(t);
        }
    }
    let pem = s.apns_key_pem.clone().ok_or_else(|| other("APNs is not configured"))?;
    let mut h = Header::new(Algorithm::ES256);
    h.kid = s.apns_key_id.clone();
    let claims = serde_json::json!({"iss": s.apns_team_id.clone().unwrap_or_default(), "iat": now_ms() / 1000});
    let key = EncodingKey::from_ec_pem(pem.as_bytes()).map_err(other)?;
    let jwt = jsonwebtoken::encode(&h, &claims, &key).map_err(other)?;
    *APNS_JWT.lock().unwrap_or_else(|e| e.into_inner()) = Some((jwt.clone(), now_ms() + 40 * 60 * 1000));
    Ok(jwt)
}

async fn send_apns(st: &St, s: &PushSettings, reg: &PushRegistration) -> Result<(), PushErr> {
    let jwt = apns_jwt(s)?;
    let base = s.apns_api_base.clone().unwrap_or_else(|| if s.apns_sandbox { "https://api.sandbox.push.apple.com".into() } else { "https://api.push.apple.com".into() });
    let resp = st
        .http
        .post(format!("{}/3/device/{}", base.trim_end_matches('/'), reg.target))
        .header("authorization", format!("bearer {jwt}"))
        .header("apns-push-type", "background")
        .header("apns-priority", "5")
        .header("apns-topic", s.apns_topic.clone().unwrap_or_default())
        .json(&serde_json::json!({"aps": {"content-available": 1}}))
        .send()
        .await
        .map_err(other)?;
    let status = resp.status();
    if status.is_success() {
        Ok(())
    } else if status.as_u16() == 410 || status.as_u16() == 400 && resp.text().await.unwrap_or_default().contains("BadDeviceToken") {
        Err(PushErr::Gone)
    } else {
        Err(other(format!("APNs HTTP {status}")))
    }
}

// ---------------------------------------------------------------- routes

use crate::auth::{Admin, Auth};
use axum::extract::{Path, State};
use axum::Json;

pub async fn register(State(st): State<St>, auth: Auth, Json(req): Json<PushRegistration>) -> AppResult<Json<PushRegistration>> {
    if !matches!(req.kind.as_str(), "fcm" | "ntfy" | "unifiedpush" | "apns") {
        return Err(AppError::validation("kind must be fcm, ntfy, unifiedpush or apns"));
    }
    if req.kind == "unifiedpush" && !url::Url::parse(req.target.trim()).map(|u| matches!(u.scheme(), "http" | "https") && u.has_host()).unwrap_or(false) {
        return Err(AppError::validation("a unifiedpush target must be the full endpoint URL (http or https) given by the distributor"));
    }
    if req.target.trim().is_empty() || req.target.len() > 512 {
        return Err(AppError::validation("missing push target"));
    }
    if let Some(u) = &req.ntfy_url {
        if !u.is_empty() && !(u.starts_with("https://") || u.starts_with("http://")) {
            return Err(AppError::validation("the ntfy server must be an http(s) URL"));
        }
    }
    Ok(Json(
        st.db
            .call(move |c| {
                // One registration per device session and kind.
                c.execute("DELETE FROM push_registrations WHERE session_id=?1 AND kind=?2", params![auth.session_id, req.kind])?;
                let id = new_id();
                let now = now_ms();
                c.execute(
                    "INSERT INTO push_registrations(id,user_id,session_id,kind,target,ntfy_url,device_name,created_ms) VALUES(?1,?2,?3,?4,?5,?6,?7,?8)",
                    params![id, auth.user_id, auth.session_id, req.kind, req.target.trim(), req.ntfy_url, req.device_name, now],
                )?;
                Ok(PushRegistration { id, kind: req.kind, target: req.target.trim().to_string(), ntfy_url: req.ntfy_url, device_name: req.device_name, created_ms: now })
            })
            .await?,
    ))
}

pub async fn list(State(st): State<St>, auth: Auth) -> AppResult<Json<Vec<PushRegistration>>> {
    Ok(Json(
        st.db
            .call(move |c| {
                let mut s = c.prepare("SELECT id,kind,target,ntfy_url,device_name,created_ms FROM push_registrations WHERE user_id=?1 ORDER BY created_ms")?;
                let v = s
                    .query_map([&auth.user_id], |r| Ok(PushRegistration { id: r.get(0)?, kind: r.get(1)?, target: r.get(2)?, ntfy_url: r.get(3)?, device_name: r.get(4)?, created_ms: r.get(5)? }))?
                    .collect::<Result<Vec<_>, _>>()?;
                Ok(v)
            })
            .await?,
    ))
}

pub async fn unregister(State(st): State<St>, auth: Auth, Path(id): Path<String>) -> AppResult<Json<serde_json::Value>> {
    st.db
        .call(move |c| {
            c.execute("DELETE FROM push_registrations WHERE id=?1 AND user_id=?2", params![id, auth.user_id])?;
            Ok(())
        })
        .await?;
    Ok(Json(serde_json::json!({"ok": true})))
}

pub async fn get_settings(State(st): State<St>, Admin(_a): Admin) -> AppResult<Json<PushSettings>> {
    Ok(Json(st.db.call(|c| load_settings(c)).await?.masked()))
}

pub async fn put_settings(State(st): State<St>, Admin(a): Admin, Json(req): Json<PushSettings>) -> AppResult<Json<PushSettings>> {
    if a.role != Role::Owner {
        return Err(AppError::forbidden("only the owner can change server-wide settings"));
    }
    let s = st
        .db
        .call(move |c| {
            let prev = load_settings(c)?;
            let new = req.unmask_from(&prev);
            if let Some(sa) = &new.fcm_service_account {
                serde_json::from_str::<serde_json::Value>(sa).map_err(|_| AppError::validation("the FCM service account must be the JSON file from Firebase"))?;
            }
            save_settings(c, &new)?;
            Ok(new)
        })
        .await?;
    *FCM_TOKEN.lock().unwrap_or_else(|e| e.into_inner()) = None;
    *APNS_JWT.lock().unwrap_or_else(|e| e.into_inner()) = None;
    Ok(Json(s.masked()))
}
