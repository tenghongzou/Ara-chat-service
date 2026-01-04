//! Notification types for push notification integration
//!
//! Defines the event types and payload formats for sending notifications
//! to the Notification Service via Redis Pub/Sub.

use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// Chat notification event types
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ChatNotificationType {
    /// New message notification (for offline users)
    Message,
    /// @mention notification
    Mention,
    /// Emoji reaction notification
    Reaction,
}

impl ChatNotificationType {
    /// Get the event type string for the notification service
    pub fn event_type(&self) -> &'static str {
        match self {
            Self::Message => "chat.message",
            Self::Mention => "chat.mention",
            Self::Reaction => "chat.reaction",
        }
    }

    /// Get the default priority for this event type
    pub fn default_priority(&self) -> NotificationPriority {
        match self {
            Self::Message => NotificationPriority::Normal,
            Self::Mention => NotificationPriority::High,
            Self::Reaction => NotificationPriority::Normal,
        }
    }
}

/// Notification priority levels
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum NotificationPriority {
    Low,
    Normal,
    High,
    Critical,
}

impl NotificationPriority {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Low => "Low",
            Self::Normal => "Normal",
            Self::High => "High",
            Self::Critical => "Critical",
        }
    }
}

/// Payload for chat notifications
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NotificationPayload {
    /// The conversation where the event occurred
    pub conversation_id: Uuid,
    /// The message ID (if applicable)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub message_id: Option<Uuid>,
    /// The user who triggered the event (sender/reactor)
    pub sender_id: Uuid,
    /// Display name of the sender (optional)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sender_name: Option<String>,
    /// Preview of the message content (truncated to 100 chars)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub content_preview: Option<String>,
    /// Emoji for reaction events
    #[serde(skip_serializing_if = "Option::is_none")]
    pub emoji: Option<String>,
    /// Action type for reactions ("add" or "remove")
    #[serde(skip_serializing_if = "Option::is_none")]
    pub action: Option<String>,
}

impl NotificationPayload {
    /// Create a new message notification payload
    pub fn new_message(
        conversation_id: Uuid,
        message_id: Uuid,
        sender_id: Uuid,
        content_preview: Option<String>,
    ) -> Self {
        Self {
            conversation_id,
            message_id: Some(message_id),
            sender_id,
            sender_name: None,
            content_preview,
            emoji: None,
            action: None,
        }
    }

    /// Create a mention notification payload
    pub fn mention(
        conversation_id: Uuid,
        message_id: Uuid,
        sender_id: Uuid,
        sender_name: Option<String>,
        content_preview: Option<String>,
    ) -> Self {
        Self {
            conversation_id,
            message_id: Some(message_id),
            sender_id,
            sender_name,
            content_preview,
            emoji: None,
            action: None,
        }
    }

    /// Create a reaction notification payload
    pub fn reaction(
        conversation_id: Uuid,
        message_id: Uuid,
        reactor_id: Uuid,
        emoji: String,
        action: &str,
    ) -> Self {
        Self {
            conversation_id,
            message_id: Some(message_id),
            sender_id: reactor_id,
            sender_name: None,
            content_preview: None,
            emoji: Some(emoji),
            action: Some(action.to_string()),
        }
    }
}

/// Event payload structure matching Notification Service expectations
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EventPayload {
    /// Event type (e.g., "chat.message", "chat.mention", "chat.reaction")
    pub event_type: String,
    /// The actual payload data
    pub payload: serde_json::Value,
    /// Priority level
    pub priority: String,
    /// Time-to-live in seconds
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ttl: Option<u32>,
    /// Correlation ID for tracing
    #[serde(skip_serializing_if = "Option::is_none")]
    pub correlation_id: Option<String>,
}

/// Redis message format for the Notification Service
///
/// This matches the expected format of the Notification Service's Redis subscriber.
/// Channel: `notification:user:{user_id}`
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NotificationEvent {
    /// Target type - always "user" for point-to-point
    #[serde(rename = "type")]
    pub target_type: String,
    /// Target user ID
    pub target: String,
    /// The event payload
    pub event: EventPayload,
}

impl NotificationEvent {
    /// Create a new notification event for a user
    pub fn for_user(
        user_id: Uuid,
        notification_type: ChatNotificationType,
        payload: NotificationPayload,
        ttl: Option<u32>,
    ) -> Result<Self, serde_json::Error> {
        let payload_value = serde_json::to_value(&payload)?;

        Ok(Self {
            target_type: "user".to_string(),
            target: user_id.to_string(),
            event: EventPayload {
                event_type: notification_type.event_type().to_string(),
                payload: payload_value,
                priority: notification_type.default_priority().as_str().to_string(),
                ttl,
                correlation_id: None,
            },
        })
    }

    /// Serialize to JSON for Redis publish
    pub fn to_json(&self) -> Result<String, serde_json::Error> {
        serde_json::to_string(self)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_chat_notification_type_event_type() {
        assert_eq!(ChatNotificationType::Message.event_type(), "chat.message");
        assert_eq!(ChatNotificationType::Mention.event_type(), "chat.mention");
        assert_eq!(ChatNotificationType::Reaction.event_type(), "chat.reaction");
    }

    #[test]
    fn test_chat_notification_type_priority() {
        assert_eq!(
            ChatNotificationType::Message.default_priority(),
            NotificationPriority::Normal
        );
        assert_eq!(
            ChatNotificationType::Mention.default_priority(),
            NotificationPriority::High
        );
        assert_eq!(
            ChatNotificationType::Reaction.default_priority(),
            NotificationPriority::Normal
        );
    }

    #[test]
    fn test_notification_payload_new_message() {
        let conv_id = Uuid::new_v4();
        let msg_id = Uuid::new_v4();
        let sender_id = Uuid::new_v4();

        let payload = NotificationPayload::new_message(
            conv_id,
            msg_id,
            sender_id,
            Some("Hello world".to_string()),
        );

        assert_eq!(payload.conversation_id, conv_id);
        assert_eq!(payload.message_id, Some(msg_id));
        assert_eq!(payload.sender_id, sender_id);
        assert_eq!(payload.content_preview, Some("Hello world".to_string()));
        assert!(payload.emoji.is_none());
    }

    #[test]
    fn test_notification_payload_mention() {
        let conv_id = Uuid::new_v4();
        let msg_id = Uuid::new_v4();
        let sender_id = Uuid::new_v4();

        let payload = NotificationPayload::mention(
            conv_id,
            msg_id,
            sender_id,
            Some("Alice".to_string()),
            Some("Hey @Bob".to_string()),
        );

        assert_eq!(payload.sender_name, Some("Alice".to_string()));
        assert_eq!(payload.content_preview, Some("Hey @Bob".to_string()));
    }

    #[test]
    fn test_notification_payload_reaction() {
        let conv_id = Uuid::new_v4();
        let msg_id = Uuid::new_v4();
        let reactor_id = Uuid::new_v4();

        let payload = NotificationPayload::reaction(
            conv_id,
            msg_id,
            reactor_id,
            "👍".to_string(),
            "add",
        );

        assert_eq!(payload.emoji, Some("👍".to_string()));
        assert_eq!(payload.action, Some("add".to_string()));
    }

    #[test]
    fn test_notification_event_serialization() {
        let user_id = Uuid::new_v4();
        let conv_id = Uuid::new_v4();
        let msg_id = Uuid::new_v4();
        let sender_id = Uuid::new_v4();

        let payload = NotificationPayload::mention(
            conv_id,
            msg_id,
            sender_id,
            Some("Alice".to_string()),
            Some("Hey @Bob".to_string()),
        );

        let event = NotificationEvent::for_user(
            user_id,
            ChatNotificationType::Mention,
            payload,
            Some(3600),
        )
        .unwrap();

        let json = event.to_json().unwrap();
        assert!(json.contains("\"type\":\"user\""));
        assert!(json.contains("\"event_type\":\"chat.mention\""));
        assert!(json.contains("\"priority\":\"High\""));
        assert!(json.contains("\"ttl\":3600"));
    }

    #[test]
    fn test_notification_event_deserialization() {
        let json = r#"{
            "type": "user",
            "target": "550e8400-e29b-41d4-a716-446655440000",
            "event": {
                "event_type": "chat.message",
                "payload": {"conversation_id": "550e8400-e29b-41d4-a716-446655440001"},
                "priority": "Normal"
            }
        }"#;

        let event: NotificationEvent = serde_json::from_str(json).unwrap();
        assert_eq!(event.target_type, "user");
        assert_eq!(event.event.event_type, "chat.message");
        assert_eq!(event.event.priority, "Normal");
    }

    #[test]
    fn test_priority_as_str() {
        assert_eq!(NotificationPriority::Low.as_str(), "Low");
        assert_eq!(NotificationPriority::Normal.as_str(), "Normal");
        assert_eq!(NotificationPriority::High.as_str(), "High");
        assert_eq!(NotificationPriority::Critical.as_str(), "Critical");
    }
}
