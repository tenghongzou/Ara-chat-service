//! User blocking types

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// Represents a user block relationship
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UserBlock {
    /// User who initiated the block
    pub blocker_id: Uuid,
    /// User who was blocked
    pub blocked_user_id: Uuid,
    /// When the block was created
    pub blocked_at: DateTime<Utc>,
    /// Optional reason for the block
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
}

/// Information about a blocked user (for API responses)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BlockedUserInfo {
    /// The blocked user's ID
    pub user_id: Uuid,
    /// When the user was blocked (milliseconds timestamp)
    pub blocked_at: i64,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_user_block_serialization() {
        let block = UserBlock {
            blocker_id: Uuid::new_v4(),
            blocked_user_id: Uuid::new_v4(),
            blocked_at: Utc::now(),
            reason: Some("spam".to_string()),
        };

        let json = serde_json::to_string(&block).unwrap();
        assert!(json.contains("blocker_id"));
        assert!(json.contains("blocked_user_id"));
        assert!(json.contains("spam"));
    }

    #[test]
    fn test_blocked_user_info_serialization() {
        let info = BlockedUserInfo {
            user_id: Uuid::new_v4(),
            blocked_at: 1704355200000,
        };

        let json = serde_json::to_string(&info).unwrap();
        assert!(json.contains("user_id"));
        assert!(json.contains("1704355200000"));
    }

    #[test]
    fn test_user_block_without_reason() {
        let block = UserBlock {
            blocker_id: Uuid::new_v4(),
            blocked_user_id: Uuid::new_v4(),
            blocked_at: Utc::now(),
            reason: None,
        };

        let json = serde_json::to_string(&block).unwrap();
        assert!(!json.contains("reason"));
    }
}
