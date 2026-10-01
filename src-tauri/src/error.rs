use serde::{ser::SerializeStruct, Serialize, Serializer};
use std::sync::PoisonError;
use thiserror::Error;

/// Application-wide error. Serialized to the frontend as `{ code, message }`.
#[derive(Debug, Error)]
pub enum AppError {
    #[error("{0}")]
    Validation(String),
    #[error("{0}")]
    NotFound(String),
    #[error("{0}")]
    Conflict(String),
    #[error("{0}")]
    Database(String),
    #[error("{0}")]
    Io(String),
    #[error("{0}")]
    Upstream(String),
    #[error("{0}")]
    Internal(String),
}

pub type Result<T> = std::result::Result<T, AppError>;

impl AppError {
    pub fn validation(message: impl Into<String>) -> Self {
        Self::Validation(message.into())
    }

    pub fn not_found(message: impl Into<String>) -> Self {
        Self::NotFound(message.into())
    }

    pub fn conflict(message: impl Into<String>) -> Self {
        Self::Conflict(message.into())
    }

    pub fn upstream(message: impl Into<String>) -> Self {
        Self::Upstream(message.into())
    }

    pub fn internal(message: impl Into<String>) -> Self {
        Self::Internal(message.into())
    }

    pub fn code(&self) -> &'static str {
        match self {
            Self::Validation(_) => "validation",
            Self::NotFound(_) => "not_found",
            Self::Conflict(_) => "conflict",
            Self::Database(_) => "database",
            Self::Io(_) => "io",
            Self::Upstream(_) => "upstream",
            Self::Internal(_) => "internal",
        }
    }
}

impl Serialize for AppError {
    fn serialize<S: Serializer>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error> {
        let mut state = serializer.serialize_struct("AppError", 2)?;
        state.serialize_field("code", self.code())?;
        state.serialize_field("message", &self.to_string())?;
        state.end()
    }
}

impl From<String> for AppError {
    fn from(message: String) -> Self {
        Self::Validation(message)
    }
}

impl From<&str> for AppError {
    fn from(message: &str) -> Self {
        Self::Validation(message.to_owned())
    }
}

impl From<AppError> for String {
    fn from(error: AppError) -> Self {
        error.to_string()
    }
}

impl From<rusqlite::Error> for AppError {
    fn from(error: rusqlite::Error) -> Self {
        Self::Database(error.to_string())
    }
}

impl From<r2d2::Error> for AppError {
    fn from(error: r2d2::Error) -> Self {
        Self::Database(error.to_string())
    }
}

impl From<std::io::Error> for AppError {
    fn from(error: std::io::Error) -> Self {
        Self::Io(error.to_string())
    }
}

impl From<serde_json::Error> for AppError {
    fn from(error: serde_json::Error) -> Self {
        Self::Internal(error.to_string())
    }
}

impl<T> From<PoisonError<T>> for AppError {
    fn from(error: PoisonError<T>) -> Self {
        Self::Internal(error.to_string())
    }
}

impl From<tokio::task::JoinError> for AppError {
    fn from(error: tokio::task::JoinError) -> Self {
        Self::Internal(format!("后台任务失败: {error}"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn serializes_code_and_message() {
        let value = serde_json::to_value(AppError::from("请填写")).unwrap();
        assert_eq!(
            value,
            serde_json::json!({"code":"validation","message":"请填写"})
        );
        let value = serde_json::to_value(AppError::not_found("缺失")).unwrap();
        assert_eq!(value["code"], "not_found");
    }

    #[test]
    fn converts_between_string_and_app_error() {
        let error: AppError = "bad".into();
        let message: String = error.into();
        assert_eq!(message, "bad");
    }
}
