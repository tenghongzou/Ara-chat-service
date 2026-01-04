//! Email error types

use axum::http::StatusCode;
use thiserror::Error;
use uuid::Uuid;

/// Email service errors
#[derive(Debug, Error)]
pub enum EmailError {
    #[error("Database error: {0}")]
    Database(#[from] sqlx::Error),

    #[error("Email backend error: {0}")]
    Backend(String),

    #[error("Template error: {0}")]
    Template(String),

    #[error("Configuration error: {0}")]
    Configuration(String),

    #[error("User not found: {0}")]
    UserNotFound(Uuid),

    #[error("No email address for user: {0}")]
    NoEmailAddress(Uuid),

    #[error("User opted out of email notifications")]
    OptedOut,

    #[error("Rate limited: max {max} emails per hour")]
    RateLimited { max: u32 },

    #[error("Quiet hours active")]
    QuietHours,

    #[error("Email service unavailable: {0}")]
    ServiceUnavailable(String),

    #[error("SMTP error: {0}")]
    Smtp(String),

    #[error("SendGrid error: {0}")]
    SendGrid(String),

    #[error("HTTP error: {0}")]
    Http(#[from] reqwest::Error),

    #[error("Invalid email address: {0}")]
    InvalidEmail(String),
}

impl EmailError {
    /// Get error code for API responses
    pub fn code(&self) -> &'static str {
        match self {
            Self::Database(_) => "DATABASE_ERROR",
            Self::Backend(_) => "BACKEND_ERROR",
            Self::Template(_) => "TEMPLATE_ERROR",
            Self::Configuration(_) => "CONFIGURATION_ERROR",
            Self::UserNotFound(_) => "USER_NOT_FOUND",
            Self::NoEmailAddress(_) => "NO_EMAIL_ADDRESS",
            Self::OptedOut => "OPTED_OUT",
            Self::RateLimited { .. } => "RATE_LIMITED",
            Self::QuietHours => "QUIET_HOURS",
            Self::ServiceUnavailable(_) => "SERVICE_UNAVAILABLE",
            Self::Smtp(_) => "SMTP_ERROR",
            Self::SendGrid(_) => "SENDGRID_ERROR",
            Self::Http(_) => "HTTP_ERROR",
            Self::InvalidEmail(_) => "INVALID_EMAIL",
        }
    }

    /// Get HTTP status code for this error
    pub fn status_code(&self) -> StatusCode {
        match self {
            Self::Database(_) => StatusCode::INTERNAL_SERVER_ERROR,
            Self::Backend(_) => StatusCode::BAD_GATEWAY,
            Self::Template(_) => StatusCode::INTERNAL_SERVER_ERROR,
            Self::Configuration(_) => StatusCode::INTERNAL_SERVER_ERROR,
            Self::UserNotFound(_) => StatusCode::NOT_FOUND,
            Self::NoEmailAddress(_) => StatusCode::BAD_REQUEST,
            Self::OptedOut => StatusCode::OK, // Not an error, just skipped
            Self::RateLimited { .. } => StatusCode::TOO_MANY_REQUESTS,
            Self::QuietHours => StatusCode::OK, // Not an error, just delayed
            Self::ServiceUnavailable(_) => StatusCode::SERVICE_UNAVAILABLE,
            Self::Smtp(_) => StatusCode::BAD_GATEWAY,
            Self::SendGrid(_) => StatusCode::BAD_GATEWAY,
            Self::Http(_) => StatusCode::BAD_GATEWAY,
            Self::InvalidEmail(_) => StatusCode::BAD_REQUEST,
        }
    }

    /// Whether this error should trigger a retry
    pub fn is_retryable(&self) -> bool {
        matches!(
            self,
            Self::Backend(_)
                | Self::ServiceUnavailable(_)
                | Self::Smtp(_)
                | Self::SendGrid(_)
                | Self::Http(_)
        )
    }

    /// Whether this error should be logged as a warning (vs debug)
    pub fn is_warning(&self) -> bool {
        matches!(
            self,
            Self::Database(_)
                | Self::Backend(_)
                | Self::Configuration(_)
                | Self::ServiceUnavailable(_)
                | Self::Smtp(_)
                | Self::SendGrid(_)
                | Self::Http(_)
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_error_codes() {
        assert_eq!(EmailError::OptedOut.code(), "OPTED_OUT");
        assert_eq!(EmailError::RateLimited { max: 5 }.code(), "RATE_LIMITED");
    }

    #[test]
    fn test_retryable_errors() {
        assert!(EmailError::Smtp("timeout".into()).is_retryable());
        assert!(EmailError::SendGrid("rate limit".into()).is_retryable());
        assert!(!EmailError::OptedOut.is_retryable());
        assert!(!EmailError::NoEmailAddress(Uuid::nil()).is_retryable());
    }

    #[test]
    fn test_status_codes() {
        assert_eq!(EmailError::OptedOut.status_code(), StatusCode::OK);
        assert_eq!(
            EmailError::RateLimited { max: 5 }.status_code(),
            StatusCode::TOO_MANY_REQUESTS
        );
        assert_eq!(
            EmailError::UserNotFound(Uuid::nil()).status_code(),
            StatusCode::NOT_FOUND
        );
    }
}
