//! Core message types for the chat protocol

use std::collections::HashMap;
use std::sync::Arc;

use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::markdown::RenderingHints;

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

    /// Pin a message in a conversation
    PinMessage {
        conversation_id: Uuid,
        message_id: Uuid,
    },

    /// Unpin a message in a conversation
    UnpinMessage {
        conversation_id: Uuid,
        message_id: Uuid,
    },

    /// Mute a conversation (no push notifications)
    MuteConversation {
        conversation_id: Uuid,
    },

    /// Unmute a conversation
    UnmuteConversation {
        conversation_id: Uuid,
    },

    /// Block a user
    BlockUser {
        user_id: Uuid,
    },

    /// Unblock a user
    UnblockUser {
        user_id: Uuid,
    },

    /// Get list of blocked users
    GetBlockedUsers,

    /// Forward a message to one or more conversations
    ForwardMessage {
        /// ID of the message to forward
        message_id: Uuid,
        /// Source conversation ID (for validation)
        source_conversation_id: Uuid,
        /// Target conversation IDs to forward to (max 10)
        target_conversation_ids: Vec<Uuid>,
    },

    /// Announce client capabilities (compression support, etc.)
    Capabilities {
        /// Supported compression algorithms (e.g., ["zstd"])
        #[serde(default)]
        compression: Vec<String>,
        /// Maximum message size client can handle
        #[serde(skip_serializing_if = "Option::is_none")]
        max_message_size: Option<usize>,
    },

    /// Subscribe to conversation updates (messages, typing, etc.)
    ///
    /// Switches the connection to explicit subscription mode.
    /// Only subscribed conversations will receive messages.
    SubscribeConversations {
        /// Conversation IDs to subscribe to
        conversation_ids: Vec<Uuid>,
    },

    /// Unsubscribe from conversation updates
    UnsubscribeConversations {
        /// Conversation IDs to unsubscribe from
        conversation_ids: Vec<Uuid>,
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

    /// Thread info updated (new reply added to a message)
    #[serde(rename = "thread_updated")]
    ThreadUpdated {
        conversation_id: Uuid,
        message_id: Uuid,
        reply_count: i32,
        last_reply_at: i64,
        last_reply_sender_id: Uuid,
    },

    /// Message was pinned
    #[serde(rename = "message_pinned")]
    MessagePinned {
        conversation_id: Uuid,
        message_id: Uuid,
        pinned_by: Uuid,
        pinned_at: i64,
    },

    /// Message was unpinned
    #[serde(rename = "message_unpinned")]
    MessageUnpinned {
        conversation_id: Uuid,
        message_id: Uuid,
        unpinned_by: Uuid,
    },

    /// Conversation was muted by the user
    #[serde(rename = "conversation_muted")]
    ConversationMuted {
        conversation_id: Uuid,
        muted_at: i64,
    },

    /// Conversation was unmuted by the user
    #[serde(rename = "conversation_unmuted")]
    ConversationUnmuted {
        conversation_id: Uuid,
    },

    /// User was blocked
    #[serde(rename = "user_blocked")]
    UserBlocked {
        user_id: Uuid,
        blocked_at: i64,
    },

    /// User was unblocked
    #[serde(rename = "user_unblocked")]
    UserUnblocked {
        user_id: Uuid,
    },

    /// List of blocked users
    #[serde(rename = "blocked_users")]
    BlockedUsers {
        users: Vec<crate::blocking::BlockedUserInfo>,
    },

    /// Message was forwarded successfully
    #[serde(rename = "message_forwarded")]
    MessageForwarded {
        source_message_id: Uuid,
        results: Vec<ForwardResult>,
    },

    /// Link previews are ready for a message
    #[serde(rename = "link_preview_ready")]
    LinkPreviewReady {
        message_id: Uuid,
        conversation_id: Uuid,
        previews: Vec<crate::link_preview::LinkPreview>,
    },

    /// Server acknowledgment of client capabilities
    #[serde(rename = "capabilities_ack")]
    CapabilitiesAck {
        /// Selected compression algorithm (null if compression disabled)
        #[serde(skip_serializing_if = "Option::is_none")]
        compression: Option<String>,
        /// Compression threshold in bytes
        threshold: usize,
    },

    /// Subscription status update confirmation
    #[serde(rename = "subscription_updated")]
    SubscriptionUpdated {
        /// Conversations that were newly subscribed
        subscribed: Vec<Uuid>,
        /// Conversations that were unsubscribed
        unsubscribed: Vec<Uuid>,
        /// Total active subscriptions for this connection
        total_subscriptions: usize,
    },

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

    pub fn capabilities_ack(compression: Option<String>, threshold: usize) -> Self {
        Self::CapabilitiesAck {
            compression,
            threshold,
        }
    }

    pub fn subscription_updated(
        subscribed: Vec<Uuid>,
        unsubscribed: Vec<Uuid>,
        total_subscriptions: usize,
    ) -> Self {
        Self::SubscriptionUpdated {
            subscribed,
            unsubscribed,
            total_subscriptions,
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
    /// Preview of the message being replied to
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reply_context: Option<ReplyContext>,
    /// Thread metadata (reply count, last reply info)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub thread_info: Option<ThreadInfo>,
    #[serde(default)]
    pub mentions: Vec<Uuid>,
    #[serde(default)]
    pub reactions: HashMap<String, Vec<Uuid>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub recalled_at: Option<i64>,
    /// Timestamp when message was pinned (milliseconds since epoch)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub pinned_at: Option<i64>,
    /// User who pinned the message
    #[serde(skip_serializing_if = "Option::is_none")]
    pub pinned_by: Option<Uuid>,
    /// Forwarding metadata - present if this message was forwarded
    #[serde(skip_serializing_if = "Option::is_none")]
    pub forwarded_from: Option<ForwardedFrom>,
    /// Markdown rendering hints for client-side formatting
    #[serde(skip_serializing_if = "Option::is_none")]
    pub rendering_hints: Option<RenderingHints>,
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
    /// Whether the current user has muted this conversation
    #[serde(default)]
    pub is_muted: bool,
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

/// Thread metadata for a message (reply count, last reply info)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ThreadInfo {
    /// Number of replies to this message
    pub reply_count: i32,
    /// Timestamp of the last reply (milliseconds)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub last_reply_at: Option<i64>,
    /// Sender ID of the last reply
    #[serde(skip_serializing_if = "Option::is_none")]
    pub last_reply_sender_id: Option<Uuid>,
}

/// Reply context - preview of the original message being replied to
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReplyContext {
    /// ID of the original message
    pub id: Uuid,
    /// Sender of the original message
    pub sender_id: Uuid,
    /// Truncated content preview (max 100 chars)
    pub content_preview: String,
    /// Content type of the original message
    pub content_type: ContentType,
}

impl ReplyContext {
    /// Maximum length for content preview
    pub const MAX_PREVIEW_LENGTH: usize = 100;

    /// Create a new reply context with truncated content
    pub fn new(id: Uuid, sender_id: Uuid, content: &str, content_type: ContentType) -> Self {
        let content_preview = if content.len() > Self::MAX_PREVIEW_LENGTH {
            let mut preview = content.chars().take(Self::MAX_PREVIEW_LENGTH).collect::<String>();
            preview.push_str("...");
            preview
        } else {
            content.to_string()
        };

        Self {
            id,
            sender_id,
            content_preview,
            content_type,
        }
    }
}

/// Forwarding metadata for a forwarded message
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ForwardedFrom {
    /// ID of the original message
    pub message_id: Uuid,
    /// Original sender ID
    pub sender_id: Uuid,
    /// Original conversation ID
    pub conversation_id: Uuid,
}

/// Result of forwarding a message to a single conversation
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ForwardResult {
    /// Target conversation ID
    pub conversation_id: Uuid,
    /// Whether the forward was successful
    pub success: bool,
    /// New message ID if successful
    #[serde(skip_serializing_if = "Option::is_none")]
    pub message_id: Option<Uuid>,
    /// Error message if failed
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
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

#[cfg(test)]
mod tests {
    use super::*;

    // ==================== ServerMessage Serialization Tests ====================

    #[test]
    fn test_server_message_authenticated_serialization() {
        let user_id = Uuid::parse_str("550e8400-e29b-41d4-a716-446655440000").unwrap();
        let msg = ServerMessage::authenticated(user_id);
        let json = serde_json::to_string(&msg).unwrap();

        assert!(json.contains("\"type\":\"authenticated\""));
        assert!(json.contains("\"user_id\":\"550e8400-e29b-41d4-a716-446655440000\""));
    }

    #[test]
    fn test_server_message_error_serialization() {
        let msg = ServerMessage::error("INVALID_TOKEN", "Token has expired");
        let json = serde_json::to_string(&msg).unwrap();

        assert!(json.contains("\"type\":\"error\""));
        assert!(json.contains("INVALID_TOKEN"));
        assert!(json.contains("Token has expired"));
    }

    #[test]
    fn test_server_message_shutdown_serialization() {
        let msg = ServerMessage::shutdown("Server maintenance", Some(30));
        let json = serde_json::to_string(&msg).unwrap();

        assert!(json.contains("\"type\":\"shutdown\""));
        assert!(json.contains("Server maintenance"));
        assert!(json.contains("30"));
    }

    #[test]
    fn test_server_message_pong_serialization() {
        let msg = ServerMessage::Pong;
        let json = serde_json::to_string(&msg).unwrap();
        assert!(json.contains("\"type\":\"pong\""));
    }

    // ==================== ClientMessage Deserialization Tests ====================

    #[test]
    fn test_client_message_ping_deserialization() {
        let json = r#"{"type":"Ping"}"#;
        let msg: ClientMessage = serde_json::from_str(json).unwrap();
        assert!(matches!(msg, ClientMessage::Ping));
    }

    #[test]
    fn test_client_message_authenticate_deserialization() {
        let json = r#"{"type":"Authenticate","payload":{"token":"test-jwt-token"}}"#;
        let msg: ClientMessage = serde_json::from_str(json).unwrap();

        if let ClientMessage::Authenticate { token } = msg {
            assert_eq!(token, "test-jwt-token");
        } else {
            panic!("Expected Authenticate message");
        }
    }

    #[test]
    fn test_client_message_send_message_deserialization() {
        let json = r#"{
            "type": "SendMessage",
            "payload": {
                "conversation_id": "550e8400-e29b-41d4-a716-446655440000",
                "content": "Hello world!",
                "content_type": "text"
            }
        }"#;
        let msg: ClientMessage = serde_json::from_str(json).unwrap();

        if let ClientMessage::SendMessage {
            conversation_id,
            content,
            content_type,
            mentions,
            ..
        } = msg
        {
            assert_eq!(
                conversation_id.to_string(),
                "550e8400-e29b-41d4-a716-446655440000"
            );
            assert_eq!(content, "Hello world!");
            assert_eq!(content_type, ContentType::Text);
            assert!(mentions.is_empty()); // Default empty vec
        } else {
            panic!("Expected SendMessage");
        }
    }

    #[test]
    fn test_client_message_typing_deserialization() {
        let json = r#"{
            "type": "Typing",
            "payload": {
                "conversation_id": "550e8400-e29b-41d4-a716-446655440000",
                "is_typing": true
            }
        }"#;
        let msg: ClientMessage = serde_json::from_str(json).unwrap();

        if let ClientMessage::Typing {
            conversation_id,
            is_typing,
        } = msg
        {
            assert_eq!(
                conversation_id.to_string(),
                "550e8400-e29b-41d4-a716-446655440000"
            );
            assert!(is_typing);
        } else {
            panic!("Expected Typing");
        }
    }

    // ==================== OutboundMessage Tests ====================

    #[test]
    fn test_outbound_message_from_server_message() {
        let msg = ServerMessage::Pong;
        let outbound: OutboundMessage = msg.into();

        assert!(matches!(outbound, OutboundMessage::Raw(_)));
    }

    #[test]
    fn test_outbound_message_preserialized() {
        let msg = ServerMessage::error("TEST", "Test message");
        let outbound = OutboundMessage::preserialized(&msg).unwrap();

        assert!(matches!(outbound, OutboundMessage::Serialized(_)));

        let json = outbound.to_json().unwrap();
        assert!(json.contains("TEST"));
        assert!(json.contains("Test message"));
    }

    #[test]
    fn test_outbound_message_raw_to_json() {
        let msg = ServerMessage::Pong;
        let outbound = OutboundMessage::Raw(msg);
        let json = outbound.to_json().unwrap();
        assert!(json.contains("pong"));
    }

    #[test]
    fn test_outbound_message_serialized_arc_sharing() {
        let msg = ServerMessage::error("SHARED", "Shared message");
        let outbound = OutboundMessage::preserialized(&msg).unwrap();

        // Clone should share the Arc
        let outbound2 = outbound.clone();

        let json1 = outbound.to_json().unwrap();
        let json2 = outbound2.to_json().unwrap();

        assert_eq!(json1, json2);
    }

    // ==================== Enum Default Tests ====================

    #[test]
    fn test_content_type_default() {
        let default = ContentType::default();
        assert_eq!(default, ContentType::Text);
    }

    #[test]
    fn test_presence_status_default() {
        let default = PresenceStatus::default();
        assert_eq!(default, PresenceStatus::Offline);
    }

    #[test]
    fn test_participant_role_default() {
        let default = ParticipantRole::default();
        assert_eq!(default, ParticipantRole::Member);
    }

    // ==================== Enum Serialization Tests ====================

    #[test]
    fn test_content_type_serialization() {
        assert_eq!(
            serde_json::to_string(&ContentType::Text).unwrap(),
            "\"text\""
        );
        assert_eq!(
            serde_json::to_string(&ContentType::Image).unwrap(),
            "\"image\""
        );
        assert_eq!(
            serde_json::to_string(&ContentType::File).unwrap(),
            "\"file\""
        );
        assert_eq!(
            serde_json::to_string(&ContentType::System).unwrap(),
            "\"system\""
        );
    }

    #[test]
    fn test_presence_status_serialization() {
        assert_eq!(
            serde_json::to_string(&PresenceStatus::Online).unwrap(),
            "\"online\""
        );
        assert_eq!(
            serde_json::to_string(&PresenceStatus::Away).unwrap(),
            "\"away\""
        );
        assert_eq!(
            serde_json::to_string(&PresenceStatus::Busy).unwrap(),
            "\"busy\""
        );
        assert_eq!(
            serde_json::to_string(&PresenceStatus::Offline).unwrap(),
            "\"offline\""
        );
    }

    #[test]
    fn test_conversation_type_serialization() {
        assert_eq!(
            serde_json::to_string(&ConversationType::Direct).unwrap(),
            "\"direct\""
        );
        assert_eq!(
            serde_json::to_string(&ConversationType::Group).unwrap(),
            "\"group\""
        );
    }

    #[test]
    fn test_reaction_action_serialization() {
        assert_eq!(
            serde_json::to_string(&ReactionAction::Add).unwrap(),
            "\"add\""
        );
        assert_eq!(
            serde_json::to_string(&ReactionAction::Remove).unwrap(),
            "\"remove\""
        );
    }

    // ==================== ChatMessage Tests ====================

    #[test]
    fn test_chat_message_serialization() {
        let msg = ChatMessage {
            id: Uuid::new_v4(),
            conversation_id: Uuid::new_v4(),
            sender_id: Uuid::new_v4(),
            content: "Hello!".to_string(),
            content_type: ContentType::Text,
            created_at: 1704067200,
            updated_at: None,
            reply_to_id: None,
            reply_context: None,
            thread_info: None,
            mentions: vec![],
            reactions: HashMap::new(),
            recalled_at: None,
            pinned_at: None,
            pinned_by: None,
            forwarded_from: None,
            rendering_hints: None,
        };

        let json = serde_json::to_string(&msg).unwrap();
        assert!(json.contains("Hello!"));
        assert!(json.contains("\"content_type\":\"text\""));
        // Optional fields with None should not appear
        assert!(!json.contains("updated_at"));
        assert!(!json.contains("reply_to_id"));
        assert!(!json.contains("recalled_at"));
        assert!(!json.contains("reply_context"));
        assert!(!json.contains("thread_info"));
        assert!(!json.contains("pinned_at"));
        assert!(!json.contains("pinned_by"));
        assert!(!json.contains("forwarded_from"));
        assert!(!json.contains("rendering_hints"));
    }

    #[test]
    fn test_chat_message_with_reactions() {
        let user_id = Uuid::new_v4();
        let mut reactions = HashMap::new();
        reactions.insert("👍".to_string(), vec![user_id]);

        let msg = ChatMessage {
            id: Uuid::new_v4(),
            conversation_id: Uuid::new_v4(),
            sender_id: Uuid::new_v4(),
            content: "Hello!".to_string(),
            content_type: ContentType::Text,
            created_at: 1704067200,
            updated_at: None,
            reply_to_id: None,
            reply_context: None,
            thread_info: None,
            mentions: vec![],
            reactions,
            recalled_at: None,
            pinned_at: None,
            pinned_by: None,
            forwarded_from: None,
            rendering_hints: None,
        };

        let json = serde_json::to_string(&msg).unwrap();
        assert!(json.contains("👍"));
    }

    // ==================== ThreadInfo and ReplyContext Tests ====================

    #[test]
    fn test_thread_info_serialization() {
        let info = ThreadInfo {
            reply_count: 5,
            last_reply_at: Some(1704067200000),
            last_reply_sender_id: Some(Uuid::new_v4()),
        };

        let json = serde_json::to_string(&info).unwrap();
        assert!(json.contains("\"reply_count\":5"));
        assert!(json.contains("last_reply_at"));
    }

    #[test]
    fn test_thread_info_without_replies() {
        let info = ThreadInfo {
            reply_count: 0,
            last_reply_at: None,
            last_reply_sender_id: None,
        };

        let json = serde_json::to_string(&info).unwrap();
        assert!(json.contains("\"reply_count\":0"));
        assert!(!json.contains("last_reply_at"));
    }

    #[test]
    fn test_reply_context_truncation() {
        let long_content = "a".repeat(200);
        let context = ReplyContext::new(
            Uuid::new_v4(),
            Uuid::new_v4(),
            &long_content,
            ContentType::Text,
        );

        assert!(context.content_preview.len() <= ReplyContext::MAX_PREVIEW_LENGTH + 3); // +3 for "..."
        assert!(context.content_preview.ends_with("..."));
    }

    #[test]
    fn test_reply_context_no_truncation() {
        let short_content = "Hello world";
        let context = ReplyContext::new(
            Uuid::new_v4(),
            Uuid::new_v4(),
            short_content,
            ContentType::Text,
        );

        assert_eq!(context.content_preview, short_content);
        assert!(!context.content_preview.ends_with("..."));
    }

    #[test]
    fn test_thread_updated_server_message() {
        let msg = ServerMessage::ThreadUpdated {
            conversation_id: Uuid::new_v4(),
            message_id: Uuid::new_v4(),
            reply_count: 3,
            last_reply_at: 1704067200000,
            last_reply_sender_id: Uuid::new_v4(),
        };

        let json = serde_json::to_string(&msg).unwrap();
        assert!(json.contains("\"type\":\"thread_updated\""));
        assert!(json.contains("\"reply_count\":3"));
    }

    // ==================== UnreadSyncData Tests ====================

    #[test]
    fn test_unread_sync_data_serialization() {
        let conv_id = Uuid::new_v4();
        let mut per_conversation = HashMap::new();
        per_conversation.insert(conv_id, 5u64);

        let data = UnreadSyncData {
            total: 10,
            per_conversation,
        };

        let json = serde_json::to_string(&data).unwrap();
        assert!(json.contains("\"total\":10"));
        assert!(json.contains(&conv_id.to_string()));
    }
}
