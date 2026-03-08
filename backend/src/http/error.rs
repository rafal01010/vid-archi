use axum::Json;
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use serde::Serialize;
use thiserror::Error;

pub type AppResult<T> = Result<T, AppError>;

#[derive(Debug, Error)]
pub enum AppError {
    #[error("{0}")]
    BadRequest(String),
    #[error("{0}")]
    NotFound(String),
    #[error("{0}")]
    Conflict(String),
    #[error("{0}")]
    Internal(String),
    #[error("database operation failed")]
    Database(#[from] sqlx::Error),
}

impl AppError {
    pub fn bad_request(message: impl Into<String>) -> Self {
        Self::BadRequest(message.into())
    }

    pub fn not_found(message: impl Into<String>) -> Self {
        Self::NotFound(message.into())
    }

    pub fn conflict(message: impl Into<String>) -> Self {
        Self::Conflict(message.into())
    }

    pub fn internal(message: impl Into<String>) -> Self {
        Self::Internal(message.into())
    }

    pub fn internal_with_context(message: impl Into<String>, context: impl Into<String>) -> Self {
        let message = message.into();
        let context = context.into();

        tracing::error!(message = %message, context = %context, "internal application error");
        Self::Internal(message)
    }
}

impl IntoResponse for AppError {
    fn into_response(self) -> Response {
        let (status_code, code, message) = match self {
            AppError::BadRequest(message) => (StatusCode::BAD_REQUEST, "bad_request", message),
            AppError::NotFound(message) => (StatusCode::NOT_FOUND, "not_found", message),
            AppError::Conflict(message) => (StatusCode::CONFLICT, "conflict", message),
            AppError::Internal(message) => (
                StatusCode::INTERNAL_SERVER_ERROR,
                "internal_error",
                message,
            ),
            AppError::Database(error) => {
                tracing::error!(error = %error, "database error");
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    "database_error",
                    "database operation failed".to_owned(),
                )
            }
        };

        (status_code, Json(ErrorResponse::new(code, message))).into_response()
    }
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct ErrorResponse {
    error: ErrorBody,
}

impl ErrorResponse {
    fn new(code: &'static str, message: String) -> Self {
        Self {
            error: ErrorBody { code, message },
        }
    }
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct ErrorBody {
    code: &'static str,
    message: String,
}
