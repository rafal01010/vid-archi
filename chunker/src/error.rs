use thiserror::Error;

pub type AppResult<T> = Result<T, AppError>;

#[derive(Debug, Error)]
pub enum AppError {
    #[error("{0}")]
    Config(String),
    #[error("{0}")]
    Conflict(String),
    #[error("{0}")]
    Internal(String),
    #[error("database operation failed")]
    Database(#[from] sqlx::Error),
    #[error("io operation failed")]
    Io(#[from] std::io::Error),
}

impl AppError {
    pub fn config(message: impl Into<String>) -> Self {
        Self::Config(message.into())
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

        tracing::error!(message = %message, context = %context, "chunker internal error");
        Self::Internal(message)
    }
}
