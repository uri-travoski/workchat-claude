use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::Json;
use proto::{ApiError, ErrorBody, ErrorCode};

#[derive(Debug, Clone)]
pub struct AppError {
    pub code: ErrorCode,
    pub message: String,
}

pub type AppResult<T> = Result<T, AppError>;

impl AppError {
    pub fn new(code: ErrorCode, message: impl Into<String>) -> Self {
        AppError { code, message: message.into() }
    }
    pub fn internal(message: impl Into<String>) -> Self {
        let m: String = message.into();
        tracing::error!("internal error: {m}");
        AppError { code: ErrorCode::Internal, message: "internal error".into() }
    }
    pub fn forbidden(m: impl Into<String>) -> Self {
        Self::new(ErrorCode::Forbidden, m)
    }
    pub fn not_found(m: impl Into<String>) -> Self {
        Self::new(ErrorCode::NotFound, m)
    }
    pub fn validation(m: impl Into<String>) -> Self {
        Self::new(ErrorCode::Validation, m)
    }
    pub fn conflict(m: impl Into<String>) -> Self {
        Self::new(ErrorCode::Conflict, m)
    }
    pub fn storage(m: impl Into<String>) -> Self {
        Self::new(ErrorCode::StorageError, m)
    }
}

impl std::fmt::Display for AppError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}: {}", self.code.as_str(), self.message)
    }
}

impl std::error::Error for AppError {}

impl From<rusqlite::Error> for AppError {
    fn from(e: rusqlite::Error) -> Self {
        match e {
            rusqlite::Error::QueryReturnedNoRows => AppError::not_found("not found"),
            e => AppError::internal(format!("sqlite: {e}")),
        }
    }
}

impl From<anyhow::Error> for AppError {
    fn from(e: anyhow::Error) -> Self {
        AppError::internal(format!("{e:#}"))
    }
}

impl From<serde_json::Error> for AppError {
    fn from(e: serde_json::Error) -> Self {
        AppError::internal(format!("json: {e}"))
    }
}

impl IntoResponse for AppError {
    fn into_response(self) -> Response {
        let status = StatusCode::from_u16(self.code.http_status()).unwrap_or(StatusCode::INTERNAL_SERVER_ERROR);
        let body = ErrorBody { error: ApiError { code: self.code, message: self.message } };
        (status, Json(body)).into_response()
    }
}
