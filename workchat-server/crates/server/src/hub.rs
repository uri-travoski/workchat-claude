//! Registry of live WebSocket connections and event fan-out.

use proto::{Event, ServerWsMsg};
use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use tokio::sync::mpsc::UnboundedSender;

pub enum WsOut {
    Msg(ServerWsMsg),
    Close(u16, String),
}

#[derive(Clone)]
pub struct ConnHandle {
    pub id: u64,
    pub session_id: String,
    pub tx: UnboundedSender<WsOut>,
}

#[derive(Clone, Default)]
pub struct Hub {
    inner: Arc<Mutex<HashMap<String, Vec<ConnHandle>>>>,
    next: Arc<AtomicU64>,
}

impl Hub {
    pub fn new() -> Hub {
        Hub::default()
    }

    pub fn register(&self, user_id: &str, session_id: &str, tx: UnboundedSender<WsOut>) -> (u64, bool) {
        let id = self.next.fetch_add(1, Ordering::Relaxed) + 1;
        let mut g = self.inner.lock().unwrap_or_else(|e| e.into_inner());
        let v = g.entry(user_id.to_string()).or_default();
        let first = v.is_empty();
        v.push(ConnHandle { id, session_id: session_id.to_string(), tx });
        (id, first)
    }

    /// Returns true when this was the user's last connection.
    pub fn unregister(&self, user_id: &str, conn_id: u64) -> bool {
        let mut g = self.inner.lock().unwrap_or_else(|e| e.into_inner());
        if let Some(v) = g.get_mut(user_id) {
            v.retain(|c| c.id != conn_id);
            if v.is_empty() {
                g.remove(user_id);
                return true;
            }
        }
        false
    }

    /// Test support: forgets the user's live connections so the server treats them as offline (and sends wake-up pushes
    /// instead of live events) while their sockets stay open. Used by the emulator push test.
    pub fn forget_user(&self, user_id: &str) {
        self.inner.lock().unwrap_or_else(|e| e.into_inner()).remove(user_id);
    }

    pub fn is_online(&self, user_id: &str) -> bool {
        let g = self.inner.lock().unwrap_or_else(|e| e.into_inner());
        g.get(user_id).map(|v| !v.is_empty()).unwrap_or(false)
    }

    pub fn online_count(&self) -> usize {
        self.inner.lock().unwrap_or_else(|e| e.into_inner()).len()
    }

    pub fn dispatch(&self, events: Vec<(String, Event)>) {
        let g = self.inner.lock().unwrap_or_else(|e| e.into_inner());
        for (user, ev) in events {
            if let Some(conns) = g.get(&user) {
                for c in conns {
                    let _ = c.tx.send(WsOut::Msg(ServerWsMsg::Events { events: vec![ev.clone()], caught_up: true }));
                }
            }
        }
    }

    pub fn send_user(&self, user_id: &str, msg: ServerWsMsg) {
        let g = self.inner.lock().unwrap_or_else(|e| e.into_inner());
        if let Some(conns) = g.get(user_id) {
            for c in conns {
                let _ = c.tx.send(WsOut::Msg(msg.clone()));
            }
        }
    }

    /// Tell every connection of `user_id` why it is being closed, then close it.
    pub fn kick_user(&self, user_id: &str, code: proto::ErrorCode, close_code: u16, message: &str) {
        let g = self.inner.lock().unwrap_or_else(|e| e.into_inner());
        if let Some(conns) = g.get(user_id) {
            for c in conns {
                let _ = c.tx.send(WsOut::Msg(ServerWsMsg::Error { code, message: message.to_string() }));
                let _ = c.tx.send(WsOut::Close(close_code, code.as_str()));
            }
        }
    }

    pub fn kick_session(&self, session_id: &str, code: proto::ErrorCode, close_code: u16, message: &str) {
        let g = self.inner.lock().unwrap_or_else(|e| e.into_inner());
        for conns in g.values() {
            for c in conns.iter().filter(|c| c.session_id == session_id) {
                let _ = c.tx.send(WsOut::Msg(ServerWsMsg::Error { code, message: message.to_string() }));
                let _ = c.tx.send(WsOut::Close(close_code, code.as_str()));
            }
        }
    }
}
