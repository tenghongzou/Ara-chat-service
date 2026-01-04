//! Attachment errors

use thiserror::Error;

use crate::conversation::ConversationError;

/// Attachment operation errors
#[derive(Debug, Error)]
pub enum AttachmentError {
    /// File too large
    #[error("File size {size} exceeds maximum allowed {max}")]
    FileTooLarge { size: usize, max: usize },

    /// Invalid MIME type
    #[error("MIME type '{0}' is not allowed")]
    InvalidMimeType(String),

    /// Storage error
    #[error("Storage error: {0}")]
    StorageError(String),

    /// Attachment not found
    #[error("Attachment not found: {0}")]
    NotFound(uuid::Uuid),

    /// Permission denied
    #[error("Permission denied")]
    PermissionDenied,

    /// Invalid file name
    #[error("Invalid file name: {0}")]
    InvalidFileName(String),

    /// Thumbnail generation failed
    #[error("Thumbnail generation failed: {0}")]
    ThumbnailError(String),

    /// Database error
    #[error("Database error: {0}")]
    DatabaseError(#[from] sqlx::Error),

    /// IO error
    #[error("IO error: {0}")]
    IoError(#[from] std::io::Error),

    /// User is not a participant
    #[error("User is not a participant of the conversation")]
    NotParticipant,
}

impl AttachmentError {
    /// Get error code for API response
    pub fn error_code(&self) -> &'static str {
        match self {
            Self::FileTooLarge { .. } => "FILE_TOO_LARGE",
            Self::InvalidMimeType(_) => "INVALID_MIME_TYPE",
            Self::StorageError(_) => "STORAGE_ERROR",
            Self::NotFound(_) => "NOT_FOUND",
            Self::PermissionDenied => "PERMISSION_DENIED",
            Self::InvalidFileName(_) => "INVALID_FILE_NAME",
            Self::ThumbnailError(_) => "THUMBNAIL_ERROR",
            Self::DatabaseError(_) => "DATABASE_ERROR",
            Self::IoError(_) => "IO_ERROR",
            Self::NotParticipant => "NOT_PARTICIPANT",
        }
    }

    /// Get HTTP status code
    pub fn status_code(&self) -> u16 {
        match self {
            Self::FileTooLarge { .. } => 413, // Payload Too Large
            Self::InvalidMimeType(_) => 415,  // Unsupported Media Type
            Self::NotFound(_) => 404,
            Self::PermissionDenied => 403,
            Self::NotParticipant => 403,
            Self::InvalidFileName(_) => 400,
            _ => 500,
        }
    }
}

impl From<ConversationError> for AttachmentError {
    fn from(err: ConversationError) -> Self {
        match err {
            ConversationError::NotFound => AttachmentError::NotParticipant,
            ConversationError::NotParticipant => AttachmentError::NotParticipant,
            ConversationError::Database(msg) => AttachmentError::StorageError(msg),
        }
    }
}
