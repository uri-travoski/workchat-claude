//! SV-16 webhooks: outgoing events with an HMAC signature and retries, plus the delivery worker
//! that is also used for bot webhooks (`setWebhook`). Deliveries live in the database, so retries
//! survive a restart.

use crate::db::{new_id, now_ms};
use crate::error::{AppError, AppResult};
use crate::state::St;
use hmac::{Hmac, Mac};
use proto::*;
use rusqlite::{params, Connection};
use sha2::Sha256;
use std::sync::OnceLock;
use tokio::sync::Notify;

type HmacSha256 = Hmac<Sha256>;

static WAKE: OnceLock<Notify> = OnceLock::new();

fn notify() -> &'static Notify {
    WAKE.get_or_init(Notify::new)
}

/// Tells the delivery worker that new work was queued.
pub fn wake() {
    notify().notify_waiters();
}

pub const EVENTS: &[&str] = &["message.created", "message.updated", "message.deleted", "form.submitted"];

pub fn valid_url(u: &str) -> AppResult<()> {
    let parsed = url::Url::parse(u).map_err(|_| AppError::validation("that URL is not valid"))?;
    if !matches!(parsed.scheme(), "http" | "https") || parsed.host_str().is_none() {
        return Err(AppError::validation("the URL must start with http:// or https://"));
    }
    Ok(())
}

pub fn sign(secret: &str, ts: i64, body: &str) -> String {
    let mut mac = HmacSha256::new_from_slice(secret.as_bytes()).expect("hmac key");
    mac.update(format!("{ts}.{body}").as_bytes());
    format!("sha256={}", hex::encode(mac.finalize().into_bytes()))
}

pub fn enqueue(conn: &Connection, kind: &str, ref_id: &str, url: &str, secret: Option<&str>, event: &str, body: &str) -> AppResult<()> {
    let now = now_ms();
    conn.execute(
        "INSERT INTO deliveries(kind,ref_id,url,secret,event,body,next_ms,created_ms) VALUES(?1,?2,?3,?4,?5,?6,?7,?8)",
        params![kind, ref_id, url, secret, event, body, now, now],
    )?;
    Ok(())
}

fn sender_json(conn: &Connection, id: &str) -> serde_json::Value {
    conn.query_row("SELECT username, display_name, is_bot FROM users WHERE id=?1", [id], |r| {
        Ok(serde_json::json!({"id": id, "username": r.get::<_, String>(0)?, "display_name": r.get::<_, String>(1)?, "is_bot": r.get::<_, i64>(2)? != 0}))
    })
    .unwrap_or_else(|_| serde_json::json!({"id": id}))
}

/// Queues a delivery for every enabled outgoing webhook that wants this event.
pub fn on_event(conn: &Connection, event: &str, conv_id: &str, extra: serde_json::Value) -> AppResult<()> {
    let mut st = conn.prepare("SELECT id, url, secret, events, conv_id FROM webhooks_out WHERE enabled=1")?;
    let hooks = st
        .query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?, r.get::<_, String>(2)?, r.get::<_, String>(3)?, r.get::<_, Option<String>>(4)?)))?
        .collect::<Result<Vec<_>, _>>()?;
    if hooks.is_empty() {
        return Ok(());
    }
    let conv = conn
        .query_row("SELECT kind, name FROM conversations WHERE id=?1", [conv_id], |r| Ok((r.get::<_, String>(0)?, r.get::<_, Option<String>>(1)?)))
        .ok();
    let mut payload = serde_json::json!({
        "event": event,
        "ts": now_ms(),
        "conversation": {"id": conv_id, "kind": conv.as_ref().map(|c| c.0.clone()), "name": conv.as_ref().and_then(|c| c.1.clone())},
    });
    if let (Some(o), Some(e)) = (payload.as_object_mut(), extra.as_object()) {
        for (k, v) in e {
            o.insert(k.clone(), v.clone());
        }
    }
    let body = payload.to_string();
    for (id, url, secret, events, hook_conv) in hooks {
        let evs: Vec<String> = serde_json::from_str(&events).unwrap_or_default();
        if !evs.is_empty() && !evs.iter().any(|e| e == event) {
            continue;
        }
        if hook_conv.as_deref().map(|c| c != conv_id).unwrap_or(false) {
            continue;
        }
        enqueue(conn, "hook", &id, &url, Some(&secret), event, &body)?;
    }
    Ok(())
}

pub fn on_message(conn: &Connection, event: &str, m: &Message, _kind: ConversationKind) -> AppResult<()> {
    let sender = sender_json(conn, &m.sender_id);
    on_event(conn, event, &m.conv_id, serde_json::json!({"message": m, "sender": sender}))
}

// ------------------------------------------------------------------ worker

pub fn spawn(st: St) {
    tokio::spawn(async move {
        let mut last_prune = 0i64;
        loop {
            if st.is_stopped() {
                break;
            }
            let tick = std::time::Duration::from_millis(st.cfg.worker_tick_ms.max(20));
            let _ = tokio::time::timeout(tick, notify().notified()).await;
            if st.is_stopped() {
                break;
            }
            if let Err(e) = process_due(&st).await {
                tracing::warn!("webhook worker: {e}");
            }
            if now_ms() - last_prune > 3_600_000 {
                last_prune = now_ms();
                let cutoff = now_ms() - 3 * 24 * 3600 * 1000;
                let _ = st
                    .db
                    .call(move |c| {
                        c.execute("DELETE FROM deliveries WHERE state!='pending' AND COALESCE(done_ms, created_ms) < ?1", [cutoff])?;
                        c.execute("DELETE FROM bot_updates WHERE created_ms < ?1", [now_ms() - 24 * 3600 * 1000])?;
                        Ok(())
                    })
                    .await;
            }
        }
    });
}

struct Due {
    id: i64,
    kind: String,
    ref_id: String,
    url: String,
    secret: Option<String>,
    event: String,
    body: String,
    attempts: i64,
}

async fn process_due(st: &St) -> AppResult<()> {
    let due: Vec<Due> = st
        .db
        .call(|c| {
            let now = now_ms();
            let mut stmt = c.prepare(
                "SELECT id,kind,ref_id,url,secret,event,body,attempts FROM deliveries WHERE state='pending' AND next_ms<=?1 ORDER BY id LIMIT 20",
            )?;
            let rows = stmt
                .query_map([now], |r| {
                    Ok(Due { id: r.get(0)?, kind: r.get(1)?, ref_id: r.get(2)?, url: r.get(3)?, secret: r.get(4)?, event: r.get(5)?, body: r.get(6)?, attempts: r.get(7)? })
                })?
                .collect::<Result<Vec<_>, _>>()?;
            // Claim: if the process dies mid-delivery the row is retried after a minute.
            for d in &rows {
                c.execute("UPDATE deliveries SET next_ms=?2 WHERE id=?1", params![d.id, now + 60_000])?;
            }
            Ok(rows)
        })
        .await?;
    if due.is_empty() {
        return Ok(());
    }
    let mut set = tokio::task::JoinSet::new();
    for d in due {
        let st = st.clone();
        set.spawn(async move {
            let res = send(&st, &d).await;
            let _ = record(&st, &d, res).await;
        });
    }
    while set.join_next().await.is_some() {}
    Ok(())
}

async fn send(st: &St, d: &Due) -> Result<u16, String> {
    let ts = now_ms() / 1000;
    let mut req = st
        .http
        .post(&d.url)
        .timeout(std::time::Duration::from_secs(10))
        .header("content-type", "application/json")
        .header("user-agent", format!("workchat/{}", proto::VERSION))
        .header("x-workchat-event", &d.event)
        .header("x-workchat-delivery", d.id.to_string());
    if d.kind == "bot" {
        if let Some(s) = d.secret.as_deref().filter(|s| !s.is_empty()) {
            req = req.header("x-telegram-bot-api-secret-token", s);
        }
    } else if let Some(s) = d.secret.as_deref() {
        let sig = sign(s, ts, &d.body);
        req = req.header("x-workchat-timestamp", ts.to_string()).header("x-workchat-signature", sig);
    }
    match req.body(d.body.clone()).send().await {
        Ok(r) => {
            let code = r.status().as_u16();
            if r.status().is_success() {
                Ok(code)
            } else {
                Err(format!("HTTP {code}"))
            }
        }
        Err(e) => Err(format!("{}", e.without_url())),
    }
}

pub fn retry_delay_ms(base: u64, attempts: i64) -> i64 {
    let mult = 3i64.saturating_pow((attempts - 1).clamp(0, 12) as u32);
    ((base as i64).saturating_mul(mult)).min(3_600_000)
}

async fn record(st: &St, d: &Due, res: Result<u16, String>) -> AppResult<()> {
    let (id, kind, ref_id, attempts) = (d.id, d.kind.clone(), d.ref_id.clone(), d.attempts + 1);
    let (base, max) = (st.cfg.webhook_retry_base_ms, st.cfg.webhook_max_attempts);
    st.db
        .call(move |c| {
            let now = now_ms();
            match res {
                Ok(code) => {
                    c.execute(
                        "UPDATE deliveries SET state='done', attempts=?2, last_status=?3, last_error=NULL, done_ms=?4 WHERE id=?1",
                        params![id, attempts, code as i64, now],
                    )?;
                    if kind == "bot" {
                        c.execute("UPDATE bots SET webhook_error=NULL, webhook_error_ms=NULL WHERE user_id=?1", [&ref_id])?;
                    }
                }
                Err(e) => {
                    if attempts >= max {
                        c.execute("UPDATE deliveries SET state='dead', attempts=?2, last_error=?3, done_ms=?4 WHERE id=?1", params![id, attempts, e, now])?;
                    } else {
                        c.execute(
                            "UPDATE deliveries SET attempts=?2, last_error=?3, next_ms=?4 WHERE id=?1",
                            params![id, attempts, e, now + retry_delay_ms(base, attempts)],
                        )?;
                    }
                    if kind == "bot" {
                        c.execute("UPDATE bots SET webhook_error=?2, webhook_error_ms=?3 WHERE user_id=?1", params![ref_id, e, now])?;
                    }
                }
            }
            Ok(())
        })
        .await
}

pub fn new_hook_id() -> String {
    new_id()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn signature_is_stable_and_keyed() {
        let a = sign("secret", 1700000000, "{\"a\":1}");
        assert_eq!(a, sign("secret", 1700000000, "{\"a\":1}"));
        assert_ne!(a, sign("other", 1700000000, "{\"a\":1}"));
        assert_ne!(a, sign("secret", 1700000001, "{\"a\":1}"));
        assert!(a.starts_with("sha256="));
        assert_eq!(a.len(), 7 + 64);
    }

    #[test]
    fn backoff_grows_and_caps() {
        assert_eq!(retry_delay_ms(1000, 1), 1000);
        assert_eq!(retry_delay_ms(1000, 2), 3000);
        assert_eq!(retry_delay_ms(1000, 3), 9000);
        assert_eq!(retry_delay_ms(10_000, 12), 3_600_000);
    }

    #[test]
    fn url_validation() {
        assert!(valid_url("https://example.com/hook").is_ok());
        assert!(valid_url("http://127.0.0.1:9/x").is_ok());
        assert!(valid_url("ftp://x").is_err());
        assert!(valid_url("nonsense").is_err());
    }
}
