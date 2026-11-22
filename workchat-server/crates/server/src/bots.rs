//! SV-15 / SV-17 bots: accounts, tokens, update queue, Telegram-shaped JSON, forms and App Home.
//! HTTP front ends live in `routes/botapi.rs` (bot side) and `routes/botadmin.rs` (admin side).

use crate::auth::random_token;
use crate::db::{new_id, now_ms};
use crate::emit::Emitter;
use crate::error::{AppError, AppResult};
use crate::hub::Hub;
use crate::queries::*;
use crate::state::St;
use proto::*;
use rusqlite::{params, Connection, OptionalExtension};
use serde_json::{json, Value};
use std::sync::OnceLock;
use tokio::sync::Notify;

static UPDATES: OnceLock<Notify> = OnceLock::new();

fn updates_notify() -> &'static Notify {
    UPDATES.get_or_init(Notify::new)
}

/// Wakes long-polling `getUpdates` calls.
pub fn wake() {
    updates_notify().notify_waiters();
}

pub fn new_token() -> String {
    random_token("bt_", 30)
}

#[derive(Clone, Debug)]
pub struct BotCtx {
    pub id: String,
}

/// Resolves a bot token. Disabled or deleted bots are refused.
pub fn lookup(conn: &Connection, token: &str) -> AppResult<BotCtx> {
    let row: Option<(String, i64, i64)> = conn
        .query_row(
            "SELECT b.user_id, u.disabled, u.deleted FROM bots b JOIN users u ON u.id=b.user_id WHERE b.token=?1",
            [token],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
        )
        .optional()?;
    match row {
        None => Err(AppError::new(ErrorCode::Unauthorized, "invalid bot token")),
        Some((_, d, x)) if d != 0 || x != 0 => Err(AppError::new(ErrorCode::AccountDisabled, "this bot is disabled")),
        Some((id, _, _)) => Ok(BotCtx { id }),
    }
}

/// Creates the user row of a bot or webhook identity (no password, `is_bot`), and announces it.
pub fn create_bot_user(conn: &Connection, hub: &Hub, username: &str, display_name: &str, em: &mut Emitter) -> AppResult<UserRow> {
    if !rules::valid_username(username) {
        return Err(AppError::validation("username must be 2-32 characters: letters, digits, . _ -"));
    }
    let display = display_name.trim();
    if display.is_empty() || display.chars().count() > 80 {
        return Err(AppError::validation("display name must be 1-80 characters"));
    }
    let taken: i64 = conn.query_row("SELECT COUNT(*) FROM users WHERE username=?1", [username], |r| r.get(0))?;
    if taken > 0 {
        return Err(AppError::conflict("username already taken"));
    }
    let id = new_id();
    // The password hash can never match: it is not a valid argon2 string.
    conn.execute(
        "INSERT INTO users(id,username,display_name,password_hash,password_set,role,is_bot,created_ms) VALUES(?1,?2,?3,'!bot',0,'member',1,?4)",
        params![id, username, display, now_ms()],
    )?;
    let row = get_user(conn, &id)?;
    let user = to_user(&row, hub);
    for uid in user_ids_all_active(conn)? {
        em.emit(conn, &uid, EventData::UserUpsert { user: user.clone() })?;
    }
    Ok(row)
}

pub fn add_to_conversation(conn: &Connection, hub: &Hub, conv_id: &str, user_id: &str, em: &mut Emitter) -> AppResult<()> {
    let cr = conv_row(conn, conv_id)?;
    if !matches!(cr.kind, ConversationKind::Channel | ConversationKind::GroupDm) {
        return Err(AppError::validation("bots can only be added to channels and group DMs"));
    }
    let u = get_user(conn, user_id)?;
    if u.disabled || u.deleted {
        return Err(AppError::validation("cannot add a disabled account"));
    }
    conn.execute("INSERT OR IGNORE INTO members(conv_id,user_id,role,joined_ms) VALUES(?1,?2,'member',?3)", params![conv_id, user_id, now_ms()])?;
    let _ = hub;
    crate::routes::chat::upsert_for_members(conn, em, conv_id)?;
    Ok(())
}

pub fn admin_bot(conn: &Connection, hub: &Hub, user_id: &str) -> AppResult<AdminBot> {
    let row = get_user(conn, user_id)?;
    let (description, token, created_ms, wurl, wsecret, werr, werr_ms): (String, String, i64, Option<String>, Option<String>, Option<String>, Option<i64>) = conn
        .query_row(
            "SELECT description, token, created_ms, webhook_url, webhook_secret, webhook_error, webhook_error_ms FROM bots WHERE user_id=?1",
            [user_id],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?, r.get(5)?, r.get(6)?)),
        )
        .optional()?
        .ok_or_else(|| AppError::not_found("bot not found"))?;
    let queued: i64 = conn.query_row("SELECT COUNT(*) FROM bot_updates WHERE bot_id=?1", [user_id], |r| r.get(0))?;
    let mut st = conn.prepare(
        "SELECT c.id, COALESCE(c.name,''), c.kind FROM members m JOIN conversations c ON c.id=m.conv_id WHERE m.user_id=?1 ORDER BY c.name",
    )?;
    let conversations = st
        .query_map([user_id], |r| {
            Ok(BotConvRef { id: r.get(0)?, name: r.get(1)?, kind: ConversationKind::parse(&r.get::<_, String>(2)?).unwrap_or(ConversationKind::Channel) })
        })?
        .collect::<Result<Vec<_>, _>>()?;
    let webhook = match wurl.filter(|u| !u.is_empty()) {
        Some(url) => {
            let pending: i64 = conn.query_row("SELECT COUNT(*) FROM deliveries WHERE kind='bot' AND ref_id=?1 AND state='pending'", [user_id], |r| r.get(0))?;
            Some(BotWebhookInfo { url, has_secret: wsecret.map(|s| !s.is_empty()).unwrap_or(false), pending_updates: pending, last_error: werr, last_error_ms: werr_ms })
        }
        None => None,
    };
    Ok(AdminBot { user: to_user(&row, hub), description, token, created_ms, conversations, webhook, queued_updates: queued })
}

pub fn profile(conn: &Connection, hub: &Hub, user_id: &str) -> AppResult<BotProfile> {
    let row = get_user(conn, user_id)?;
    let (description, home): (String, String) = conn
        .query_row("SELECT description, home FROM bots WHERE user_id=?1", [user_id], |r| Ok((r.get(0)?, r.get(1)?)))
        .optional()?
        .ok_or_else(|| AppError::not_found("bot not found"))?;
    Ok(BotProfile { user: to_user(&row, hub), description, home: serde_json::from_str(&home).unwrap_or_default() })
}

pub fn set_home(conn: &Connection, bot_id: &str, home: &BotHome) -> AppResult<()> {
    if home.blocks.len() > 100 {
        return Err(AppError::validation("too many blocks"));
    }
    for b in &home.blocks {
        if let HomeBlock::Form { form } = b {
            form.check_shape().map_err(AppError::validation)?;
        }
    }
    conn.execute("UPDATE bots SET home=?2 WHERE user_id=?1", params![bot_id, serde_json::to_string(home)?])?;
    Ok(())
}

// ------------------------------------------------------------------ Telegram-shaped JSON

pub fn tg_user(conn: &Connection, id: &str) -> Value {
    conn.query_row("SELECT username, display_name, is_bot FROM users WHERE id=?1", [id], |r| {
        Ok(json!({"id": id, "is_bot": r.get::<_, i64>(2)? != 0, "first_name": r.get::<_, String>(1)?, "username": r.get::<_, String>(0)?}))
    })
    .unwrap_or_else(|_| json!({"id": id, "is_bot": false, "first_name": "Unknown"}))
}

pub fn tg_chat(conn: &Connection, conv_id: &str) -> Value {
    conn.query_row("SELECT kind, name FROM conversations WHERE id=?1", [conv_id], |r| {
        let kind: String = r.get(0)?;
        let name: Option<String> = r.get(1)?;
        let ty = match kind.as_str() {
            "dm" | "self" => "private",
            "group_dm" => "group",
            _ => "supergroup",
        };
        Ok(match name {
            Some(n) => json!({"id": conv_id, "type": ty, "title": n}),
            None => json!({"id": conv_id, "type": ty}),
        })
    })
    .unwrap_or_else(|_| json!({"id": conv_id, "type": "private"}))
}

fn tg_file(a: &Attachment) -> Value {
    json!({"file_id": a.id, "file_unique_id": a.id, "file_name": a.name, "mime_type": a.content_type, "file_size": a.size})
}

pub fn tg_message(conn: &Connection, m: &Message) -> Value {
    let mut v = json!({
        "message_id": m.id,
        "from": tg_user(conn, &m.sender_id),
        "chat": tg_chat(conn, &m.conv_id),
        "date": m.ts / 1000,
    });
    let o = v.as_object_mut().expect("object");
    if m.attachments.is_empty() {
        o.insert("text".into(), json!(m.body));
    } else {
        if !m.body.is_empty() {
            o.insert("caption".into(), json!(m.body));
        }
        let a = &m.attachments[0];
        let key = if a.content_type.starts_with("image/") {
            "photo"
        } else if a.content_type.starts_with("video/") {
            "video"
        } else if a.content_type.starts_with("audio/ogg") || a.content_type.starts_with("audio/opus") {
            "voice"
        } else if a.content_type.starts_with("audio/") {
            "audio"
        } else {
            "document"
        };
        if key == "photo" {
            o.insert("photo".into(), json!([tg_file(a)]));
        } else {
            o.insert(key.into(), tg_file(a));
        }
        o.insert("attachments".into(), json!(m.attachments.iter().map(tg_file).collect::<Vec<_>>()));
    }
    if let Some(r) = &m.reply_to {
        o.insert("reply_to_message".into(), json!({"message_id": r}));
    }
    if let Some(t) = &m.thread_root {
        o.insert("message_thread_id".into(), json!(t));
    }
    if let Some(e) = m.edited_ms {
        o.insert("edit_date".into(), json!(e / 1000));
    }
    if let Some(f) = &m.form {
        o.insert("form".into(), serde_json::to_value(f).unwrap_or(Value::Null));
    }
    v
}

// ------------------------------------------------------------------ updates queue

/// Stores an update for the bot, or hands it to the bot's webhook. Returns nothing; wakes pollers.
pub fn push_update(conn: &Connection, bot_id: &str, key: &str, payload: Value) -> AppResult<()> {
    conn.execute("INSERT INTO bot_updates(bot_id,data,created_ms) VALUES(?1,'',?2)", params![bot_id, now_ms()])?;
    let uid = conn.last_insert_rowid();
    let mut obj = serde_json::Map::new();
    obj.insert("update_id".into(), json!(uid));
    obj.insert(key.into(), payload);
    let data = Value::Object(obj).to_string();
    let hook: Option<(Option<String>, Option<String>)> =
        conn.query_row("SELECT webhook_url, webhook_secret FROM bots WHERE user_id=?1", [bot_id], |r| Ok((r.get(0)?, r.get(1)?))).optional()?;
    match hook {
        Some((Some(url), secret)) if !url.is_empty() => {
            crate::hooks::enqueue(conn, "bot", bot_id, &url, secret.as_deref(), key, &data)?;
            conn.execute("DELETE FROM bot_updates WHERE id=?1", [uid])?;
        }
        _ => {
            conn.execute("UPDATE bot_updates SET data=?2 WHERE id=?1", params![uid, data])?;
        }
    }
    Ok(())
}

/// Queues `message` / `edited_message` updates for every bot that is a member of the conversation
/// (except the sender). Messages written by bots are not delivered to other bots.
pub fn on_message(conn: &Connection, key: &str, m: &Message, _kind: ConversationKind) -> AppResult<()> {
    let sender_is_bot: bool = conn
        .query_row("SELECT is_bot FROM users WHERE id=?1", [&m.sender_id], |r| r.get::<_, i64>(0))
        .map(|v| v != 0)
        .unwrap_or(false);
    if sender_is_bot {
        return Ok(());
    }
    let mut st = conn.prepare(
        "SELECT m.user_id FROM members m JOIN bots b ON b.user_id=m.user_id JOIN users u ON u.id=m.user_id
         WHERE m.conv_id=?1 AND u.disabled=0 AND u.deleted=0",
    )?;
    let bots = st.query_map([&m.conv_id], |r| r.get::<_, String>(0))?.collect::<Result<Vec<_>, _>>()?;
    if bots.is_empty() {
        return Ok(());
    }
    let payload = tg_message(conn, m);
    for b in bots {
        push_update(conn, &b, key, payload.clone())?;
    }
    Ok(())
}

pub async fn fetch_updates(st: &St, bot_id: &str, offset: Option<i64>, limit: i64, timeout_secs: u64) -> AppResult<Vec<Value>> {
    let deadline = tokio::time::Instant::now() + std::time::Duration::from_secs(timeout_secs.min(50));
    loop {
        let (b, off, lim) = (bot_id.to_string(), offset, limit.clamp(1, 100));
        let got: Vec<Value> = st
            .db
            .call(move |c| {
                let hook: Option<String> = c.query_row("SELECT webhook_url FROM bots WHERE user_id=?1", [&b], |r| r.get(0))?;
                if hook.map(|h| !h.is_empty()).unwrap_or(false) {
                    return Err(AppError::conflict("Conflict: can't use getUpdates method while a webhook is active; use deleteWebhook to delete the webhook first"));
                }
                if let Some(o) = off {
                    c.execute("DELETE FROM bot_updates WHERE bot_id=?1 AND id<?2", params![b, o])?;
                }
                let mut stmt = c.prepare("SELECT data FROM bot_updates WHERE bot_id=?1 AND id>=?2 ORDER BY id LIMIT ?3")?;
                let rows = stmt.query_map(params![b, off.unwrap_or(0), lim], |r| r.get::<_, String>(0))?.collect::<Result<Vec<_>, _>>()?;
                Ok(rows.into_iter().filter_map(|s| serde_json::from_str(&s).ok()).collect())
            })
            .await?;
        if !got.is_empty() || tokio::time::Instant::now() >= deadline || st.is_stopped() {
            return Ok(got);
        }
        let notified = updates_notify().notified();
        tokio::pin!(notified);
        notified.as_mut().enable();
        // Re-check after enabling to avoid missing an update that arrived in between.
        let b2 = bot_id.to_string();
        let off2 = offset;
        let any: bool = st
            .db
            .call(move |c| Ok(c.query_row("SELECT COUNT(*) FROM bot_updates WHERE bot_id=?1 AND id>=?2", params![b2, off2.unwrap_or(0)], |r| r.get::<_, i64>(0))? > 0))
            .await?;
        if any {
            continue;
        }
        let _ = tokio::time::timeout_at(deadline, notified).await;
    }
}

// ------------------------------------------------------------------ sending, editing

/// Resolves a Telegram `chat_id`: a conversation id the bot belongs to, or a user id with an
/// existing DM with the bot.
pub fn resolve_chat(conn: &Connection, bot_id: &str, chat_id: &str) -> AppResult<String> {
    if is_member(conn, chat_id, bot_id).unwrap_or(false) {
        return Ok(chat_id.to_string());
    }
    let mut ids = [bot_id.to_string(), chat_id.to_string()];
    ids.sort();
    let key = format!("dm:{}:{}", ids[0], ids[1]);
    if let Some(id) = conn.query_row("SELECT id FROM conversations WHERE dm_key=?1", [&key], |r| r.get::<_, String>(0)).optional()? {
        return Ok(id);
    }
    Err(AppError::not_found("chat not found or the bot is not a member"))
}

/// Telegram `reply_markup.inline_keyboard` as a form of buttons.
pub fn form_from_markup(markup: &Value) -> Option<Form> {
    let rows = markup.get("inline_keyboard")?.as_array()?;
    let mut buttons = vec![];
    for row in rows {
        for b in row.as_array()? {
            let text = b.get("text")?.as_str()?.to_string();
            let id = b.get("callback_data").and_then(|v| v.as_str()).map(str::to_string).unwrap_or_else(|| format!("b{}", buttons.len() + 1));
            buttons.push(FormButton { id, label: text, style: None, form: None });
        }
    }
    if buttons.is_empty() {
        return None;
    }
    Some(Form { id: format!("kb-{}", &new_id()[..8]), title: None, description: None, fields: vec![], buttons, submit_label: None })
}

pub fn bot_edit(
    conn: &mut Connection,
    hub: &Hub,
    bot_id: &str,
    message_id: &str,
    body: Option<String>,
    form: Option<Option<Form>>,
) -> AppResult<Message> {
    let (conv_id, sender, deleted): (String, String, i64) = conn
        .query_row("SELECT conv_id, sender_id, deleted FROM messages WHERE id=?1", [message_id], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))
        .optional()?
        .ok_or_else(|| AppError::not_found("message not found"))?;
    if sender != bot_id || !is_member(conn, &conv_id, bot_id)? {
        return Err(AppError::forbidden("a bot can only edit its own messages"));
    }
    if deleted != 0 {
        return Err(AppError::validation("deleted messages cannot be edited"));
    }
    if let Some(Some(f)) = &form {
        f.check_shape().map_err(AppError::validation)?;
    }
    if let Some(b) = &body {
        if b.trim().is_empty() && form.is_none() {
            return Err(AppError::validation("message text is empty"));
        }
        if b.chars().count() > 20_000 {
            return Err(AppError::validation("message too long"));
        }
    }
    let tx = conn.transaction()?;
    let now = now_ms();
    let (old_body, version): (String, i64) = tx.query_row("SELECT body, version FROM messages WHERE id=?1", [message_id], |r| Ok((r.get(0)?, r.get(1)?)))?;
    tx.execute("INSERT OR REPLACE INTO message_edits(message_id,version,body,ts) VALUES(?1,?2,?3,?4)", params![message_id, version, old_body, now])?;
    let new_body = body.unwrap_or(old_body);
    tx.execute("UPDATE messages SET body=?2, version=version+1, edited_ms=?3 WHERE id=?1", params![message_id, new_body.trim_end(), now])?;
    if let Some(f) = form {
        let fj = f.as_ref().map(serde_json::to_string).transpose()?;
        tx.execute("UPDATE messages SET form=?2 WHERE id=?1", params![message_id, fj])?;
        tx.execute("DELETE FROM form_responses WHERE message_id=?1", [message_id])?;
    }
    let message = load_message(&tx, message_id)?;
    let mut em = Emitter::new();
    for u in recipients(&tx, &conv_id, message_id)? {
        em.emit(&tx, &u, EventData::MessageEdit { message: message.clone() })?;
    }
    let kind = conv_row(&tx, &conv_id)?.kind;
    crate::hooks::on_message(&tx, "message.updated", &message, kind)?;
    tx.commit()?;
    em.flush(hub);
    Ok(message)
}

// ------------------------------------------------------------------ forms: user side

pub fn user_json(conn: &Connection, id: &str) -> Value {
    tg_user(conn, id)
}

/// A member submits a form (or presses a button) on a bot message. The bot gets a `callback_query` update.
pub fn submit_form(conn: &mut Connection, hub: &Hub, user_id: &str, message_id: &str, req: &FormSubmit) -> AppResult<Message> {
    let (conv_id, sender, deleted, form_json): (String, String, i64, Option<String>) = conn
        .query_row("SELECT conv_id, sender_id, deleted, form FROM messages WHERE id=?1", [message_id], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)))
        .optional()?
        .ok_or_else(|| AppError::not_found("message not found"))?;
    member_conv(conn, &conv_id, user_id)?;
    let form: Form = form_json
        .and_then(|j| serde_json::from_str(&j).ok())
        .filter(|_| deleted == 0)
        .ok_or_else(|| AppError::validation("this message has no form"))?;
    form.validate(req.button_id.as_deref(), &req.values).map_err(AppError::validation)?;
    let already: i64 = conn.query_row("SELECT COUNT(*) FROM form_responses WHERE message_id=?1 AND user_id=?2", params![message_id, user_id], |r| r.get(0))?;
    if already > 0 {
        return Err(AppError::conflict("you already responded to this form"));
    }
    let tx = conn.transaction()?;
    let now = now_ms();
    tx.execute(
        "INSERT INTO form_responses(message_id,user_id,button_id,data,ts) VALUES(?1,?2,?3,?4,?5)",
        params![message_id, user_id, req.button_id, serde_json::to_string(&req.values)?, now],
    )?;
    let message = load_message(&tx, message_id)?;
    let mut em = Emitter::new();
    for u in recipients(&tx, &conv_id, message_id)? {
        em.emit(&tx, &u, EventData::MessageEdit { message: message.clone() })?;
    }
    let is_bot_sender = tx.query_row("SELECT COUNT(*) FROM bots WHERE user_id=?1", [&sender], |r| r.get::<_, i64>(0))? > 0;
    let data = req.button_id.clone().unwrap_or_else(|| form.id.clone());
    let cq = json!({
        "id": new_id(),
        "from": tg_user(&tx, user_id),
        "message": tg_message(&tx, &message),
        "data": data,
        "form_id": form.id,
        "button_id": req.button_id,
        "values": req.values,
    });
    if is_bot_sender {
        push_update(&tx, &sender, "callback_query", cq.clone())?;
    }
    let kind = conv_row(&tx, &conv_id)?.kind;
    let _ = kind;
    crate::hooks::on_event(&tx, "form.submitted", &conv_id, json!({"callback_query": cq}))?;
    tx.commit()?;
    em.flush(hub);
    wake();
    crate::hooks::wake();
    Ok(message)
}

/// A member presses a button or submits a form on a bot's Home page.
pub fn submit_home(conn: &Connection, user_id: &str, bot_id: &str, form_id: Option<&str>, req: &FormSubmit) -> AppResult<()> {
    let home: String = conn
        .query_row("SELECT home FROM bots WHERE user_id=?1", [bot_id], |r| r.get(0))
        .optional()?
        .ok_or_else(|| AppError::not_found("bot not found"))?;
    let home: BotHome = serde_json::from_str(&home).unwrap_or_default();
    let data = if let Some(fid) = form_id {
        let form = home
            .blocks
            .iter()
            .find_map(|b| match b {
                HomeBlock::Form { form } if form.id == fid => Some(form),
                _ => None,
            })
            .ok_or_else(|| AppError::not_found("form not found on the home page"))?;
        form.validate(req.button_id.as_deref(), &req.values).map_err(AppError::validation)?;
        req.button_id.clone().unwrap_or_else(|| fid.to_string())
    } else {
        let bid = req.button_id.clone().ok_or_else(|| AppError::validation("button_id is required"))?;
        let ok = home.blocks.iter().any(|b| matches!(b, HomeBlock::Button { id, .. } if *id == bid));
        if !ok {
            return Err(AppError::not_found("button not found on the home page"));
        }
        bid
    };
    let cq = json!({
        "id": new_id(),
        "from": tg_user(conn, user_id),
        "message": Value::Null,
        "home": true,
        "data": data,
        "form_id": form_id,
        "button_id": req.button_id,
        "values": req.values,
    });
    push_update(conn, bot_id, "callback_query", cq)?;
    wake();
    crate::hooks::wake();
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn markup_becomes_buttons() {
        let f = form_from_markup(&json!({"inline_keyboard": [[{"text": "Yes", "callback_data": "y"}, {"text": "No", "callback_data": "n"}]]})).unwrap();
        assert_eq!(f.buttons.len(), 2);
        assert_eq!(f.buttons[0].id, "y");
        assert!(form_from_markup(&json!({"inline_keyboard": []})).is_none());
    }
}
