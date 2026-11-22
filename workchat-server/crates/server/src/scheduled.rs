//! SV-10 scheduled messages: stored in the database, sent as the user by a background worker.
//! Because the queue is in SQLite and sends are idempotent (`sched-<id>` client id), pending
//! messages survive restarts and a message can never be sent twice.

use crate::db::{new_id, now_ms};
use crate::emit::Emitter;
use crate::error::{AppError, AppResult};
use crate::hub::Hub;
use crate::queries::*;
use crate::state::St;
use proto::*;
use rusqlite::{params, Connection, OptionalExtension};

const MAX_PENDING: i64 = 200;
const MAX_AHEAD_MS: i64 = 366 * 24 * 3600 * 1000;

fn row(r: &rusqlite::Row) -> rusqlite::Result<ScheduledMessage> {
    let atts: String = r.get(5)?;
    Ok(ScheduledMessage {
        id: r.get(0)?,
        user_id: r.get(1)?,
        conv_id: r.get(2)?,
        body: r.get(3)?,
        thread_root: r.get(4)?,
        attachment_ids: serde_json::from_str(&atts).unwrap_or_default(),
        due_ms: r.get(6)?,
        created_ms: r.get(7)?,
        state: ScheduledState::parse(&r.get::<_, String>(8)?),
        error: r.get(9)?,
        message_id: r.get(10)?,
    })
}

const COLS: &str = "id,user_id,conv_id,body,thread_root,attachment_ids,due_ms,created_ms,state,error,message_id";

pub fn get(conn: &Connection, id: &str) -> AppResult<ScheduledMessage> {
    conn.query_row(&format!("SELECT {COLS} FROM scheduled WHERE id=?1"), [id], row)
        .optional()?
        .ok_or_else(|| AppError::not_found("scheduled message not found"))
}

/// The user's pending messages plus what was sent, cancelled or failed in the last week.
pub fn list(conn: &Connection, user_id: &str) -> AppResult<Vec<ScheduledMessage>> {
    let cutoff = now_ms() - 7 * 24 * 3600 * 1000;
    let mut st = conn.prepare(&format!(
        "SELECT {COLS} FROM scheduled WHERE user_id=?1 AND (state='pending' OR COALESCE(done_ms, created_ms)>?2) ORDER BY due_ms"
    ))?;
    let v = st.query_map(params![user_id, cutoff], row)?.collect::<Result<Vec<_>, _>>()?;
    Ok(v)
}

fn emit(conn: &Connection, em: &mut Emitter, item: &ScheduledMessage) -> AppResult<()> {
    em.emit(conn, &item.user_id, EventData::ScheduledUpdate { item: item.clone() })?;
    Ok(())
}

pub fn create(conn: &mut Connection, hub: &Hub, user_id: &str, conv_id: &str, req: &ScheduleRequest) -> AppResult<ScheduledMessage> {
    let cr = member_conv(conn, conv_id, user_id)?;
    if cr.archived {
        return Err(AppError::forbidden("this conversation is archived"));
    }
    let now = now_ms();
    if req.due_ms <= now {
        return Err(AppError::validation("choose a time in the future"));
    }
    if req.due_ms > now + MAX_AHEAD_MS {
        return Err(AppError::validation("messages can be scheduled up to a year ahead"));
    }
    let body = req.body.trim_end().to_string();
    if body.trim().is_empty() && req.attachment_ids.is_empty() {
        return Err(AppError::validation("message is empty"));
    }
    if body.chars().count() > 20_000 {
        return Err(AppError::validation("message too long"));
    }
    let pending: i64 = conn.query_row("SELECT COUNT(*) FROM scheduled WHERE user_id=?1 AND state='pending'", [user_id], |r| r.get(0))?;
    if pending >= MAX_PENDING {
        return Err(AppError::validation("too many scheduled messages"));
    }
    for a in &req.attachment_ids {
        let ok: i64 = conn.query_row(
            "SELECT COUNT(*) FROM attachments WHERE id=?1 AND uploader_id=?2 AND removed=0 AND purpose='file'",
            params![a, user_id],
            |r| r.get(0),
        )?;
        if ok == 0 {
            return Err(AppError::validation("attachment not found"));
        }
    }
    let tx = conn.transaction()?;
    let id = new_id();
    tx.execute(
        "INSERT INTO scheduled(id,user_id,conv_id,body,thread_root,attachment_ids,due_ms,created_ms) VALUES(?1,?2,?3,?4,?5,?6,?7,?8)",
        params![id, user_id, conv_id, body, req.thread_root, serde_json::to_string(&req.attachment_ids)?, req.due_ms, now],
    )?;
    let item = get(&tx, &id)?;
    let mut em = Emitter::new();
    emit(&tx, &mut em, &item)?;
    tx.commit()?;
    em.flush(hub);
    Ok(item)
}

pub fn cancel(conn: &mut Connection, hub: &Hub, user_id: &str, id: &str) -> AppResult<ScheduledMessage> {
    let item = get(conn, id)?;
    if item.user_id != user_id {
        return Err(AppError::not_found("scheduled message not found"));
    }
    if item.state != ScheduledState::Pending {
        return Err(AppError::conflict("this message is no longer pending"));
    }
    let tx = conn.transaction()?;
    let n = tx.execute("UPDATE scheduled SET state='cancelled', done_ms=?2 WHERE id=?1 AND state='pending'", params![id, now_ms()])?;
    if n == 0 {
        return Err(AppError::conflict("this message is no longer pending"));
    }
    let item = get(&tx, id)?;
    let mut em = Emitter::new();
    emit(&tx, &mut em, &item)?;
    tx.commit()?;
    em.flush(hub);
    Ok(item)
}

async fn finish(st: &St, id: &str, result: Result<String, String>) -> AppResult<()> {
    let hub = st.hub.clone();
    let id = id.to_string();
    st.db
        .call(move |c| {
            let tx = c.transaction()?;
            match &result {
                Ok(mid) => tx.execute("UPDATE scheduled SET state='sent', message_id=?2, done_ms=?3 WHERE id=?1", params![id, mid, now_ms()])?,
                Err(e) => tx.execute("UPDATE scheduled SET state='failed', error=?2, done_ms=?3 WHERE id=?1", params![id, e, now_ms()])?,
            };
            let item = get(&tx, &id)?;
            let mut em = Emitter::new();
            emit(&tx, &mut em, &item)?;
            tx.commit()?;
            em.flush(&hub);
            Ok(())
        })
        .await
}

/// Sends everything that is due. Returns how many were handled.
pub async fn send_due(st: &St) -> AppResult<usize> {
    let due: Vec<ScheduledMessage> = st
        .db
        .call(|c| {
            let mut stmt = c.prepare(&format!("SELECT {COLS} FROM scheduled WHERE state='pending' AND due_ms<=?1 ORDER BY due_ms LIMIT 50"))?;
            let v = stmt.query_map([now_ms()], row)?.collect::<Result<Vec<_>, _>>()?;
            Ok(v)
        })
        .await?;
    let n = due.len();
    for item in due {
        let uid = item.user_id.clone();
        let disabled = st.db.call(move |c| Ok(get_user(c, &uid).map(|u| u.disabled || u.deleted).unwrap_or(true))).await?;
        if disabled {
            finish(st, &item.id, Err("the account is disabled".into())).await?;
            continue;
        }
        let req = SendMessage {
            client_id: format!("sched-{}", item.id),
            body: item.body.clone(),
            thread_root: item.thread_root.clone(),
            reply_to: None,
            mentions: vec![],
            attachment_ids: item.attachment_ids.clone(),
            also_send_to_channel: false,
        };
        match crate::routes::chat::post_as(st, &item.user_id, &item.conv_id, req, None).await {
            Ok((_, m)) => finish(st, &item.id, Ok(m.id)).await?,
            Err(e) => finish(st, &item.id, Err(e.message.clone())).await?,
        }
    }
    Ok(n)
}

pub fn spawn(st: St) {
    tokio::spawn(async move {
        loop {
            if st.is_stopped() {
                break;
            }
            if let Err(e) = send_due(&st).await {
                tracing::warn!("scheduled sender: {e}");
            }
            tokio::time::sleep(std::time::Duration::from_millis(st.cfg.worker_tick_ms.max(20))).await;
        }
    });
}
