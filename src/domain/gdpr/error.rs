//! GDPR error types

use thiserror::Error;
use uuid::Uuid;

/// Errors that can occur during GDPR operations
#[derive(Debug, Error)]
pub enum GdprError {
    #[error("Database error: {0}")]
    Database(String),

    #[error("User not found: {0}")]
    UserNotFound(Uuid),

    #[error("Request not found: {0}")]
    RequestNotFound(Uuid),

    #[error("Request already processing for user {0}")]
    AlreadyProcessing(Uuid),

    #[error("Export failed: {0}")]
    ExportFailed(String),

    #[error("Deletion failed: {0}")]
    DeletionFailed(String),

    #[error("Storage error: {0}")]
    StorageError(String),

    #[error("Serialization error: {0}")]
    Serialization(String),

    #[error("IO error: {0}")]
    Io(String),

    #[error("Rate limited: please wait {retry_after_seconds} seconds")]
    RateLimited { retry_after_seconds: u64 },

    #[error("Unauthorized: {0}")]
    Unauthorized(String),

    #[error("Forbidden: {0}")]
    Forbidden(String),

    #[error("Service unavailable: {0}")]
    ServiceUnavailable(String),
}

impl From<sqlx::Error> for GdprError {
    fn from(err: sqlx::Error) -> Self {
        GdprError::Database(err.to_string())
    }
}

impl From<std::io::Error> for GdprError {
    fn from(err: std::io::Error) -> Self {
        GdprError::Io(err.to_string())
    }
}

impl From<serde_json::Error> for GdprError {
    fn from(err: serde_json::Error) -> Self {
        GdprError::Serialization(err.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_database_error_display() {
        let err = GdprError::Database("connection failed".to_string());
        assert_eq!(err.to_string(), "Database error: connection failed");
    }

    #[test]
    fn test_user_not_found_display() {
        let user_id = Uuid::parse_str("550e8400-e29b-41d4-a716-446655440000").unwrap();
        let err = GdprError::UserNotFound(user_id);
        assert!(err.to_string().contains("550e8400"));
    }

    #[test]
    fn test_rate_limited_display() {
        let err = GdprError::RateLimited { retry_after_seconds: 3600 };
        assert!(err.to_string().contains("3600"));
    }

    #[test]
    fn test_export_failed_display() {
        let err = GdprError::ExportFailed("disk full".to_string());
        assert_eq!(err.to_string(), "Export failed: disk full");
    }

    #[test]
    fn test_error_debug() {
        let err = GdprError::Forbidden("not owner".to_string());
        let debug = format!("{:?}", err);
        assert!(debug.contains("Forbidden"));
    }
}
