//! Custom emoji support
//!
//! Provides custom emoji upload, management, and search functionality.
//! Emojis are scoped per tenant for multi-tenant isolation.

mod error;
mod service;
mod types;

pub use error::EmojiError;
pub use service::EmojiService;
pub use types::{
    is_allowed_mime_type, is_animated_mime_type, is_valid_shortcode, normalize_shortcode,
    CreatePackRequest, CustomEmoji, CustomEmojiResponse, CustomEmojiRow, EmojiPack,
    EmojiPackResponse, EmojiPackRow, EmojiSearchResult, StandardEmoji, UpdatePackRequest,
    UploadEmojiRequest, ALLOWED_MIME_TYPES, EMOJI_THUMBNAIL_SIZE, MAX_EMOJI_DIMENSION,
    MAX_EMOJI_SIZE, MAX_SHORTCODE_LENGTH, MIN_SHORTCODE_LENGTH,
};
