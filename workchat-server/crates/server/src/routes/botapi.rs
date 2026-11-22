//! Bot-facing HTTP API (SV-15): Telegram-compatible `/bot<token>/<method>` endpoints and the
//! native bearer-token REST API under `/api/v1/bot`. Both call the same operations.

use crate::bots::{self, BotCtx};
use crate::db::new_id;
use crate::error::{AppError, AppResult};
use crate::queries::*;
use crate::routes::files::{self, UploadQuery};
use crate::state::St;
use axum::body::Body;
use axum::extract::{FromRequest, FromRequestParts, Multipart, Path, Query, Request, State};
use axum::http::request::Parts;
use axum::http::{header, HeaderMap, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::Json;
use futures_util::StreamExt;
use proto::*;
use rusqlite::OptionalExtension;
use serde_json::{json, Map, Value};
use sha2::{Digest, Sha256};
use std::path::PathBuf;
use tokio::io::AsyncWriteExt;

// ------------------------------------------------------------------ auth

async fn auth_token(st: &St, token: &str) -> AppResult<BotCtx> {
    let t = token.to_string();
    let bot = st.db.call(move |c| bots::lookup(c, &t)).await?;
    if !st.limiter.check(&format!("bot:{}", bot.id)) {
        return Err(AppError::new(ErrorCode::RateLimited, "too many requests"));
    }
    Ok(bot)
}

/// Extractor for the native API: `Authorization: Bearer bt_...`.
pub struct BotAuth(pub BotCtx);

impl FromRequestParts<St> for BotAuth {
    type Rejection = AppError;
    async fn from_request_parts(parts: &mut Parts, st: &St) -> Result<Self, Self::Rejection> {
        let token = parts
            .headers
            .get(header::AUTHORIZATION)
            .and_then(|v| v.to_str().ok())
            .and_then(|v| v.strip_prefix("Bearer "))
            .map(str::trim)
            .ok_or_else(|| AppError::new(ErrorCode::Unauthorized, "missing bearer token"))?;
        Ok(BotAuth(auth_token(st, token).await?))
    }
}

// ------------------------------------------------------------------ shared operations

pub struct NewMsg {
    pub conv_id: String,
    pub body: String,
    pub reply_to: Option<String>,
    pub thread_root: Option<String>,
    pub form: Option<Form>,
    pub attachment_ids: Vec<String>,
    pub client_id: Option<String>,
    pub mentions: Vec<String>,
}

pub async fn bot_send(st: &St, bot: &BotCtx, m: NewMsg) -> AppResult<(bool, Message)> {
    let req = SendMessage {
        client_id: m.client_id.filter(|c| !c.is_empty()).unwrap_or_else(|| format!("bot-{}", new_id())),
        body: m.body,
        thread_root: m.thread_root,
        reply_to: m.reply_to,
        mentions: m.mentions,
        attachment_ids: m.attachment_ids,
        also_send_to_channel: false,
    };
    crate::routes::chat::post_as(st, &bot.id, &m.conv_id, req, m.form).await
}

pub async fn bot_delete(st: &St, bot: &BotCtx, id: &str) -> AppResult<()> {
    let hub = st.hub.clone();
    let (bid, mid) = (bot.id.clone(), id.to_string());
    let keys = st
        .db
        .call(move |c| {
            let (conv, sender): (String, String) = c
                .query_row("SELECT conv_id, sender_id FROM messages WHERE id=?1", [&mid], |r| Ok((r.get(0)?, r.get(1)?)))
                .optional()?
                .ok_or_else(|| AppError::not_found("message not found"))?;
            if sender != bid || !is_member(c, &conv, &bid)? {
                return Err(AppError::forbidden("a bot can only delete its own messages"));
            }
            let tx = c.transaction()?;
            let mut em = crate::emit::Emitter::new();
            let keys = crate::routes::chat::tombstone_message(&tx, &mut em, &mid)?;
            tx.commit()?;
            em.flush(&hub);
            Ok(keys)
        })
        .await?;
    crate::hooks::wake();
    let storage = st.storage();
    for k in keys {
        let _ = storage.delete(&k).await;
    }
    Ok(())
}

pub async fn bot_edit(st: &St, bot: &BotCtx, id: &str, body: Option<String>, form: Option<Option<Form>>) -> AppResult<Message> {
    let hub = st.hub.clone();
    let (bid, mid) = (bot.id.clone(), id.to_string());
    let m = st.db.call(move |c| bots::bot_edit(c, &hub, &bid, &mid, body, form)).await?;
    crate::hooks::wake();
    bots::wake();
    Ok(m)
}

pub async fn set_webhook(st: &St, bot: &BotCtx, url: &str, secret: Option<String>) -> AppResult<()> {
    if !url.is_empty() {
        crate::hooks::valid_url(url)?;
    }
    let (bid, url, secret) = (bot.id.clone(), url.to_string(), secret.filter(|s| !s.is_empty()));
    st.db
        .call(move |c| {
            let tx = c.transaction()?;
            if url.is_empty() {
                tx.execute("UPDATE bots SET webhook_url=NULL, webhook_secret=NULL, webhook_error=NULL, webhook_error_ms=NULL WHERE user_id=?1", [&bid])?;
            } else {
                tx.execute("UPDATE bots SET webhook_url=?2, webhook_secret=?3, webhook_error=NULL, webhook_error_ms=NULL WHERE user_id=?1", rusqlite::params![bid, url, secret])?;
                // Updates that were waiting for getUpdates are handed to the webhook.
                let queued: Vec<(i64, String)> = {
                    let mut s = tx.prepare("SELECT id, data FROM bot_updates WHERE bot_id=?1 ORDER BY id")?;
                    let v = s.query_map([&bid], |r| Ok((r.get(0)?, r.get(1)?)))?.collect::<Result<Vec<_>, _>>()?;
                    v
                };
                for (id, data) in queued {
                    crate::hooks::enqueue(&tx, "bot", &bid, &url, secret.as_deref(), "update", &data)?;
                    tx.execute("DELETE FROM bot_updates WHERE id=?1", [id])?;
                }
            }
            tx.commit()?;
            Ok(())
        })
        .await?;
    crate::hooks::wake();
    Ok(())
}

pub async fn delete_webhook(st: &St, bot: &BotCtx, drop_pending: bool) -> AppResult<()> {
    set_webhook(st, bot, "", None).await?;
    if drop_pending {
        let bid = bot.id.clone();
        st.db
            .call(move |c| {
                c.execute("DELETE FROM bot_updates WHERE bot_id=?1", [&bid])?;
                Ok(())
            })
            .await?;
    }
    Ok(())
}

pub async fn webhook_info(st: &St, bot: &BotCtx) -> AppResult<Value> {
    let (hub, bid) = (st.hub.clone(), bot.id.clone());
    let info = st.db.call(move |c| bots::admin_bot(c, &hub, &bid)).await?;
    Ok(match info.webhook {
        Some(w) => {
            let mut v = json!({"url": w.url, "has_custom_certificate": false, "pending_update_count": w.pending_updates});
            if let (Some(e), Some(ms)) = (w.last_error, w.last_error_ms) {
                v["last_error_message"] = json!(e);
                v["last_error_date"] = json!(ms / 1000);
            }
            v
        }
        None => json!({"url": "", "has_custom_certificate": false, "pending_update_count": info.queued_updates}),
    })
}

/// Copies an existing attachment (by file_id) so it can be attached to a new bot message.
async fn clone_attachment(st: &St, bot: &BotCtx, file_id: &str) -> AppResult<Attachment> {
    let (fid, bid) = (file_id.to_string(), bot.id.clone());
    let row = st
        .db
        .call(move |c| {
            let row = files::att_row(c, &fid)?;
            if !files::can_access(c, &fid, &row, &bid, false)? || row.removed {
                return Err(AppError::not_found("file not found"));
            }
            Ok(row)
        })
        .await?;
    let data = st.storage().get(&row.key, None).await?;
    let tmp_dir = st.cfg.data_dir.join("tmp");
    tokio::fs::create_dir_all(&tmp_dir).await.map_err(|e| AppError::internal(e.to_string()))?;
    let id = new_id();
    let tmp = tmp_dir.join(&id);
    let mut f = tokio::fs::File::create(&tmp).await.map_err(|e| AppError::internal(e.to_string()))?;
    let mut hasher = Sha256::new();
    let mut size = 0u64;
    let mut stream = data.stream;
    while let Some(chunk) = stream.next().await {
        let chunk = chunk.map_err(|e| AppError::storage(e.to_string()))?;
        size += chunk.len() as u64;
        hasher.update(&chunk);
        f.write_all(&chunk).await.map_err(|e| AppError::internal(e.to_string()))?;
    }
    f.flush().await.map_err(|e| AppError::internal(e.to_string()))?;
    drop(f);
    files::finalize_upload(st, &bot.id, &id, &row.name, &row.content_type, "file", &tmp, size, &hex::encode(hasher.finalize())).await
}

// ------------------------------------------------------------------ Telegram parameter parsing

pub struct TgFile {
    pub field: String,
    pub name: String,
    pub content_type: String,
    pub tmp: PathBuf,
    pub size: u64,
    pub sha: String,
}

#[derive(Default)]
pub struct TgParams {
    pub map: Map<String, Value>,
    pub files: Vec<TgFile>,
}

impl Drop for TgParams {
    fn drop(&mut self) {
        for f in &self.files {
            let _ = std::fs::remove_file(&f.tmp);
        }
    }
}

impl TgParams {
    pub fn str(&self, k: &str) -> Option<String> {
        match self.map.get(k)? {
            Value::String(s) => Some(s.clone()),
            Value::Number(n) => Some(n.to_string()),
            Value::Bool(b) => Some(b.to_string()),
            _ => None,
        }
    }
    pub fn i64(&self, k: &str) -> Option<i64> {
        self.str(k).and_then(|s| s.parse().ok())
    }
    pub fn bool(&self, k: &str) -> bool {
        matches!(self.map.get(k), Some(Value::Bool(true))) || matches!(self.str(k).as_deref(), Some("true") | Some("1"))
    }
    /// A structured value that may arrive as a JSON object or as a JSON string (form fields, multipart).
    pub fn json(&self, k: &str) -> Option<Value> {
        match self.map.get(k)? {
            Value::String(s) => serde_json::from_str(s).ok(),
            Value::Null => None,
            v => Some(v.clone()),
        }
    }
}

impl FromRequest<St> for TgParams {
    type Rejection = AppError;
    async fn from_request(req: Request, st: &St) -> Result<Self, Self::Rejection> {
        let mut out = TgParams::default();
        if let Some(q) = req.uri().query() {
            for (k, v) in serde_urlencoded::from_str::<Vec<(String, String)>>(q).unwrap_or_default() {
                out.map.insert(k, Value::String(v));
            }
        }
        let ct = req.headers().get(header::CONTENT_TYPE).and_then(|v| v.to_str().ok()).unwrap_or("").to_lowercase();
        if ct.starts_with("multipart/form-data") {
            let mut mp = Multipart::from_request(req, st).await.map_err(|e| AppError::validation(format!("bad multipart body: {e}")))?;
            let tmp_dir = st.cfg.data_dir.join("tmp");
            tokio::fs::create_dir_all(&tmp_dir).await.map_err(|e| AppError::internal(e.to_string()))?;
            while let Some(mut field) = mp.next_field().await.map_err(|e| AppError::validation(format!("bad multipart body: {e}")))? {
                let fname = field.name().unwrap_or("").to_string();
                match field.file_name().map(files::safe_name) {
                    Some(name) => {
                        let ctype = field
                            .content_type()
                            .map(|c| c.to_string())
                            .filter(|c| c != "application/octet-stream")
                            .unwrap_or_else(|| mime_guess::from_path(&name).first_or_octet_stream().to_string());
                        let tmp = tmp_dir.join(new_id());
                        let mut f = tokio::fs::File::create(&tmp).await.map_err(|e| AppError::internal(e.to_string()))?;
                        let (mut size, mut hasher) = (0u64, Sha256::new());
                        // Registered first so the temp file is removed on every error path.
                        out.files.push(TgFile { field: fname, name, content_type: ctype, tmp: tmp.clone(), size: 0, sha: String::new() });
                        while let Some(chunk) = field.chunk().await.map_err(|e| AppError::validation(format!("upload interrupted: {e}")))? {
                            size += chunk.len() as u64;
                            if size > st.cfg.max_upload_bytes {
                                return Err(AppError::new(ErrorCode::FileTooLarge, format!("file exceeds the {} MB limit", st.cfg.max_upload_bytes / 1024 / 1024)));
                            }
                            hasher.update(&chunk);
                            f.write_all(&chunk).await.map_err(|e| AppError::internal(e.to_string()))?;
                        }
                        f.flush().await.map_err(|e| AppError::internal(e.to_string()))?;
                        if let Some(last) = out.files.last_mut() {
                            last.size = size;
                            last.sha = hex::encode(hasher.finalize());
                        }
                    }
                    None => {
                        let text = field.text().await.map_err(|e| AppError::validation(format!("bad multipart field: {e}")))?;
                        out.map.insert(fname, Value::String(text));
                    }
                }
            }
        } else {
            let bytes = axum::body::to_bytes(req.into_body(), 1024 * 1024).await.map_err(|_| AppError::validation("request body too large"))?;
            if ct.starts_with("application/json") {
                if !bytes.is_empty() {
                    match serde_json::from_slice::<Value>(&bytes) {
                        Ok(Value::Object(m)) => out.map.extend(m),
                        _ => return Err(AppError::validation("the JSON body must be an object")),
                    }
                }
            } else if !bytes.is_empty() {
                for (k, v) in serde_urlencoded::from_bytes::<Vec<(String, String)>>(&bytes).unwrap_or_default() {
                    out.map.insert(k, Value::String(v));
                }
            }
        }
        Ok(out)
    }
}

fn convert_parse_mode(text: &str, mode: Option<&str>) -> String {
    match mode.map(|m| m.to_lowercase()).as_deref() {
        Some("html") => {
            let mut s = text.to_string();
            for (tag, rep) in [("b", "*"), ("strong", "*"), ("i", "_"), ("em", "_"), ("s", "~"), ("strike", "~"), ("del", "~"), ("code", "`")] {
                s = s.replace(&format!("<{tag}>"), rep).replace(&format!("</{tag}>"), rep);
            }
            s = s.replace("<pre>", "```\n").replace("</pre>", "\n```");
            // <a href="u">t</a> becomes "t (u)".
            while let Some(a) = s.find("<a href=\"") {
                let Some(q) = s[a + 9..].find('"') else { break };
                let url = s[a + 9..a + 9 + q].to_string();
                let Some(gt) = s[a..].find('>') else { break };
                let Some(close) = s[a..].find("</a>") else { break };
                let label = s[a + gt + 1..a + close].to_string();
                s.replace_range(a..a + close + 4, &format!("{label} ({url})"));
            }
            s.replace("&lt;", "<").replace("&gt;", ">").replace("&quot;", "\"").replace("&amp;", "&")
        }
        Some("markdownv2") => {
            // MarkdownV2 escapes punctuation with a backslash.
            let mut out = String::new();
            let mut it = text.chars().peekable();
            while let Some(c) = it.next() {
                if c == '\\' {
                    if let Some(n) = it.next() {
                        out.push(n);
                    }
                } else {
                    out.push(c);
                }
            }
            out
        }
        _ => text.to_string(),
    }
}

// ------------------------------------------------------------------ Telegram dispatch

fn tg_error(e: AppError) -> Response {
    let (status, desc) = match e.code {
        ErrorCode::Unauthorized | ErrorCode::AccountDisabled | ErrorCode::SessionRevoked => (401, "Unauthorized".to_string()),
        ErrorCode::Forbidden => (403, format!("Forbidden: {}", e.message)),
        ErrorCode::Conflict => (409, e.message.clone()),
        ErrorCode::RateLimited => (429, "Too Many Requests: retry later".to_string()),
        ErrorCode::FileTooLarge => (413, format!("Request Entity Too Large: {}", e.message)),
        ErrorCode::Internal | ErrorCode::StorageError => (500, "Internal Server Error".to_string()),
        _ => (400, format!("Bad Request: {}", e.message)),
    };
    (StatusCode::from_u16(status).unwrap_or(StatusCode::BAD_REQUEST), Json(json!({"ok": false, "error_code": status, "description": desc}))).into_response()
}

fn tg_ok(v: Value) -> Response {
    Json(json!({"ok": true, "result": v})).into_response()
}

pub async fn telegram(State(st): State<St>, Path((token, method)): Path<(String, String)>, params: Result<TgParams, AppError>) -> Response {
    let bot = match auth_token(&st, &token).await {
        Ok(b) => b,
        Err(e) => return tg_error(e),
    };
    let params = match params {
        Ok(p) => p,
        Err(e) => return tg_error(e),
    };
    match run_method(&st, &bot, &method.to_lowercase(), params).await {
        Ok(v) => tg_ok(v),
        Err(e) => tg_error(e),
    }
}

async fn tg_message_json(st: &St, m: Message) -> AppResult<Value> {
    Ok(st.db.call(move |c| Ok(bots::tg_message(c, &m))).await?)
}

fn form_param(p: &TgParams) -> AppResult<Option<Form>> {
    if let Some(v) = p.json("form") {
        return serde_json::from_value::<Form>(v).map(Some).map_err(|e| AppError::validation(format!("form: {e}")));
    }
    if let Some(v) = p.json("reply_markup") {
        return Ok(bots::form_from_markup(&v));
    }
    Ok(None)
}

async fn chat_of(st: &St, bot: &BotCtx, p: &TgParams) -> AppResult<String> {
    let chat = p.str("chat_id").ok_or_else(|| AppError::validation("chat_id is required"))?;
    let (bid, chat2) = (bot.id.clone(), chat);
    st.db.call(move |c| bots::resolve_chat(c, &bid, &chat2)).await
}

async fn run_method(st: &St, bot: &BotCtx, method: &str, p: TgParams) -> AppResult<Value> {
    match method {
        "getme" => {
            let (hub, id) = (st.hub.clone(), bot.id.clone());
            let prof = st.db.call(move |c| bots::profile(c, &hub, &id)).await?;
            Ok(json!({"id": prof.user.id, "is_bot": true, "first_name": prof.user.display_name, "username": prof.user.username, "can_join_groups": false, "supports_inline_queries": false}))
        }
        "sendchataction" | "answercallbackquery" => Ok(json!(true)),
        "getchat" => {
            let chat = chat_of(st, bot, &p).await?;
            Ok(st.db.call(move |c| Ok(bots::tg_chat(c, &chat))).await?)
        }
        "sendmessage" => {
            let conv = chat_of(st, bot, &p).await?;
            let text = p.str("text").ok_or_else(|| AppError::validation("text is empty"))?;
            let m = bot_send(
                st,
                bot,
                NewMsg {
                    conv_id: conv,
                    body: convert_parse_mode(&text, p.str("parse_mode").as_deref()),
                    reply_to: p.str("reply_to_message_id").or_else(|| p.json("reply_parameters").and_then(|v| v["message_id"].as_str().map(str::to_string))),
                    thread_root: p.str("message_thread_id"),
                    form: form_param(&p)?,
                    attachment_ids: vec![],
                    client_id: p.str("client_id"),
                    mentions: vec![],
                },
            )
            .await?
            .1;
            tg_message_json(st, m).await
        }
        "senddocument" | "sendphoto" | "sendvideo" | "sendaudio" | "sendvoice" | "sendanimation" | "sendvideonote" => {
            let conv = chat_of(st, bot, &p).await?;
            let field = match method {
                "senddocument" => "document",
                "sendphoto" => "photo",
                "sendvideo" => "video",
                "sendaudio" => "audio",
                "sendvoice" => "voice",
                "sendanimation" => "animation",
                _ => "video_note",
            };
            let att = if let Some(f) = p.files.iter().find(|f| f.field == field).or(p.files.first()) {
                if f.size == 0 {
                    return Err(AppError::validation("the file is empty"));
                }
                // Hand the temp file over: finalize moves it into storage.
                let staged = st.cfg.data_dir.join("tmp").join(new_id());
                tokio::fs::copy(&f.tmp, &staged).await.map_err(|e| AppError::internal(e.to_string()))?;
                files::finalize_upload(st, &bot.id, &new_id(), &f.name, &f.content_type, "file", &staged, f.size, &f.sha).await?
            } else if let Some(v) = p.str(field) {
                if v.starts_with("http://") || v.starts_with("https://") {
                    return Err(AppError::validation("sending a file by URL is not supported; upload it or use a file_id"));
                }
                clone_attachment(st, bot, &v).await?
            } else {
                return Err(AppError::validation(format!("{field} is required (multipart file or file_id)")));
            };
            let m = bot_send(
                st,
                bot,
                NewMsg {
                    conv_id: conv,
                    body: convert_parse_mode(&p.str("caption").unwrap_or_default(), p.str("parse_mode").as_deref()),
                    reply_to: p.str("reply_to_message_id"),
                    thread_root: p.str("message_thread_id"),
                    form: form_param(&p)?,
                    attachment_ids: vec![att.id],
                    client_id: p.str("client_id"),
                    mentions: vec![],
                },
            )
            .await?
            .1;
            tg_message_json(st, m).await
        }
        "editmessagetext" | "editmessagecaption" | "editmessagereplymarkup" => {
            let id = p.str("message_id").ok_or_else(|| AppError::validation("message_id is required"))?;
            let text = p.str("text").or_else(|| p.str("caption")).map(|t| convert_parse_mode(&t, p.str("parse_mode").as_deref()));
            let form = if p.map.contains_key("form") || p.map.contains_key("reply_markup") { Some(form_param(&p)?) } else { None };
            if text.is_none() && form.is_none() {
                return Err(AppError::validation("text is required"));
            }
            let m = bot_edit(st, bot, &id, text, form).await?;
            tg_message_json(st, m).await
        }
        "deletemessage" => {
            let id = p.str("message_id").ok_or_else(|| AppError::validation("message_id is required"))?;
            bot_delete(st, bot, &id).await?;
            Ok(json!(true))
        }
        "setwebhook" => {
            let url = p.str("url").unwrap_or_default();
            set_webhook(st, bot, &url, p.str("secret_token")).await?;
            if p.bool("drop_pending_updates") {
                let bid = bot.id.clone();
                st.db.call(move |c| { c.execute("DELETE FROM bot_updates WHERE bot_id=?1", [&bid])?; Ok(()) }).await?;
            }
            Ok(json!(true))
        }
        "deletewebhook" => {
            delete_webhook(st, bot, p.bool("drop_pending_updates")).await?;
            Ok(json!(true))
        }
        "getwebhookinfo" => webhook_info(st, bot).await,
        "getupdates" => {
            let ups = bots::fetch_updates(st, &bot.id, p.i64("offset"), p.i64("limit").unwrap_or(100), p.i64("timeout").unwrap_or(0).max(0) as u64).await?;
            Ok(Value::Array(ups))
        }
        "getfile" => {
            let id = p.str("file_id").ok_or_else(|| AppError::validation("file_id is required"))?;
            let (fid, bid) = (id.clone(), bot.id.clone());
            let row = st
                .db
                .call(move |c| {
                    let row = files::att_row(c, &fid)?;
                    if !files::can_access(c, &fid, &row, &bid, false)? || row.removed {
                        return Err(AppError::not_found("file not found"));
                    }
                    Ok(row)
                })
                .await?;
            Ok(json!({"file_id": id, "file_unique_id": id, "file_size": row.size, "file_path": format!("{}/{}", id, row.name)}))
        }
        "setapphome" => {
            let v = p.json("home").or_else(|| p.json("blocks").map(|b| json!({"blocks": b}))).ok_or_else(|| AppError::validation("home is required"))?;
            let home: BotHome = serde_json::from_value(v).map_err(|e| AppError::validation(format!("home: {e}")))?;
            let bid = bot.id.clone();
            st.db.call(move |c| bots::set_home(c, &bid, &home)).await?;
            Ok(json!(true))
        }
        _ => Err(AppError::new(ErrorCode::NotFound, format!("method {method} is not supported"))),
    }
}

/// `GET /file/bot<token>/<file_id>/<name>`: download a file the bot may see.
pub async fn telegram_file(State(st): State<St>, Path((token, path)): Path<(String, String)>, headers: HeaderMap) -> Response {
    let bot = match auth_token(&st, &token).await {
        Ok(b) => b,
        Err(e) => return tg_error(e),
    };
    let id = path.split('/').next().unwrap_or("").to_string();
    match files::serve_attachment(&st, &id, &bot.id, false, &headers, true).await {
        Ok(r) => r,
        Err(e) => tg_error(e),
    }
}

// ------------------------------------------------------------------ native REST

pub async fn me(State(st): State<St>, BotAuth(bot): BotAuth) -> AppResult<Json<BotProfile>> {
    let hub = st.hub.clone();
    Ok(Json(st.db.call(move |c| bots::profile(c, &hub, &bot.id)).await?))
}

pub async fn conversations(State(st): State<St>, BotAuth(bot): BotAuth) -> AppResult<Json<Vec<Conversation>>> {
    Ok(Json(
        st.db
            .call(move |c| user_conversation_ids(c, &bot.id)?.iter().map(|i| load_conversation(c, i)).collect::<AppResult<Vec<_>>>())
            .await?,
    ))
}

#[derive(serde::Deserialize)]
pub struct HistQ {
    before: Option<i64>,
    after: Option<i64>,
    limit: Option<i64>,
}

pub async fn history(State(st): State<St>, BotAuth(bot): BotAuth, Path(id): Path<String>, Query(q): Query<HistQ>) -> AppResult<Json<HistoryResponse>> {
    Ok(Json(
        st.db
            .call(move |c| {
                member_conv(c, &id, &bot.id)?;
                crate::queries::history(c, &id, &bot.id, &HistoryQuery { before_seq: q.before, after_seq: q.after, around_ts: None, around_seq: None, limit: q.limit.unwrap_or(50) })
            })
            .await?,
    ))
}

#[derive(serde::Deserialize)]
pub struct NativeSend {
    pub conv_id: String,
    #[serde(default)]
    pub body: String,
    #[serde(default)]
    pub reply_to: Option<String>,
    #[serde(default)]
    pub thread_root: Option<String>,
    #[serde(default)]
    pub form: Option<Form>,
    #[serde(default)]
    pub attachment_ids: Vec<String>,
    #[serde(default)]
    pub client_id: Option<String>,
    #[serde(default)]
    pub mentions: Vec<String>,
}

pub async fn send(State(st): State<St>, BotAuth(bot): BotAuth, Json(r): Json<NativeSend>) -> AppResult<(StatusCode, Json<Message>)> {
    let (created, m) = bot_send(
        &st,
        &bot,
        NewMsg { conv_id: r.conv_id, body: r.body, reply_to: r.reply_to, thread_root: r.thread_root, form: r.form, attachment_ids: r.attachment_ids, client_id: r.client_id, mentions: r.mentions },
    )
    .await?;
    Ok((if created { StatusCode::CREATED } else { StatusCode::OK }, Json(m)))
}

#[derive(serde::Deserialize)]
pub struct NativeEdit {
    #[serde(default)]
    pub body: Option<String>,
    /// Replace the form; `null` is ignored, use `clear_form` to remove it.
    #[serde(default)]
    pub form: Option<Form>,
    #[serde(default)]
    pub clear_form: bool,
}

pub async fn edit(State(st): State<St>, BotAuth(bot): BotAuth, Path(id): Path<String>, Json(r): Json<NativeEdit>) -> AppResult<Json<Message>> {
    let form = if r.clear_form { Some(None) } else { r.form.map(Some) };
    Ok(Json(bot_edit(&st, &bot, &id, r.body, form).await?))
}

pub async fn delete(State(st): State<St>, BotAuth(bot): BotAuth, Path(id): Path<String>) -> AppResult<Json<Value>> {
    bot_delete(&st, &bot, &id).await?;
    Ok(Json(json!({"ok": true})))
}

pub async fn upload(State(st): State<St>, BotAuth(bot): BotAuth, Query(q): Query<UploadQuery>, headers: HeaderMap, body: Body) -> AppResult<(StatusCode, Json<UploadResponse>)> {
    files::upload_stream(&st, &bot.id, UploadQuery { name: q.name, purpose: None }, headers, body).await
}

pub async fn download(State(st): State<St>, BotAuth(bot): BotAuth, Path(id): Path<String>, headers: HeaderMap) -> AppResult<Response> {
    files::serve_attachment(&st, &id, &bot.id, false, &headers, true).await
}

#[derive(serde::Deserialize)]
pub struct UpdatesQ {
    offset: Option<i64>,
    limit: Option<i64>,
    timeout: Option<u64>,
}

pub async fn updates(State(st): State<St>, BotAuth(bot): BotAuth, Query(q): Query<UpdatesQ>) -> AppResult<Json<Value>> {
    Ok(Json(Value::Array(bots::fetch_updates(&st, &bot.id, q.offset, q.limit.unwrap_or(100), q.timeout.unwrap_or(0)).await?)))
}

#[derive(serde::Deserialize)]
pub struct WebhookBody {
    pub url: String,
    #[serde(default)]
    pub secret_token: Option<String>,
}

pub async fn put_webhook(State(st): State<St>, BotAuth(bot): BotAuth, Json(r): Json<WebhookBody>) -> AppResult<Json<Value>> {
    set_webhook(&st, &bot, &r.url, r.secret_token).await?;
    Ok(Json(webhook_info(&st, &bot).await?))
}

pub async fn get_webhook(State(st): State<St>, BotAuth(bot): BotAuth) -> AppResult<Json<Value>> {
    Ok(Json(webhook_info(&st, &bot).await?))
}

pub async fn remove_webhook(State(st): State<St>, BotAuth(bot): BotAuth) -> AppResult<Json<Value>> {
    delete_webhook(&st, &bot, false).await?;
    Ok(Json(json!({"ok": true})))
}

pub async fn get_home(State(st): State<St>, BotAuth(bot): BotAuth) -> AppResult<Json<BotHome>> {
    let hub = st.hub.clone();
    Ok(Json(st.db.call(move |c| bots::profile(c, &hub, &bot.id)).await?.home))
}

pub async fn put_home(State(st): State<St>, BotAuth(bot): BotAuth, Json(home): Json<BotHome>) -> AppResult<Json<BotHome>> {
    let h2 = home.clone();
    st.db.call(move |c| bots::set_home(c, &bot.id, &h2)).await?;
    Ok(Json(home))
}
