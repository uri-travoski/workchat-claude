//! Realtime WebSocket: hello (auth + cursor), backlog replay, live events, typing, presence, acks.

use crate::auth::authenticate;
use crate::db::now_ms;
use crate::emit::Emitter;
use crate::error::{AppError, AppResult};
use crate::hub::WsOut;
use crate::queries::*;
use crate::routes::chat::{current_cursor, fetch_events};
use crate::state::St;
use axum::extract::ws::{CloseFrame, Message as WsMessage, WebSocket, WebSocketUpgrade};
use axum::extract::State;
use axum::response::Response;
use proto::*;
use rusqlite::{params, Connection};
use std::time::Duration;
use tokio::sync::mpsc;

pub async fn ws_handler(State(st): State<St>, ws: WebSocketUpgrade) -> Response {
    ws.max_message_size(256 * 1024).on_upgrade(move |sock| handle(st, sock))
}

async fn send_json(sock: &mut WebSocket, msg: &ServerWsMsg) -> bool {
    match serde_json::to_string(msg) {
        Ok(t) => sock.send(WsMessage::Text(t.into())).await.is_ok(),
        Err(_) => false,
    }
}

async fn close_with(sock: &mut WebSocket, code: u16, reason: &str) {
    let _ = sock
        .send(WsMessage::Close(Some(CloseFrame { code, reason: reason.to_string().into() })))
        .await;
}

async fn handle(st: St, mut sock: WebSocket) {
    // 1. hello
    let hello = tokio::time::timeout(Duration::from_secs(10), async {
        while let Some(Ok(m)) = sock.recv().await {
            if let WsMessage::Text(t) = m {
                return serde_json::from_str::<ClientWsMsg>(&t).ok();
            }
        }
        None
    })
    .await
    .ok()
    .flatten();
    let Some(ClientWsMsg::Hello { token, cursor, client_version }) = hello else {
        let _ = send_json(&mut sock, &ServerWsMsg::Error { code: ErrorCode::Unauthorized, message: "expected hello".into() }).await;
        close_with(&mut sock, close::UNAUTHORIZED, "unauthorized").await;
        return;
    };
    if !client_version.is_empty() && rules::version_lt(&client_version, MIN_CLIENT_VERSION) {
        let _ = send_json(&mut sock, &ServerWsMsg::Error { code: ErrorCode::ClientTooOld, message: "client too old".into() }).await;
        close_with(&mut sock, close::FORBIDDEN, "client_too_old").await;
        return;
    }
    let auth = match st.db.call(move |c| authenticate(c, &token)).await {
        Ok(a) => a,
        Err(e) => {
            let _ = send_json(&mut sock, &ServerWsMsg::Error { code: e.code, message: e.message.clone() }).await;
            let cc = if e.code == ErrorCode::TokenExpired || e.code == ErrorCode::Unauthorized { close::UNAUTHORIZED } else { close::SESSION_ENDED };
            close_with(&mut sock, cc, &e.code.as_str()).await;
            return;
        }
    };

    // 2. register before reading the backlog so nothing is missed
    let (tx, mut rx) = mpsc::unbounded_channel::<WsOut>();
    let (conn_id, first) = st.hub.register(&auth.user_id, &auth.session_id, tx);
    if first {
        let (st2, uid) = (st.clone(), auth.user_id.clone());
        let _ = st.db.call(move |c| broadcast_presence(c, &st2, &uid)).await;
    }
    let result = run(&st, &mut sock, &mut rx, &auth.user_id, cursor).await;
    let last = st.hub.unregister(&auth.user_id, conn_id);
    if last {
        let (st2, uid) = (st.clone(), auth.user_id.clone());
        let _ = st
            .db
            .call(move |c| {
                c.execute("UPDATE users SET last_active_ms=?1 WHERE id=?2", params![now_ms(), uid])?;
                broadcast_presence(c, &st2, &uid)
            })
            .await;
    }
    if let Err((code, reason)) = result {
        close_with(&mut sock, code, &reason).await;
    }
}

async fn run(
    st: &St,
    sock: &mut WebSocket,
    rx: &mut mpsc::UnboundedReceiver<WsOut>,
    user_id: &str,
    client_cursor: i64,
) -> Result<(), (u16, String)> {
    let uid = user_id.to_string();
    let server_cursor = st.db.call(|c| current_cursor(c)).await.map_err(|e| (1011, e.message))?;
    if !send_json(sock, &ServerWsMsg::HelloOk { cursor: server_cursor, server_time: now_ms() }).await {
        return Ok(());
    }
    // Backlog replay in batches.
    let mut last_sent = client_cursor;
    loop {
        let (u2, after) = (uid.clone(), last_sent);
        let res = st.db.call(move |c| fetch_events(c, &u2, after, 300)).await;
        match res {
            Ok((events, more)) => {
                if let Some(e) = events.last() {
                    last_sent = e.cursor;
                }
                if (!events.is_empty() || !more)
                    && !send_json(sock, &ServerWsMsg::Events { events, caught_up: !more }).await {
                        return Ok(());
                    }
                if !more {
                    break;
                }
            }
            Err(e) => {
                let _ = send_json(sock, &ServerWsMsg::Error { code: e.code, message: e.message.clone() }).await;
                return Err((1000, e.code.as_str()));
            }
        }
    }
    let mut ping = tokio::time::interval(Duration::from_secs(25));
    ping.tick().await;
    let mut last_rx = tokio::time::Instant::now();
    loop {
        tokio::select! {
            incoming = sock.recv() => {
                let Some(Ok(m)) = incoming else { return Ok(()) };
                last_rx = tokio::time::Instant::now();
                match m {
                    WsMessage::Text(t) => {
                        let Ok(cm) = serde_json::from_str::<ClientWsMsg>(&t) else { continue };
                        handle_client_msg(st, sock, &uid, cm).await;
                    }
                    WsMessage::Close(_) => return Ok(()),
                    _ => {}
                }
            }
            out = rx.recv() => {
                match out {
                    Some(WsOut::Msg(ServerWsMsg::Events { events, caught_up })) => {
                        let fresh: Vec<Event> = events.into_iter().filter(|e| e.cursor > last_sent).collect();
                        if let Some(e) = fresh.last() { last_sent = e.cursor; }
                        if !fresh.is_empty() && !send_json(sock, &ServerWsMsg::Events { events: fresh, caught_up }).await {
                            return Ok(());
                        }
                    }
                    Some(WsOut::Msg(m)) => { if !send_json(sock, &m).await { return Ok(()); } }
                    Some(WsOut::Close(code, reason)) => return Err((code, reason)),
                    None => return Ok(()),
                }
            }
            _ = ping.tick() => {
                if last_rx.elapsed() > Duration::from_secs(80) { return Ok(()); }
                if sock.send(WsMessage::Ping(vec![].into())).await.is_err() { return Ok(()); }
            }
        }
    }
}

async fn handle_client_msg(st: &St, sock: &mut WebSocket, uid: &str, msg: ClientWsMsg) {
    match msg {
        ClientWsMsg::Ping => {
            let _ = send_json(sock, &ServerWsMsg::Pong).await;
        }
        ClientWsMsg::Typing { conv_id } => {
            if !st.limiter.check(&format!("typing:{uid}")) {
                return;
            }
            let (st2, u2) = (st.clone(), uid.to_string());
            let _ = st
                .db
                .call(move |c| {
                    let cr = member_conv(c, &conv_id, &u2)?;
                    if cr.kind == ConversationKind::SelfNote {
                        return Ok(());
                    }
                    for m in member_ids(c, &conv_id)? {
                        if m != u2 {
                            st2.hub.send_user(&m, ServerWsMsg::Typing { conv_id: conv_id.clone(), user_id: u2.clone() });
                        }
                    }
                    Ok(())
                })
                .await;
        }
        ClientWsMsg::Idle { idle } => {
            // Auto-away is reported by the client; stored as a transient preference only when the
            // user is on "auto" so an explicit Away/DND choice is never overwritten.
            let (st2, u2) = (st.clone(), uid.to_string());
            let _ = st
                .db
                .call(move |c| {
                    let row = get_user(c, &u2)?;
                    if row.presence_pref == "auto" || row.presence_pref == "auto_away" {
                        let v = if idle { "auto_away" } else { "auto" };
                        c.execute("UPDATE users SET presence_pref=?2 WHERE id=?1", params![u2, v])?;
                        broadcast_presence(c, &st2, &u2)?;
                    }
                    Ok(())
                })
                .await;
        }
        ClientWsMsg::Ack { cursor } => {
            let (st2, u2) = (st.clone(), uid.to_string());
            let _ = st.db.call(move |c| process_ack(c, &st2, &u2, cursor)).await;
        }
        ClientWsMsg::CallSignal { .. } => {
            let _ = send_json(sock, &ServerWsMsg::Error { code: ErrorCode::NotImplemented, message: "calls arrive in v1.1".into() }).await;
        }
        ClientWsMsg::Hello { .. } => {}
    }
}

fn process_ack(conn: &mut Connection, st: &St, user_id: &str, cursor: i64) -> AppResult<()> {
    let acked: i64 = conn.query_row("SELECT acked_cursor FROM users WHERE id=?1", [user_id], |r| r.get(0))?;
    if cursor <= acked {
        return Ok(());
    }
    let tx = conn.transaction()?;
    let ids: Vec<String> = {
        let mut stmt = tx.prepare("SELECT ref FROM events WHERE user_id=?1 AND id>?2 AND id<=?3 AND kind='message_new' AND ref IS NOT NULL")?;
        let rows = stmt.query_map(params![user_id, acked, cursor], |r| r.get(0))?.collect::<Result<Vec<String>, _>>()?;
        rows
    };
    let mut em = Emitter::new();
    for mid in ids {
        let row: Option<(String, String)> = tx
            .query_row(
                "UPDATE messages SET delivered_ms=?1 WHERE id=?2 AND delivered_ms IS NULL AND sender_id!=?3 RETURNING conv_id, sender_id",
                params![now_ms(), mid, user_id],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .ok();
        if let Some((conv_id, sender)) = row {
            em.emit(&tx, &sender, EventData::MessageDelivered { conv_id, message_id: mid })?;
        }
    }
    tx.execute("UPDATE users SET acked_cursor=?2 WHERE id=?1", params![user_id, cursor])?;
    tx.commit()?;
    em.flush(&st.hub);
    Ok(())
}

/// Tells everyone who shares a conversation with `user_id` about the user's presence.
pub fn broadcast_presence(conn: &Connection, st: &St, user_id: &str) -> Result<(), AppError> {
    broadcast_presence_hub(conn, &st.hub, user_id)
}

/// Variant used where only the hub (not the whole state) is at hand.
pub fn broadcast_presence_hub(conn: &Connection, hub: &crate::hub::Hub, user_id: &str) -> Result<(), AppError> {
    let row = get_user(conn, user_id)?;
    let presence = presence_of(&row, hub);
    let msg = ServerWsMsg::Presence { user_id: user_id.to_string(), presence, last_active_ms: visible_last_active(&row) };
    let mut stmt = conn.prepare(
        "SELECT DISTINCT m2.user_id FROM members m1 JOIN members m2 ON m1.conv_id=m2.conv_id WHERE m1.user_id=?1 AND m2.user_id!=?1",
    )?;
    let others = stmt.query_map([user_id], |r| r.get::<_, String>(0))?.collect::<Result<Vec<_>, _>>()?;
    for o in others {
        hub.send_user(&o, msg.clone());
    }
    hub.send_user(user_id, msg);
    Ok(())
}
