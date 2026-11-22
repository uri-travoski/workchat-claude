//! One error type for the whole core. It serialises to `{ "code": "...", "message": "..." }` so the UI can
//! branch on `code` (server error codes pass through unchanged) and show `message` to the person.

use proto::ErrorCode;
use serde::ser::SerializeStruct;

#[derive(Debug, thiserror::Error)]
pub enum CoreError {
    /// A problem found before talking to the server (bad input, bad address).
    #[error("{0}")]
    Local(String),
    #[error("{0}")]
    Network(String),
    #[error("{message}")]
    Api { code: ErrorCode, message: String },
    #[error("You are not signed in.")]
    NotSignedIn,
    #[error("{0}")]
    Storage(String),
}

pub type Result<T> = std::result::Result<T, CoreError>;

impl CoreError {
    pub fn local(m: impl Into<String>) -> Self {
        CoreError::Local(m.into())
    }
    pub fn code(&self) -> String {
        match self {
            CoreError::Local(_) => "local".into(),
            CoreError::Network(_) => "network".into(),
            CoreError::Api { code, .. } => code.as_str(),
            CoreError::NotSignedIn => "not_signed_in".into(),
            CoreError::Storage(_) => "storage".into(),
        }
    }
    /// True when retrying later may succeed (offline, server busy).
    pub fn is_transient(&self) -> bool {
        match self {
            CoreError::Network(_) => true,
            CoreError::Api { code, .. } => matches!(code, ErrorCode::RateLimited | ErrorCode::Internal | ErrorCode::StorageError | ErrorCode::TokenExpired),
            _ => false,
        }
    }
}

impl serde::Serialize for CoreError {
    fn serialize<S: serde::Serializer>(&self, s: S) -> std::result::Result<S::Ok, S::Error> {
        let mut st = s.serialize_struct("CoreError", 2)?;
        st.serialize_field("code", &self.code())?;
        st.serialize_field("message", &self.to_string())?;
        st.end()
    }
}

impl From<rusqlite::Error> for CoreError {
    fn from(e: rusqlite::Error) -> Self {
        CoreError::Storage(format!("local database: {e}"))
    }
}
impl From<serde_json::Error> for CoreError {
    fn from(e: serde_json::Error) -> Self {
        CoreError::Storage(format!("local data: {e}"))
    }
}
impl From<std::io::Error> for CoreError {
    fn from(e: std::io::Error) -> Self {
        CoreError::Storage(format!("file error: {e}"))
    }
}
