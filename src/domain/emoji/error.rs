//! Custom emoji errors

use thiserror::Error;

/// Custom emoji operation errors
#[derive(Debug, Error)]
pub enum EmojiError {
    /// Emoji not found
    #[error("Emoji not found: {0}")]
    NotFound(uuid::Uuid),

    /// Emoji pack not found
    #[error("Emoji pack not found: {0}")]
    PackNotFound(uuid::Uuid),

    /// Shortcode already exists
    #[error("Shortcode '{0}' already exists")]
    ShortcodeExists(String),

    /// Pack name already exists
    #[error("Pack name '{0}' already exists")]
    PackNameExists(String),

    /// Invalid shortcode format
    #[error("Invalid shortcode format: {0}")]
    InvalidShortcode(String),

    /// File too large
    #[error("File size {size} exceeds maximum allowed {max}")]
    FileTooLarge { size: usize, max: usize },

    /// Invalid MIME type
    #[error("MIME type '{0}' is not allowed. Only PNG, GIF, and WebP are supported")]
    InvalidMimeType(String),

    /// Storage error
    #[error("Storage error: {0}")]
    StorageError(String),

    /// Permission denied
    #[error("Permission denied")]
    PermissionDenied,

    /// Image processing error
    #[error("Image processing error: {0}")]
    ImageError(String),

    /// Database error
    #[error("Database error: {0}")]
    DatabaseError(#[from] sqlx::Error),

    /// IO error
    #[error("IO error: {0}")]
    IoError(#[from] std::io::Error),
}

impl EmojiError {
    /// Get error code for API response
    pub fn error_code(&self) -> &'static str {
        match self {
            Self::NotFound(_) => "EMOJI_NOT_FOUND",
            Self::PackNotFound(_) => "PACK_NOT_FOUND",
            Self::ShortcodeExists(_) => "SHORTCODE_EXISTS",
            Self::PackNameExists(_) => "PACK_NAME_EXISTS",
            Self::InvalidShortcode(_) => "INVALID_SHORTCODE",
            Self::FileTooLarge { .. } => "FILE_TOO_LARGE",
            Self::InvalidMimeType(_) => "INVALID_MIME_TYPE",
            Self::StorageError(_) => "STORAGE_ERROR",
            Self::PermissionDenied => "PERMISSION_DENIED",
            Self::ImageError(_) => "IMAGE_ERROR",
            Self::DatabaseError(_) => "DATABASE_ERROR",
            Self::IoError(_) => "IO_ERROR",
        }
    }

    /// Get HTTP status code
    pub fn status_code(&self) -> u16 {
        match self {
            Self::NotFound(_) => 404,
            Self::PackNotFound(_) => 404,
            Self::ShortcodeExists(_) => 409,
            Self::PackNameExists(_) => 409,
            Self::InvalidShortcode(_) => 400,
            Self::FileTooLarge { .. } => 413,
            Self::InvalidMimeType(_) => 415,
            Self::PermissionDenied => 403,
            Self::ImageError(_) => 422,
            _ => 500,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use uuid::Uuid;

    #[test]
    fn test_error_codes() {
        let err = EmojiError::NotFound(Uuid::new_v4());
        assert_eq!(err.error_code(), "EMOJI_NOT_FOUND");
        assert_eq!(err.status_code(), 404);

        let err = EmojiError::ShortcodeExists("test".to_string());
        assert_eq!(err.error_code(), "SHORTCODE_EXISTS");
        assert_eq!(err.status_code(), 409);

        let err = EmojiError::InvalidMimeType("image/bmp".to_string());
        assert_eq!(err.error_code(), "INVALID_MIME_TYPE");
        assert_eq!(err.status_code(), 415);

        let err = EmojiError::FileTooLarge {
            size: 500000,
            max: 262144,
        };
        assert_eq!(err.error_code(), "FILE_TOO_LARGE");
        assert_eq!(err.status_code(), 413);
    }

    #[test]
    fn test_error_display() {
        let err = EmojiError::InvalidShortcode("has spaces".to_string());
        assert!(err.to_string().contains("has spaces"));

        let err = EmojiError::PermissionDenied;
        assert_eq!(err.to_string(), "Permission denied");
    }
}
