//! Conversation types

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::message::{ConversationType, ParticipantRole};

/// A conversation (direct or group chat)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Conversation {
    pub id: Uuid,
    pub tenant_id: String,
    pub conversation_type: ConversationType,
    pub name: Option<String>,
    pub avatar_url: Option<String>,
    pub created_by: Uuid,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub participant_count: u32,
    pub last_message_id: Option<Uuid>,
    pub last_message_at: Option<DateTime<Utc>>,
}

/// A participant in a conversation
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConversationParticipant {
    pub conversation_id: Uuid,
    pub user_id: Uuid,
    pub tenant_id: String,
    pub role: ParticipantRole,
    pub joined_at: DateTime<Utc>,
    pub left_at: Option<DateTime<Utc>>,
    pub last_read_message_id: Option<Uuid>,
    pub last_read_at: Option<DateTime<Utc>>,
}

/// Direct message lookup entry for O(1) private chat lookups
#[derive(Debug, Clone)]
pub struct DirectMessageEntry {
    pub user_pair_hash: Vec<u8>,
    pub conversation_id: Uuid,
    pub user1_id: Uuid,
    pub user2_id: Uuid,
    pub tenant_id: String,
}
