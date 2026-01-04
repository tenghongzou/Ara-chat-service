//! Unified API error handling
//!
//! Provides consistent error responses across REST and WebSocket APIs.

use axum::{
    http::StatusCode,
    response::{IntoResponse, Response},
    Json,
};
use serde::Serialize;

use crate::infrastructure::auth::JwtError;
use crate::domain::conversation::ConversationError;
use crate::domain::message::{MessageHandlerError, RouterError, StorageError};
use crate::domain::validation::ValidationError;

/// Standard API error response format
#[derive(Debug, Serialize)]
pub struct ErrorResponse {
    /// Machine-readable error code (e.g., "UNAUTHORIZED", "NOT_FOUND")
    pub code: &'static str,
    /// Human-readable error message
    pub message: String,
    /// Optional request ID for tracing
    #[serde(skip_serializing_if = "Option::is_none")]
    pub request_id: Option<String>,
}

impl ErrorResponse {
    pub fn new(code: &'static str, message: impl Into<String>) -> Self {
        Self {
            code,
            message: message.into(),
            request_id: None,
        }
    }

    pub fn with_request_id(mut self, request_id: Option<String>) -> Self {
        self.request_id = request_id;
        self
    }
}

/// Unified API error type that implements IntoResponse
#[derive(Debug)]
pub enum ApiError {
    // Authentication errors (401)
    Unauthorized(&'static str),
    InvalidToken(String),

    // Authorization errors (403)
    Forbidden(&'static str),
    NotParticipant,
    NotOwner,

    // Not found errors (404)
    NotFound(&'static str),
    MessageNotFound,
    ConversationNotFound,

    // Validation errors (400)
    BadRequest(&'static str, String),
    Validation(ValidationError),
    InvalidReplyTarget,

    // Conflict errors (409)
    Conflict(&'static str),
    AlreadyRecalled,

    // Rate limiting (429)
    RateLimited,

    // Business logic errors (422)
    WindowExpired { kind: &'static str, allowed_seconds: u64 },

    // Service unavailable (503)
    ServiceUnavailable(&'static str),

    // Internal errors (500)
    Internal(String),
    Storage(StorageError),
    Routing(RouterError),
}

impl ApiError {
    /// Get the error code for this error
    pub fn code(&self) -> &'static str {
        match self {
            Self::Unauthorized(_) => "UNAUTHORIZED",
            Self::InvalidToken(_) => "INVALID_TOKEN",
            Self::Forbidden(_) => "FORBIDDEN",
            Self::NotParticipant => "NOT_PARTICIPANT",
            Self::NotOwner => "NOT_OWNER",
            Self::NotFound(_) => "NOT_FOUND",
            Self::MessageNotFound => "MESSAGE_NOT_FOUND",
            Self::ConversationNotFound => "CONVERSATION_NOT_FOUND",
            Self::BadRequest(code, _) => code,
            Self::Validation(v) => v.code(),
            Self::InvalidReplyTarget => "INVALID_REPLY_TARGET",
            Self::Conflict(_) => "CONFLICT",
            Self::AlreadyRecalled => "ALREADY_RECALLED",
            Self::RateLimited => "RATE_LIMITED",
            Self::WindowExpired { kind, .. } => match *kind {
                "recall" => "RECALL_WINDOW_EXPIRED",
                "edit" => "EDIT_WINDOW_EXPIRED",
                _ => "WINDOW_EXPIRED",
            },
            Self::ServiceUnavailable(_) => "SERVICE_UNAVAILABLE",
            Self::Internal(_) => "INTERNAL_ERROR",
            Self::Storage(_) => "STORAGE_ERROR",
            Self::Routing(_) => "ROUTING_ERROR",
        }
    }

    /// Get the HTTP status code for this error
    pub fn status_code(&self) -> StatusCode {
        match self {
            Self::Unauthorized(_) | Self::InvalidToken(_) => StatusCode::UNAUTHORIZED,
            Self::Forbidden(_) | Self::NotParticipant | Self::NotOwner => StatusCode::FORBIDDEN,
            Self::NotFound(_) | Self::MessageNotFound | Self::ConversationNotFound => {
                StatusCode::NOT_FOUND
            }
            Self::BadRequest(_, _) | Self::Validation(_) | Self::InvalidReplyTarget => StatusCode::BAD_REQUEST,
            Self::Conflict(_) | Self::AlreadyRecalled => StatusCode::CONFLICT,
            Self::RateLimited => StatusCode::TOO_MANY_REQUESTS,
            Self::WindowExpired { .. } => StatusCode::UNPROCESSABLE_ENTITY,
            Self::ServiceUnavailable(_) => StatusCode::SERVICE_UNAVAILABLE,
            Self::Internal(_) | Self::Storage(_) | Self::Routing(_) => {
                StatusCode::INTERNAL_SERVER_ERROR
            }
        }
    }

    /// Get the error message
    pub fn message(&self) -> String {
        match self {
            Self::Unauthorized(msg) => msg.to_string(),
            Self::InvalidToken(msg) => msg.clone(),
            Self::Forbidden(msg) => msg.to_string(),
            Self::NotParticipant => "User is not a participant in this conversation".to_string(),
            Self::NotOwner => "User is not the owner of this resource".to_string(),
            Self::NotFound(msg) => msg.to_string(),
            Self::MessageNotFound => "Message not found".to_string(),
            Self::ConversationNotFound => "Conversation not found".to_string(),
            Self::BadRequest(_, msg) => msg.clone(),
            Self::Validation(v) => v.to_string(),
            Self::InvalidReplyTarget => "Invalid reply target: message not found, deleted, or in different conversation".to_string(),
            Self::Conflict(msg) => msg.to_string(),
            Self::AlreadyRecalled => "Message has already been recalled".to_string(),
            Self::RateLimited => "Too many requests, please try again later".to_string(),
            Self::WindowExpired { kind, allowed_seconds } => {
                format!("{} window expired (allowed: {} seconds)", kind, allowed_seconds)
            }
            Self::ServiceUnavailable(msg) => msg.to_string(),
            Self::Internal(msg) => msg.clone(),
            Self::Storage(e) => e.to_string(),
            Self::Routing(e) => e.to_string(),
        }
    }
}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        let status = self.status_code();
        let body = ErrorResponse::new(self.code(), self.message());

        (status, Json(body)).into_response()
    }
}

// Implement From traits for automatic conversion

impl From<JwtError> for ApiError {
    fn from(err: JwtError) -> Self {
        match err {
            JwtError::Validation(msg) => Self::InvalidToken(msg),
            JwtError::InvalidSubject => Self::InvalidToken("Invalid subject in token".to_string()),
            JwtError::SecretTooShort { .. } => Self::Internal("JWT configuration error".to_string()),
        }
    }
}

impl From<ValidationError> for ApiError {
    fn from(err: ValidationError) -> Self {
        Self::Validation(err)
    }
}

impl From<StorageError> for ApiError {
    fn from(err: StorageError) -> Self {
        match err {
            StorageError::NotFound => Self::MessageNotFound,
            _ => Self::Storage(err),
        }
    }
}

impl From<RouterError> for ApiError {
    fn from(err: RouterError) -> Self {
        Self::Routing(err)
    }
}

impl From<ConversationError> for ApiError {
    fn from(err: ConversationError) -> Self {
        match err {
            ConversationError::NotFound => Self::ConversationNotFound,
            ConversationError::NotParticipant => Self::NotParticipant,
            _ => Self::Internal(err.to_string()),
        }
    }
}

impl From<MessageHandlerError> for ApiError {
    fn from(err: MessageHandlerError) -> Self {
        match err {
            MessageHandlerError::NotParticipant => Self::NotParticipant,
            MessageHandlerError::MessageNotFound => Self::MessageNotFound,
            MessageHandlerError::NotMessageOwner => Self::NotOwner,
            MessageHandlerError::MessageRecalled => Self::AlreadyRecalled,
            MessageHandlerError::RecallWindowExpired { allowed_seconds } => Self::WindowExpired {
                kind: "recall",
                allowed_seconds,
            },
            MessageHandlerError::EditWindowExpired { allowed_seconds } => Self::WindowExpired {
                kind: "edit",
                allowed_seconds,
            },
            MessageHandlerError::InvalidReplyTarget => Self::InvalidReplyTarget,
            MessageHandlerError::InsufficientPinPermission => Self::Forbidden("Insufficient permission to pin/unpin messages"),
            MessageHandlerError::Storage(e) => Self::from(e),
            MessageHandlerError::Routing(e) => Self::from(e),
            MessageHandlerError::Conversation(e) => Self::from(e),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_error_response_new() {
        let resp = ErrorResponse::new("TEST_CODE", "Test message");
        assert_eq!(resp.code, "TEST_CODE");
        assert_eq!(resp.message, "Test message");
        assert!(resp.request_id.is_none());
    }

    #[test]
    fn test_error_response_with_request_id() {
        let resp = ErrorResponse::new("TEST", "Test")
            .with_request_id(Some("req-123".to_string()));
        assert_eq!(resp.request_id, Some("req-123".to_string()));
    }

    #[test]
    fn test_api_error_unauthorized_code() {
        let err = ApiError::Unauthorized("test");
        assert_eq!(err.code(), "UNAUTHORIZED");
        assert_eq!(err.status_code(), StatusCode::UNAUTHORIZED);
    }

    #[test]
    fn test_api_error_not_participant() {
        let err = ApiError::NotParticipant;
        assert_eq!(err.code(), "NOT_PARTICIPANT");
        assert_eq!(err.status_code(), StatusCode::FORBIDDEN);
    }

    #[test]
    fn test_api_error_message_not_found() {
        let err = ApiError::MessageNotFound;
        assert_eq!(err.code(), "MESSAGE_NOT_FOUND");
        assert_eq!(err.status_code(), StatusCode::NOT_FOUND);
    }

    #[test]
    fn test_api_error_validation() {
        let validation_err = ValidationError::ContentTooLong {
            max: 10000,
            actual: 15000,
        };
        let err = ApiError::Validation(validation_err);
        assert_eq!(err.code(), "CONTENT_TOO_LONG");
        assert_eq!(err.status_code(), StatusCode::BAD_REQUEST);
    }

    #[test]
    fn test_api_error_window_expired() {
        let err = ApiError::WindowExpired {
            kind: "recall",
            allowed_seconds: 120,
        };
        assert_eq!(err.code(), "RECALL_WINDOW_EXPIRED");
        assert_eq!(err.status_code(), StatusCode::UNPROCESSABLE_ENTITY);
        assert!(err.message().contains("120"));
    }

    #[test]
    fn test_api_error_rate_limited() {
        let err = ApiError::RateLimited;
        assert_eq!(err.code(), "RATE_LIMITED");
        assert_eq!(err.status_code(), StatusCode::TOO_MANY_REQUESTS);
    }

    #[test]
    fn test_from_message_handler_error() {
        let handler_err = MessageHandlerError::NotParticipant;
        let api_err: ApiError = handler_err.into();
        assert_eq!(api_err.code(), "NOT_PARTICIPANT");
    }

    #[test]
    fn test_from_storage_error_not_found() {
        let storage_err = StorageError::NotFound;
        let api_err: ApiError = storage_err.into();
        assert_eq!(api_err.code(), "MESSAGE_NOT_FOUND");
    }

    #[test]
    fn test_error_response_serialization() {
        let resp = ErrorResponse::new("TEST", "Test message");
        let json = serde_json::to_string(&resp).unwrap();
        assert!(json.contains("TEST"));
        assert!(json.contains("Test message"));
        // request_id should not appear when None
        assert!(!json.contains("request_id"));
    }

    #[test]
    fn test_error_response_serialization_with_request_id() {
        let resp = ErrorResponse::new("TEST", "Test")
            .with_request_id(Some("req-abc".to_string()));
        let json = serde_json::to_string(&resp).unwrap();
        assert!(json.contains("request_id"));
        assert!(json.contains("req-abc"));
    }
}
