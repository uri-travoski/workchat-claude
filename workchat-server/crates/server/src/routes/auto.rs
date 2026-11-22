//! Member-facing automation routes: scheduled messages (SV-10), form responses and bot profiles
//! (SV-17), AI search and recap (SV-21) and the incoming webhook receiver (SV-16).

use crate::auth::Auth;
use crate::bots;
use crate::db::new_id;
use crate::error::{AppError, AppResult};
use crate::state::St;
use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::Json;
use proto::*;
use rusqlite::OptionalExtension;
use serde::Deserialize;

type J<T> = AppResult<Json<T>>;

pub async fn list_scheduled(State(st): State<St>, auth: Auth) -> J<Vec<ScheduledMessage>> {
    Ok(Json(st.db.call(move |c| crate::scheduled::list(c, &auth.user_id)).await?))
}

pub async fn schedule(State(st): State<St>, auth: Auth, Path(conv): Path<String>, Json(req): Json<ScheduleRequest>) -> AppResult<(StatusCode, Json<ScheduledMessage>)> {
    let hub = st.hub.clone();
    let item = st.db.call(move |c| crate::scheduled::create(c, &hub, &auth.user_id, &conv, &req)).await?;
    Ok((StatusCode::CREATED, Json(item)))
}

pub async fn cancel_scheduled(State(st): State<St>, auth: Auth, Path(id): Path<String>) -> J<ScheduledMessage> {
    let hub = st.hub.clone();
    Ok(Json(st.db.call(move |c| crate::scheduled::cancel(c, &hub, &auth.user_id, &id)).await?))
}

pub async fn form_response(State(st): State<St>, auth: Auth, Path(id): Path<String>, Json(req): Json<FormSubmit>) -> J<Message> {
    let hub = st.hub.clone();
    Ok(Json(st.db.call(move |c| bots::submit_form(c, &hub, &auth.user_id, &id, &req)).await?))
}

pub async fn list_bot_profiles(State(st): State<St>, _auth: Auth) -> J<Vec<BotProfile>> {
    let hub = st.hub.clone();
    Ok(Json(
        st.db
            .call(move |c| {
                let mut s = c.prepare("SELECT b.user_id FROM bots b JOIN users u ON u.id=b.user_id WHERE u.deleted=0 AND u.disabled=0 ORDER BY u.display_name COLLATE NOCASE")?;
                let ids = s.query_map([], |r| r.get::<_, String>(0))?.collect::<Result<Vec<_>, _>>()?;
                ids.iter().map(|i| bots::profile(c, &hub, i)).collect()
            })
            .await?,
    ))
}

pub async fn bot_profile(State(st): State<St>, _auth: Auth, Path(id): Path<String>) -> J<BotProfile> {
    let hub = st.hub.clone();
    Ok(Json(st.db.call(move |c| bots::profile(c, &hub, &id)).await?))
}

#[derive(Deserialize)]
pub struct HomeSubmit {
    #[serde(default)]
    pub form_id: Option<String>,
    #[serde(flatten)]
    pub submit: FormSubmit,
}

pub async fn home_submit(State(st): State<St>, auth: Auth, Path(id): Path<String>, Json(req): Json<HomeSubmit>) -> J<serde_json::Value> {
    st.db.call(move |c| bots::submit_home(c, &auth.user_id, &id, req.form_id.as_deref(), &req.submit)).await?;
    Ok(Json(serde_json::json!({"ok": true})))
}

pub async fn ai_search(State(st): State<St>, auth: Auth, Json(req): Json<AiSearchRequest>) -> J<AiSearchResponse> {
    Ok(Json(crate::ai::ai_search(&st, &auth.user_id, req).await?))
}

pub async fn ai_recap(State(st): State<St>, auth: Auth, Json(req): Json<AiRecapRequest>) -> J<AiRecapResponse> {
    Ok(Json(crate::ai::ai_recap(&st, &auth.user_id, req).await?))
}

#[derive(Deserialize)]
pub struct IncomingBody {
    #[serde(alias = "body", alias = "content", default)]
    pub text: String,
    #[serde(default)]
    pub thread_root: Option<String>,
    #[serde(default)]
    pub form: Option<Form>,
    #[serde(default)]
    pub client_id: Option<String>,
}

/// `POST /api/v1/hooks/{id}/{secret}`: the secret URL is the credential.
pub async fn incoming_hook(State(st): State<St>, Path((id, secret)): Path<(String, String)>, Json(req): Json<IncomingBody>) -> AppResult<(StatusCode, Json<serde_json::Value>)> {
    if !st.limiter.check(&format!("hook:{id}")) {
        return Err(AppError::new(ErrorCode::RateLimited, "too many requests"));
    }
    let (id2, secret2) = (id.clone(), secret.clone());
    let found: Option<(String, String, bool)> = st
        .db
        .call(move |c| {
            Ok(c.query_row(
                "SELECT conv_id, bot_user_id, enabled FROM webhooks_in WHERE id=?1 AND secret=?2",
                [&id2, &secret2],
                |r| Ok((r.get(0)?, r.get(1)?, r.get::<_, i64>(2)? != 0)),
            )
            .optional()?)
        })
        .await?;
    let (conv, bot, enabled) = found.ok_or_else(|| AppError::new(ErrorCode::Unauthorized, "unknown webhook"))?;
    if !enabled {
        return Err(AppError::forbidden("this webhook is disabled"));
    }
    let msg = SendMessage {
        client_id: req.client_id.clone().filter(|c| !c.is_empty()).unwrap_or_else(|| format!("hook-{}", new_id())),
        body: req.text,
        thread_root: req.thread_root,
        reply_to: None,
        mentions: vec![],
        attachment_ids: vec![],
        also_send_to_channel: false,
    };
    let (created, m) = crate::routes::chat::post_as(&st, &bot, &conv, msg, req.form).await?;
    let (id3, now) = (id, crate::db::now_ms());
    st.db.call(move |c| { c.execute("UPDATE webhooks_in SET last_used_ms=?2 WHERE id=?1", rusqlite::params![id3, now])?; Ok(()) }).await?;
    Ok((if created { StatusCode::CREATED } else { StatusCode::OK }, Json(serde_json::json!({"ok": true, "message_id": m.id}))))
}
