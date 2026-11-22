use crate::error::ErrorCode;
use crate::events::Event;
use crate::types::*;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

/// WebSocket close codes used by the server.
pub mod close {
    pub const UNAUTHORIZED: u16 = 4401;
    pub const FORBIDDEN: u16 = 4403;
    pub const SESSION_ENDED: u16 = 4410;
}

/// Messages from client to server. The first message must be `hello`.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "t", rename_all = "snake_case")]
pub enum ClientWsMsg {
    Hello { token: String, cursor: i64, #[serde(default)] client_version: String },
    Ping,
    Typing { conv_id: String },
    /// Presence preference changes are made via PATCH /me; this reports idle state (auto-away).
    Idle { idle: bool },
    /// The client has applied all events up to and including `cursor` (drives delivery state).
    Ack { cursor: i64 },
    /// v1.1 stub: call signalling is not implemented in v1.0.
    CallSignal { conv_id: String, payload: serde_json::Value },
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "t", rename_all = "snake_case")]
pub enum ServerWsMsg {
    HelloOk { cursor: i64, server_time: Ts },
    /// Ordered events. `caught_up` is false while the backlog since the client's cursor is still streaming.
    Events { events: Vec<Event>, caught_up: bool },
    Typing { conv_id: String, user_id: String },
    Presence { user_id: String, presence: Presence, last_active_ms: Option<Ts> },
    Pong,
    Error { code: ErrorCode, message: String },
}
