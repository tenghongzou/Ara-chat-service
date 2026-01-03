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
