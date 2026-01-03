//! Validation error types

use thiserror::Error;

/// Validation errors for user input
#[derive(Debug, Error)]
pub enum ValidationError {
    /// Message content exceeds maximum length
    #[error("Message content too long (max {max} characters, got {actual})")]
    ContentTooLong { max: usize, actual: usize },

    /// Conversation name exceeds maximum length
    #[error("Conversation name too long (max {max} characters)")]
    NameTooLong { max: usize },

    /// Invalid emoji format or length
    #[error("Invalid emoji (max {max} characters)")]
    InvalidEmoji { max: usize },

    /// Too many mentions in a single message
    #[error("Too many mentions (max {max})")]
    TooManyMentions { max: usize },

    /// Too many participants in conversation
    #[error("Too many participants (max {max})")]
    TooManyParticipants { max: usize },

    /// JWT secret is too short
    #[error("JWT secret too short (min {min} characters required for security)")]
    JwtSecretTooShort { min: usize },

    /// Empty content not allowed
    #[error("Content cannot be empty")]
    EmptyContent,
}

impl ValidationError {
    /// Get the error code for API responses
    pub fn code(&self) -> &'static str {
        match self {
            Self::ContentTooLong { .. } => "CONTENT_TOO_LONG",
            Self::NameTooLong { .. } => "NAME_TOO_LONG",
            Self::InvalidEmoji { .. } => "INVALID_EMOJI",
            Self::TooManyMentions { .. } => "TOO_MANY_MENTIONS",
            Self::TooManyParticipants { .. } => "TOO_MANY_PARTICIPANTS",
            Self::JwtSecretTooShort { .. } => "JWT_SECRET_TOO_SHORT",
            Self::EmptyContent => "EMPTY_CONTENT",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_error_code_content_too_long() {
        let err = ValidationError::ContentTooLong {
            max: 10_000,
            actual: 15_000,
        };
        assert_eq!(err.code(), "CONTENT_TOO_LONG");
    }

    #[test]
    fn test_error_code_name_too_long() {
        let err = ValidationError::NameTooLong { max: 100 };
        assert_eq!(err.code(), "NAME_TOO_LONG");
    }

    #[test]
    fn test_error_code_invalid_emoji() {
        let err = ValidationError::InvalidEmoji { max: 32 };
        assert_eq!(err.code(), "INVALID_EMOJI");
    }

    #[test]
    fn test_error_code_too_many_mentions() {
        let err = ValidationError::TooManyMentions { max: 50 };
        assert_eq!(err.code(), "TOO_MANY_MENTIONS");
    }

    #[test]
    fn test_error_code_too_many_participants() {
        let err = ValidationError::TooManyParticipants { max: 500 };
        assert_eq!(err.code(), "TOO_MANY_PARTICIPANTS");
    }

    #[test]
    fn test_error_code_jwt_secret_too_short() {
        let err = ValidationError::JwtSecretTooShort { min: 32 };
        assert_eq!(err.code(), "JWT_SECRET_TOO_SHORT");
    }

    #[test]
    fn test_error_code_empty_content() {
        let err = ValidationError::EmptyContent;
        assert_eq!(err.code(), "EMPTY_CONTENT");
    }

    #[test]
    fn test_error_display_content_too_long() {
        let err = ValidationError::ContentTooLong {
            max: 10_000,
            actual: 15_000,
        };
        let msg = err.to_string();
        assert!(msg.contains("10000"));
        assert!(msg.contains("15000"));
        assert!(msg.contains("too long"));
    }

    #[test]
    fn test_error_display_name_too_long() {
        let err = ValidationError::NameTooLong { max: 100 };
        let msg = err.to_string();
        assert!(msg.contains("100"));
        assert!(msg.contains("name"));
    }

    #[test]
    fn test_error_display_jwt_secret() {
        let err = ValidationError::JwtSecretTooShort { min: 32 };
        let msg = err.to_string();
        assert!(msg.contains("32"));
        assert!(msg.contains("security"));
    }

    #[test]
    fn test_error_debug_impl() {
        let err = ValidationError::EmptyContent;
        let debug = format!("{:?}", err);
        assert!(debug.contains("EmptyContent"));
    }
}
