use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

/// Stable, machine-readable error codes. New codes may be added; existing ones never change meaning.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum ErrorCode {
    /// Missing or malformed credentials.
    Unauthorized,
    InvalidCredentials,
    /// Access token expired; refresh and retry.
    TokenExpired,
    /// The device session was revoked; the client must wipe local data.
    SessionRevoked,
    /// The account was disabled; the client must wipe local data.
    AccountDisabled,
    /// An admin reset the password; the client must wipe local data and sign in again.
    PasswordReset,
    /// An admin cleared this user's history; the client must wipe local data and sign in again.
    HistoryCleared,
    /// Too many failed sign-ins; try again later.
    LockedOut,
    RateLimited,
    Forbidden,
    NotFound,
    Validation,
    Conflict,
    /// Edit refused by the edit rule (a reply has been received in a DM / group DM).
    EditWindowClosed,
    FileTooLarge,
    /// The client's event cursor is older than the retained log; re-bootstrap.
    CursorExpired,
    /// Client older than `min_client_version`.
    ClientTooOld,
    /// First-run setup has not been completed yet.
    SetupRequired,
    SetupDone,
    /// A required backup could not be written, the action was refused and nothing changed.
    BackupFailed,
    StorageError,
    NotImplemented,
    Internal,
}

impl ErrorCode {
    /// HTTP status used for this code.
    pub fn http_status(self) -> u16 {
        use ErrorCode::*;
        match self {
            Unauthorized | InvalidCredentials | TokenExpired | SessionRevoked | PasswordReset
            | HistoryCleared => 401,
            AccountDisabled | Forbidden | ClientTooOld => 403,
            NotFound => 404,
            Validation | SetupRequired => 400,
            Conflict | EditWindowClosed | SetupDone => 409,
            FileTooLarge => 413,
            LockedOut | RateLimited => 429,
            CursorExpired => 410,
            NotImplemented => 501,
            BackupFailed | StorageError => 502,
            Internal => 500,
        }
    }

    /// True when the client must erase all local data and return to the sign-in screen.
    pub fn requires_wipe(self) -> bool {
        matches!(
            self,
            ErrorCode::SessionRevoked
                | ErrorCode::AccountDisabled
                | ErrorCode::PasswordReset
                | ErrorCode::HistoryCleared
        )
    }

    pub fn as_str(self) -> String {
        serde_json::to_value(self)
            .ok()
            .and_then(|v| v.as_str().map(str::to_string))
            .unwrap_or_default()
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct ApiError {
    pub code: ErrorCode,
    pub message: String,
}

/// Body of every non-2xx response.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct ErrorBody {
    pub error: ApiError,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn codes_are_snake_case_and_stable() {
        assert_eq!(ErrorCode::EditWindowClosed.as_str(), "edit_window_closed");
        assert_eq!(ErrorCode::AccountDisabled.as_str(), "account_disabled");
        assert_eq!(ErrorCode::SessionRevoked.as_str(), "session_revoked");
        assert_eq!(ErrorCode::FileTooLarge.as_str(), "file_too_large");
        assert_eq!(ErrorCode::Forbidden.as_str(), "forbidden");
    }

    #[test]
    fn wipe_codes() {
        assert!(ErrorCode::AccountDisabled.requires_wipe());
        assert!(ErrorCode::SessionRevoked.requires_wipe());
        assert!(!ErrorCode::TokenExpired.requires_wipe());
        assert!(!ErrorCode::Forbidden.requires_wipe());
    }
}
