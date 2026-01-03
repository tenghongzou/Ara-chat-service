//! Core message types for the chat protocol

use std::collections::HashMap;
use std::sync::Arc;

use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// Messages sent from client to server
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", content = "payload")]
pub enum ClientMessage {
    /// Authenticate with JWT token
    Authenticate { token: String },

    /// Send a chat message
    SendMessage {
        conversation_id: Uuid,
        content: String,
        content_type: ContentType,
        #[serde(skip_serializing_if = "Option::is_none")]
        reply_to: Option<Uuid>,
        #[serde(skip_serializing_if = "Option::is_none")]
        client_message_id: Option<String>,
        #[serde(default)]
        mentions: Vec<Uuid>,
    },

    /// Mark messages as read up to a specific message
    MarkRead {
        conversation_id: Uuid,
        message_id: Uuid,
    },

    /// Recall (delete) a sent message
    RecallMessage {
        conversation_id: Uuid,
        message_id: Uuid,
    },

    /// Edit a sent message
    EditMessage {
        conversation_id: Uuid,
        message_id: Uuid,
        new_content: String,
    },

    /// Toggle emoji reaction on a message
    ToggleReaction {
        conversation_id: Uuid,
        message_id: Uuid,
        emoji: String,
    },

    /// Indicate typing status
    Typing {
        conversation_id: Uuid,
        is_typing: bool,
    },

    /// Fetch message history for a conversation
    FetchHistory {
        conversation_id: Uuid,
        #[serde(skip_serializing_if = "Option::is_none")]
        before: Option<Uuid>,
        #[serde(skip_serializing_if = "Option::is_none")]
        limit: Option<u32>,
    },

    /// Fetch user's conversation list
    FetchConversations {
        #[serde(skip_serializing_if = "Option::is_none")]
        before: Option<i64>,
        #[serde(skip_serializing_if = "Option::is_none")]
        limit: Option<u32>,
    },

    /// Create a new conversation
    CreateConversation {
        conversation_type: ConversationType,
        participants: Vec<Uuid>,
        #[serde(skip_serializing_if = "Option::is_none")]
        name: Option<String>,
    },

    /// Update presence status
    UpdatePresence { status: PresenceStatus },

    /// Subscribe to presence updates for specific users
    SubscribePresence { user_ids: Vec<Uuid> },

    /// Unsubscribe from presence updates
    UnsubscribePresence { user_ids: Vec<Uuid> },

    /// Request unread count sync
    SyncUnread,

    /// Get reactions for specific messages
    GetReactions {
        message_ids: Vec<Uuid>,
    },

    /// Ping for keepalive
    Ping,
}

/// Messages sent from server to client
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum ServerMessage {
    /// Authentication successful
    #[serde(rename = "authenticated")]
    Authenticated { user_id: Uuid },

    /// New chat message received
    #[serde(rename = "message")]
    Message { message: ChatMessage },

    /// Confirmation that message was sent
    #[serde(rename = "message_sent")]
    MessageSent {
        conversation_id: Uuid,
        message_id: Uuid,
        #[serde(skip_serializing_if = "Option::is_none")]
        client_message_id: Option<String>,
        created_at: i64,
    },

    /// Read receipt update
    #[serde(rename = "read_receipt")]
    ReadReceipt {
        conversation_id: Uuid,
        user_id: Uuid,
        message_id: Uuid,
        read_at: i64,
    },

    /// Message was recalled
    #[serde(rename = "message_recalled")]
    MessageRecalled {
        conversation_id: Uuid,
        message_id: Uuid,
        recalled_by: Uuid,
    },

    /// Message was edited
    #[serde(rename = "message_edited")]
    MessageEdited {
        conversation_id: Uuid,
        message_id: Uuid,
        new_content: String,
        edited_at: i64,
        mentions: Vec<Uuid>,
    },

    /// Reaction update on a message
    #[serde(rename = "reaction_update")]
    ReactionUpdate {
        conversation_id: Uuid,
        message_id: Uuid,
        user_id: Uuid,
        emoji: String,
        action: ReactionAction,
    },

    /// User typing indicator
    #[serde(rename = "typing")]
    Typing {
        conversation_id: Uuid,
        user_id: Uuid,
        is_typing: bool,
    },

    /// Message history response
    #[serde(rename = "history")]
    History {
        conversation_id: Uuid,
        messages: Vec<ChatMessage>,
        has_more: bool,
    },

    /// Conversation list response
    #[serde(rename = "conversations")]
    Conversations {
        conversations: Vec<ConversationSummary>,
        has_more: bool,
    },

    /// New conversation created
    #[serde(rename = "conversation_created")]
    ConversationCreated { conversation: ConversationSummary },

    /// User presence update
    #[serde(rename = "presence")]
    Presence {
        user_id: Uuid,
        status: PresenceStatus,
        #[serde(skip_serializing_if = "Option::is_none")]
        last_seen: Option<i64>,
    },

    /// User was mentioned in a message
    #[serde(rename = "mention")]
    Mention {
        conversation_id: Uuid,
        message_id: Uuid,
        sender_id: Uuid,
        sender_name: String,
        preview: String,
    },

    /// Unread count sync
    #[serde(rename = "unread_sync")]
    UnreadSync {
        total: u64,
        per_conversation: HashMap<Uuid, u64>,
    },

    /// Reactions response for multiple messages
    #[serde(rename = "reactions")]
    Reactions {
        reactions: HashMap<Uuid, Vec<ReactionInfo>>,
    },

    /// Pong response to ping
    #[serde(rename = "pong")]
    Pong,

    /// Error response
    #[serde(rename = "error")]
    Error { code: String, message: String },

    /// Server shutdown notification
    #[serde(rename = "shutdown")]
    Shutdown {
        reason: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        reconnect_after_seconds: Option<u64>,
    },
}

impl ServerMessage {
    pub fn error(code: impl Into<String>, message: impl Into<String>) -> Self {
        Self::Error {
            code: code.into(),
            message: message.into(),
        }
    }

    pub fn authenticated(user_id: Uuid) -> Self {
        Self::Authenticated { user_id }
    }

    pub fn shutdown(reason: impl Into<String>, reconnect_after_seconds: Option<u64>) -> Self {
        Self::Shutdown {
            reason: reason.into(),
            reconnect_after_seconds,
        }
    }
}

/// Chat message content type
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ContentType {
    Text,
    Image,
    File,
    System,
}

impl Default for ContentType {
    fn default() -> Self {
        Self::Text
    }
}

/// Conversation type
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ConversationType {
    Direct,
    Group,
}

/// User presence status
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum PresenceStatus {
    Online,
    Away,
    Busy,
    Offline,
}

impl Default for PresenceStatus {
    fn default() -> Self {
        Self::Offline
    }
}

/// Reaction action type
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ReactionAction {
    Add,
    Remove,
}

/// A chat message
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChatMessage {
    pub id: Uuid,
    pub conversation_id: Uuid,
    pub sender_id: Uuid,
    pub content: String,
    pub content_type: ContentType,
    pub created_at: i64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub updated_at: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reply_to_id: Option<Uuid>,
    #[serde(default)]
    pub mentions: Vec<Uuid>,
    #[serde(default)]
    pub reactions: HashMap<String, Vec<Uuid>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub recalled_at: Option<i64>,
}

/// Conversation summary for list view
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConversationSummary {
    pub id: Uuid,
    pub conversation_type: ConversationType,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub avatar_url: Option<String>,
    pub participant_count: u32,
    pub participants: Vec<ParticipantInfo>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub last_message: Option<LastMessagePreview>,
    pub unread_count: u64,
    pub updated_at: i64,
}

/// Participant info in a conversation
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ParticipantInfo {
    pub user_id: Uuid,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub avatar_url: Option<String>,
    pub role: ParticipantRole,
}

/// Participant role in a conversation
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ParticipantRole {
    Owner,
    Admin,
    Member,
}

impl Default for ParticipantRole {
    fn default() -> Self {
        Self::Member
    }
}

/// Last message preview for conversation list
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LastMessagePreview {
    pub message_id: Uuid,
    pub sender_id: Uuid,
    pub content_preview: String,
    pub content_type: ContentType,
    pub created_at: i64,
}

/// Reaction info for a message
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReactionInfo {
    pub emoji: String,
    pub count: u32,
    pub users: Vec<Uuid>,
    pub user_reacted: bool,
}

/// Unread sync data for REST API response
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UnreadSyncData {
    pub total: u64,
    pub per_conversation: HashMap<Uuid, u64>,
}

/// Outbound message wrapper for efficient multi-send scenarios
#[derive(Debug, Clone)]
pub enum OutboundMessage {
    /// Message that will be serialized when sent
    Raw(ServerMessage),
    /// Pre-serialized message (shared across multiple sends via Arc)
    Serialized(Arc<str>),
}

impl OutboundMessage {
    /// Create a pre-serialized message from a ServerMessage
    pub fn preserialized(message: &ServerMessage) -> Result<Self, serde_json::Error> {
        let json = serde_json::to_string(message)?;
        Ok(Self::Serialized(Arc::from(json)))
    }

    /// Convert to JSON string
    pub fn to_json(&self) -> Result<String, serde_json::Error> {
        match self {
            Self::Raw(msg) => serde_json::to_string(msg),
            Self::Serialized(json) => Ok(json.to_string()),
        }
    }
}

impl From<ServerMessage> for OutboundMessage {
    fn from(msg: ServerMessage) -> Self {
        Self::Raw(msg)
    }
}
