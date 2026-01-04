//! Link Preview error types

use thiserror::Error;

/// Errors that can occur during link preview operations
#[derive(Debug, Error)]
pub enum LinkPreviewError {
    #[error("Database error: {0}")]
    Database(#[from] sqlx::Error),

    #[error("HTTP request error: {0}")]
    Http(#[from] reqwest::Error),

    #[error("Redis error: {0}")]
    Redis(String),

    #[error("URL parse error: {0}")]
    UrlParse(String),

    #[error("HTML parse error: {0}")]
    HtmlParse(String),

    #[error("Fetch timeout")]
    Timeout,

    #[error("Content too large: {size} bytes (max: {max})")]
    ContentTooLarge { size: usize, max: usize },

    #[error("Circuit breaker open")]
    CircuitBreakerOpen,

    #[error("Rate limited")]
    RateLimited,

    #[error("Invalid URL scheme: {0}")]
    InvalidScheme(String),

    #[error("Private IP address not allowed")]
    PrivateIpNotAllowed,

    #[error("Preview not found")]
    NotFound,

    #[error("Message not found")]
    MessageNotFound,
}

impl LinkPreviewError {
    /// Get the error code for API responses
    pub fn code(&self) -> &'static str {
        match self {
            Self::Database(_) => "DATABASE_ERROR",
            Self::Http(_) => "HTTP_ERROR",
            Self::Redis(_) => "REDIS_ERROR",
            Self::UrlParse(_) => "URL_PARSE_ERROR",
            Self::HtmlParse(_) => "HTML_PARSE_ERROR",
            Self::Timeout => "TIMEOUT",
            Self::ContentTooLarge { .. } => "CONTENT_TOO_LARGE",
            Self::CircuitBreakerOpen => "CIRCUIT_BREAKER_OPEN",
            Self::RateLimited => "RATE_LIMITED",
            Self::InvalidScheme(_) => "INVALID_SCHEME",
            Self::PrivateIpNotAllowed => "PRIVATE_IP_NOT_ALLOWED",
            Self::NotFound => "PREVIEW_NOT_FOUND",
            Self::MessageNotFound => "MESSAGE_NOT_FOUND",
        }
    }
}
