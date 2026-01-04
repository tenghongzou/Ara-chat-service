//! Custom emoji types

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// Allowed MIME types for custom emojis
pub const ALLOWED_MIME_TYPES: &[&str] = &["image/png", "image/gif", "image/webp"];

/// Maximum file size for emoji images (256KB)
pub const MAX_EMOJI_SIZE: usize = 262144;

/// Maximum emoji dimensions (will be resized if larger)
pub const MAX_EMOJI_DIMENSION: u32 = 128;

/// Thumbnail size for emoji display
pub const EMOJI_THUMBNAIL_SIZE: u32 = 64;

/// Minimum shortcode length
pub const MIN_SHORTCODE_LENGTH: usize = 2;

/// Maximum shortcode length
pub const MAX_SHORTCODE_LENGTH: usize = 50;

/// Custom emoji pack for grouping related emojis
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EmojiPack {
    pub id: Uuid,
    pub tenant_id: Uuid,
    pub name: String,
    pub description: Option<String>,
    pub creator_id: Uuid,
    pub is_default: bool,
    pub emoji_count: i64,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

/// Custom emoji
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CustomEmoji {
    pub id: Uuid,
    pub tenant_id: Uuid,
    pub pack_id: Option<Uuid>,
    pub shortcode: String,
    pub name: String,
    pub creator_id: Uuid,
    pub image_url: String,
    pub thumbnail_url: Option<String>,
    pub mime_type: String,
    pub file_size: i32,
    pub width: Option<i32>,
    pub height: Option<i32>,
    pub is_animated: bool,
    pub created_at: DateTime<Utc>,
}

/// Database row for custom emoji
#[derive(Debug, Clone, sqlx::FromRow)]
pub struct CustomEmojiRow {
    pub id: Uuid,
    pub tenant_id: Uuid,
    pub pack_id: Option<Uuid>,
    pub shortcode: String,
    pub name: String,
    pub creator_id: Uuid,
    pub image_path: String,
    pub thumbnail_path: Option<String>,
    pub content_hash: String,
    pub storage_backend: String,
    pub mime_type: String,
    pub file_size: i32,
    pub width: Option<i32>,
    pub height: Option<i32>,
    pub is_animated: bool,
    pub created_at: DateTime<Utc>,
}

/// Database row for emoji pack
#[derive(Debug, Clone, sqlx::FromRow)]
pub struct EmojiPackRow {
    pub id: Uuid,
    pub tenant_id: Uuid,
    pub name: String,
    pub description: Option<String>,
    pub creator_id: Uuid,
    pub is_default: bool,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

/// Request to upload a custom emoji
#[derive(Debug, Deserialize)]
pub struct UploadEmojiRequest {
    pub shortcode: String,
    pub name: String,
    pub pack_id: Option<Uuid>,
}

/// Request to create an emoji pack
#[derive(Debug, Deserialize)]
pub struct CreatePackRequest {
    pub name: String,
    pub description: Option<String>,
}

/// Request to update an emoji pack
#[derive(Debug, Deserialize)]
pub struct UpdatePackRequest {
    pub name: Option<String>,
    pub description: Option<String>,
    pub is_default: Option<bool>,
}

/// Emoji search result
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EmojiSearchResult {
    pub custom: Vec<CustomEmoji>,
    pub standard: Vec<StandardEmoji>,
}

/// Standard unicode emoji info (for search)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StandardEmoji {
    pub emoji: String,
    pub name: String,
    pub category: String,
}

/// API response for emoji pack with emoji count
#[derive(Debug, Clone, Serialize)]
pub struct EmojiPackResponse {
    pub id: Uuid,
    pub name: String,
    pub description: Option<String>,
    pub creator_id: Uuid,
    pub is_default: bool,
    pub emoji_count: i64,
    pub created_at: i64,
    pub updated_at: i64,
}

impl EmojiPackResponse {
    pub fn from_row(row: EmojiPackRow, emoji_count: i64) -> Self {
        Self {
            id: row.id,
            name: row.name,
            description: row.description,
            creator_id: row.creator_id,
            is_default: row.is_default,
            emoji_count,
            created_at: row.created_at.timestamp_millis(),
            updated_at: row.updated_at.timestamp_millis(),
        }
    }
}

/// API response for custom emoji
#[derive(Debug, Clone, Serialize)]
pub struct CustomEmojiResponse {
    pub id: Uuid,
    pub pack_id: Option<Uuid>,
    pub shortcode: String,
    pub name: String,
    pub creator_id: Uuid,
    pub image_url: String,
    pub thumbnail_url: Option<String>,
    pub mime_type: String,
    pub width: Option<i32>,
    pub height: Option<i32>,
    pub is_animated: bool,
    pub created_at: i64,
}

impl CustomEmojiResponse {
    pub fn from_emoji(emoji: CustomEmoji) -> Self {
        Self {
            id: emoji.id,
            pack_id: emoji.pack_id,
            shortcode: emoji.shortcode,
            name: emoji.name,
            creator_id: emoji.creator_id,
            image_url: emoji.image_url,
            thumbnail_url: emoji.thumbnail_url,
            mime_type: emoji.mime_type,
            width: emoji.width,
            height: emoji.height,
            is_animated: emoji.is_animated,
            created_at: emoji.created_at.timestamp_millis(),
        }
    }
}

/// Validate shortcode format
///
/// Shortcode must be 2-50 characters, lowercase alphanumeric and underscore only
pub fn is_valid_shortcode(shortcode: &str) -> bool {
    if shortcode.len() < MIN_SHORTCODE_LENGTH || shortcode.len() > MAX_SHORTCODE_LENGTH {
        return false;
    }
    shortcode
        .chars()
        .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_')
}

/// Normalize shortcode (lowercase, trim)
pub fn normalize_shortcode(shortcode: &str) -> String {
    shortcode.trim().to_lowercase()
}

/// Check if MIME type is allowed for emoji
pub fn is_allowed_mime_type(mime_type: &str) -> bool {
    ALLOWED_MIME_TYPES.contains(&mime_type)
}

/// Check if emoji is animated based on MIME type
pub fn is_animated_mime_type(mime_type: &str) -> bool {
    mime_type == "image/gif"
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_valid_shortcode() {
        assert!(is_valid_shortcode("party_parrot"));
        assert!(is_valid_shortcode("thumbsup123"));
        assert!(is_valid_shortcode("ab"));
        assert!(is_valid_shortcode("a_b_c_1_2_3"));
    }

    #[test]
    fn test_invalid_shortcode() {
        assert!(!is_valid_shortcode("")); // empty
        assert!(!is_valid_shortcode("a")); // too short
        assert!(!is_valid_shortcode("has spaces")); // no spaces
        assert!(!is_valid_shortcode("has-dashes")); // no dashes
        assert!(!is_valid_shortcode("HasUpperCase")); // no uppercase
        assert!(!is_valid_shortcode(&"a".repeat(51))); // too long
    }

    #[test]
    fn test_normalize_shortcode() {
        assert_eq!(normalize_shortcode("  PARTY_parrot  "), "party_parrot");
        assert_eq!(normalize_shortcode("ThumbsUp"), "thumbsup");
    }

    #[test]
    fn test_allowed_mime_types() {
        assert!(is_allowed_mime_type("image/png"));
        assert!(is_allowed_mime_type("image/gif"));
        assert!(is_allowed_mime_type("image/webp"));
        assert!(!is_allowed_mime_type("image/jpeg"));
        assert!(!is_allowed_mime_type("image/bmp"));
        assert!(!is_allowed_mime_type("text/plain"));
    }

    #[test]
    fn test_animated_mime_type() {
        assert!(is_animated_mime_type("image/gif"));
        assert!(!is_animated_mime_type("image/png"));
        assert!(!is_animated_mime_type("image/webp"));
    }

    #[test]
    fn test_emoji_pack_response_from_row() {
        let now = Utc::now();
        let row = EmojiPackRow {
            id: Uuid::new_v4(),
            tenant_id: Uuid::new_v4(),
            name: "Test Pack".to_string(),
            description: Some("A test pack".to_string()),
            creator_id: Uuid::new_v4(),
            is_default: false,
            created_at: now,
            updated_at: now,
        };
        let response = EmojiPackResponse::from_row(row.clone(), 5);
        assert_eq!(response.id, row.id);
        assert_eq!(response.emoji_count, 5);
    }

    #[test]
    fn test_custom_emoji_response() {
        let emoji = CustomEmoji {
            id: Uuid::new_v4(),
            tenant_id: Uuid::new_v4(),
            pack_id: None,
            shortcode: "test".to_string(),
            name: "Test".to_string(),
            creator_id: Uuid::new_v4(),
            image_url: "/emojis/test.png".to_string(),
            thumbnail_url: Some("/emojis/test_thumb.png".to_string()),
            mime_type: "image/png".to_string(),
            file_size: 1024,
            width: Some(64),
            height: Some(64),
            is_animated: false,
            created_at: Utc::now(),
        };
        let response = CustomEmojiResponse::from_emoji(emoji.clone());
        assert_eq!(response.shortcode, "test");
        assert_eq!(response.image_url, emoji.image_url);
    }
}
