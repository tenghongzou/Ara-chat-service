//! Validation limits and constants

/// Maximum message content length (10KB)
pub const MAX_MESSAGE_LENGTH: usize = 10_000;

/// Maximum conversation name length
pub const MAX_CONVERSATION_NAME_LENGTH: usize = 100;

/// Maximum emoji length (for reactions)
pub const MAX_EMOJI_LENGTH: usize = 32;

/// Maximum mentions per message
pub const MAX_MENTIONS_PER_MESSAGE: usize = 50;

/// Maximum participants per conversation
pub const MAX_PARTICIPANTS_PER_CONVERSATION: usize = 500;

/// Minimum JWT secret length (256 bits)
pub const MIN_JWT_SECRET_LENGTH: usize = 32;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_max_message_length_is_10000() {
        // 10KB limit for message content
        assert_eq!(MAX_MESSAGE_LENGTH, 10_000);
        // Verify it's reasonable for chat (not too small, not too large)
        assert!(MAX_MESSAGE_LENGTH >= 1_000, "Message limit too small");
        assert!(MAX_MESSAGE_LENGTH <= 100_000, "Message limit too large");
    }

    #[test]
    fn test_max_conversation_name_is_100() {
        assert_eq!(MAX_CONVERSATION_NAME_LENGTH, 100);
        // Name should fit in a typical UI display
        assert!(MAX_CONVERSATION_NAME_LENGTH >= 10);
        assert!(MAX_CONVERSATION_NAME_LENGTH <= 255);
    }

    #[test]
    fn test_max_mentions_is_50() {
        assert_eq!(MAX_MENTIONS_PER_MESSAGE, 50);
        // Should allow reasonable group mentions but prevent spam
        assert!(MAX_MENTIONS_PER_MESSAGE >= 10);
        assert!(MAX_MENTIONS_PER_MESSAGE <= 100);
    }

    #[test]
    fn test_max_participants_is_500() {
        assert_eq!(MAX_PARTICIPANTS_PER_CONVERSATION, 500);
        // Should support large groups but have reasonable limits
        assert!(MAX_PARTICIPANTS_PER_CONVERSATION >= 100);
        assert!(MAX_PARTICIPANTS_PER_CONVERSATION <= 10_000);
    }

    #[test]
    fn test_min_jwt_secret_is_32() {
        // 32 bytes = 256 bits (industry standard for HMAC-SHA256)
        assert_eq!(MIN_JWT_SECRET_LENGTH, 32);
        // Must be at least 256 bits for security
        assert!(MIN_JWT_SECRET_LENGTH >= 32);
    }

    #[test]
    fn test_max_emoji_length_is_32() {
        // Allow for complex emoji sequences (ZWJ, skin tones)
        assert_eq!(MAX_EMOJI_LENGTH, 32);
        assert!(MAX_EMOJI_LENGTH >= 4, "Must allow basic emoji");
        assert!(MAX_EMOJI_LENGTH <= 64, "Should limit emoji abuse");
    }
}
