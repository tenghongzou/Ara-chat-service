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
