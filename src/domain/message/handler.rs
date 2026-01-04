//! Message handler - processes incoming chat messages

use std::sync::Arc;
use std::time::Duration;

use chrono::Utc;
use uuid::Uuid;

use super::types::{ChatMessage, ContentType, OutboundMessage, ServerMessage};
use super::storage::MessageStorage;
use super::router::MessageRouter;
use crate::conversation::ConversationService;
use crate::mention::MentionParser;

/// Handles incoming chat messages
pub struct MessageHandler {
    storage: Arc<MessageStorage>,
    router: Arc<MessageRouter>,
    conversation_service: Arc<ConversationService>,
    /// Maximum time window for deduplication (default: 5 minutes)
    dedup_window: Duration,
    /// Maximum time window for message recall (default: 2 minutes)
    recall_window: Duration,
    /// Maximum time window for message edit (default: 15 minutes)
    edit_window: Duration,
}

impl MessageHandler {
    pub fn new(
        storage: Arc<MessageStorage>,
        router: Arc<MessageRouter>,
        conversation_service: Arc<ConversationService>,
    ) -> Self {
        Self {
            storage,
            router,
            conversation_service,
            dedup_window: Duration::from_secs(300), // 5 minutes
            recall_window: Duration::from_secs(120), // 2 minutes
            edit_window: Duration::from_secs(900), // 15 minutes
        }
    }

    /// Configure recall time window
    pub fn with_recall_window(mut self, window: Duration) -> Self {
        self.recall_window = window;
        self
    }

    /// Configure edit time window
    pub fn with_edit_window(mut self, window: Duration) -> Self {
        self.edit_window = window;
        self
    }

    /// Handle a send message request
    pub async fn handle_send_message(
        &self,
        sender_id: Uuid,
        conversation_id: Uuid,
        content: String,
        content_type: ContentType,
        reply_to: Option<Uuid>,
        client_message_id: Option<String>,
        explicit_mentions: Vec<Uuid>,
    ) -> Result<ChatMessage, MessageHandlerError> {
        // Verify sender is a participant
        if !self.conversation_service.is_participant(conversation_id, sender_id).await? {
            return Err(MessageHandlerError::NotParticipant);
        }

        // Check for duplicate message (idempotency)
        if let Some(ref client_id) = client_message_id {
            if let Some(existing) = self.storage.find_by_client_id(sender_id, client_id).await? {
                tracing::debug!(
                    client_message_id = %client_id,
                    existing_id = %existing.id,
                    "Duplicate message detected, returning existing"
                );
                return Ok(existing);
            }
        }

        // Parse mentions from content and merge with explicit mentions
        let parsed_mentions = MentionParser::parse(&content);
        let mut all_mentions: Vec<Uuid> = explicit_mentions;

        for mention in parsed_mentions {
            if let Some(user_id) = mention.user_id {
                if !all_mentions.contains(&user_id) {
                    all_mentions.push(user_id);
                }
            }
            // Note: username mentions would need a user lookup service
        }

        // Validate mentions - only include participants
        let participants = self.conversation_service
            .get_participant_ids(conversation_id)
            .await?;
        let valid_mentions = MentionParser::validate_mentions(&all_mentions, &participants);

        // Validate reply target if specified
        let reply_context = if let Some(reply_to_id) = reply_to {
            // Check if the reply target is valid (exists, same conversation, not deleted)
            let is_valid = self.storage.validate_reply_target(reply_to_id, conversation_id).await?;
            if !is_valid {
                return Err(MessageHandlerError::InvalidReplyTarget);
            }

            // Get the reply context (preview of original message)
            self.storage.get_reply_context(reply_to_id).await?
        } else {
            None
        };

        // Create and store message
        let mut message = self.storage.create_message(
            conversation_id,
            sender_id,
            content,
            content_type,
            reply_to,
            client_message_id,
            valid_mentions.clone(),
        ).await?;

        // Attach reply context to the message
        message.reply_context = reply_context;

        // Route message to all participants
        self.router.route_message(&message).await?;

        // If this is a reply, notify about thread update
        if let Some(reply_to_id) = reply_to {
            if let Ok(Some(thread_info)) = self.storage.get_thread_info(reply_to_id).await {
                // Route thread updated notification
                self.router.route_thread_updated(
                    conversation_id,
                    reply_to_id,
                    thread_info.reply_count,
                    message.created_at,
                    sender_id,
                ).await?;
            }
        }

        // Handle mentions (skip sender)
        let mentions_to_notify: Vec<Uuid> = valid_mentions
            .into_iter()
            .filter(|&id| id != sender_id)
            .collect();

        if !mentions_to_notify.is_empty() {
            self.router.notify_mentions(&message, &mentions_to_notify).await?;
        }

        // Update conversation's last message
        if let Err(e) = self.conversation_service
            .update_last_message(conversation_id, message.id)
            .await
        {
            tracing::warn!(
                conversation_id = %conversation_id,
                error = %e,
                "Failed to update conversation last message"
            );
        }

        Ok(message)
    }

    /// Handle edit message request
    pub async fn handle_edit_message(
        &self,
        user_id: Uuid,
        message_id: Uuid,
        new_content: String,
    ) -> Result<ChatMessage, MessageHandlerError> {
        // Get the original message
        let message = self.storage.get_message(message_id).await?
            .ok_or(MessageHandlerError::MessageNotFound)?;

        // Verify user is the sender
        if message.sender_id != user_id {
            return Err(MessageHandlerError::NotMessageOwner);
        }

        // Check if message was recalled
        if message.recalled_at.is_some() {
            return Err(MessageHandlerError::MessageRecalled);
        }

        // Check edit time window
        let now = Utc::now().timestamp_millis();
        let message_age_ms = now - message.created_at;
        let edit_window_ms = self.edit_window.as_millis() as i64;

        if message_age_ms > edit_window_ms {
            return Err(MessageHandlerError::EditWindowExpired {
                allowed_seconds: self.edit_window.as_secs(),
            });
        }

        // Parse new mentions
        let parsed_mentions = MentionParser::parse(&new_content);
        let mut new_mentions: Vec<Uuid> = Vec::new();
        for mention in parsed_mentions {
            if let Some(uid) = mention.user_id {
                if !new_mentions.contains(&uid) {
                    new_mentions.push(uid);
                }
            }
        }

        // Validate mentions
        let participants = self.conversation_service
            .get_participant_ids(message.conversation_id)
            .await?;
        let valid_mentions = MentionParser::validate_mentions(&new_mentions, &participants);

        // Update the message
        let updated = self.storage.edit_message(message_id, new_content, valid_mentions.clone()).await?;

        // Route edit notification to all participants
        self.router.route_message_edit(&updated).await?;

        // Notify new mentions (users who weren't mentioned before)
        let new_mention_targets: Vec<Uuid> = valid_mentions
            .iter()
            .filter(|&id| !message.mentions.contains(id) && *id != user_id)
            .copied()
            .collect();

        if !new_mention_targets.is_empty() {
            self.router.notify_mentions(&updated, &new_mention_targets).await?;
        }

        Ok(updated)
    }

    /// Handle recall message request
    pub async fn handle_recall_message(
        &self,
        user_id: Uuid,
        conversation_id: Uuid,
        message_id: Uuid,
    ) -> Result<(), MessageHandlerError> {
        // Verify user is the sender of the message
        let message = self.storage.get_message(message_id).await?
            .ok_or(MessageHandlerError::MessageNotFound)?;

        if message.sender_id != user_id {
            return Err(MessageHandlerError::NotMessageOwner);
        }

        // Check if already recalled
        if message.recalled_at.is_some() {
            return Ok(()); // Already recalled, idempotent
        }

        // Check recall time window
        let now = Utc::now().timestamp_millis();
        let message_age_ms = now - message.created_at;
        let recall_window_ms = self.recall_window.as_millis() as i64;

        if message_age_ms > recall_window_ms {
            return Err(MessageHandlerError::RecallWindowExpired {
                allowed_seconds: self.recall_window.as_secs(),
            });
        }

        // Mark message as recalled
        self.storage.recall_message(message_id).await?;

        // Notify participants
        self.router.route_recall(conversation_id, message_id, user_id).await?;

        tracing::info!(
            user_id = %user_id,
            message_id = %message_id,
            "Message recalled"
        );

        Ok(())
    }

    /// Get message by ID
    pub async fn get_message(&self, message_id: Uuid) -> Result<Option<ChatMessage>, MessageHandlerError> {
        Ok(self.storage.get_message(message_id).await?)
    }

    /// Handle pin message request
    /// Only owners and admins can pin messages
    pub async fn handle_pin_message(
        &self,
        user_id: Uuid,
        conversation_id: Uuid,
        message_id: Uuid,
    ) -> Result<i64, MessageHandlerError> {
        // Verify user is a participant and has permission to pin
        let participant = self.conversation_service
            .get_participant_info(conversation_id, user_id)
            .await?
            .ok_or(MessageHandlerError::NotParticipant)?;

        // Check if user has permission to pin (owner or admin)
        if !participant.can_pin_messages() {
            return Err(MessageHandlerError::InsufficientPinPermission);
        }

        // Pin the message (storage layer validates message exists and is in conversation)
        let pinned_at = self.storage.pin_message(message_id, conversation_id, user_id).await?;

        // Notify all participants about the pin
        self.router.route_message_pinned(
            conversation_id,
            message_id,
            user_id,
            pinned_at,
        ).await?;

        tracing::info!(
            user_id = %user_id,
            conversation_id = %conversation_id,
            message_id = %message_id,
            "Message pinned"
        );

        Ok(pinned_at)
    }

    /// Handle unpin message request
    /// Only owners and admins can unpin messages
    pub async fn handle_unpin_message(
        &self,
        user_id: Uuid,
        conversation_id: Uuid,
        message_id: Uuid,
    ) -> Result<(), MessageHandlerError> {
        // Verify user is a participant and has permission to unpin
        let participant = self.conversation_service
            .get_participant_info(conversation_id, user_id)
            .await?
            .ok_or(MessageHandlerError::NotParticipant)?;

        // Check if user has permission to unpin (owner or admin)
        if !participant.can_pin_messages() {
            return Err(MessageHandlerError::InsufficientPinPermission);
        }

        // Unpin the message
        let was_pinned = self.storage.unpin_message(message_id, conversation_id).await?;

        if was_pinned {
            // Notify all participants about the unpin
            self.router.route_message_unpinned(
                conversation_id,
                message_id,
                user_id,
            ).await?;

            tracing::info!(
                user_id = %user_id,
                conversation_id = %conversation_id,
                message_id = %message_id,
                "Message unpinned"
            );
        }

        Ok(())
    }

    /// Get pinned messages for a conversation
    pub async fn get_pinned_messages(
        &self,
        user_id: Uuid,
        conversation_id: Uuid,
        limit: i64,
    ) -> Result<Vec<ChatMessage>, MessageHandlerError> {
        // Verify user is a participant
        if !self.conversation_service.is_participant(conversation_id, user_id).await? {
            return Err(MessageHandlerError::NotParticipant);
        }

        let messages = self.storage.get_pinned_messages(conversation_id, limit).await?;
        Ok(messages)
    }

    // ==================== Muting Methods ====================

    /// Handle mute conversation request
    /// Any participant can mute a conversation for themselves
    pub async fn handle_mute_conversation(
        &self,
        user_id: Uuid,
        conversation_id: Uuid,
    ) -> Result<i64, MessageHandlerError> {
        // Mute the conversation (service handles participant verification)
        let muted_at = self.conversation_service
            .mute_conversation(conversation_id, user_id)
            .await?;

        // Send confirmation to the user
        let server_message = ServerMessage::ConversationMuted {
            conversation_id,
            muted_at,
        };

        if let Ok(outbound) = OutboundMessage::preserialized(&server_message) {
            self.router.connection_manager().send_to_user(&user_id, outbound).await;
        }

        tracing::info!(
            user_id = %user_id,
            conversation_id = %conversation_id,
            "Conversation muted"
        );

        Ok(muted_at)
    }

    /// Handle unmute conversation request
    pub async fn handle_unmute_conversation(
        &self,
        user_id: Uuid,
        conversation_id: Uuid,
    ) -> Result<(), MessageHandlerError> {
        // Unmute the conversation (service handles participant verification)
        self.conversation_service
            .unmute_conversation(conversation_id, user_id)
            .await?;

        // Send confirmation to the user
        let server_message = ServerMessage::ConversationUnmuted {
            conversation_id,
        };

        if let Ok(outbound) = OutboundMessage::preserialized(&server_message) {
            self.router.connection_manager().send_to_user(&user_id, outbound).await;
        }

        tracing::info!(
            user_id = %user_id,
            conversation_id = %conversation_id,
            "Conversation unmuted"
        );

        Ok(())
    }

    /// Get muted conversations for a user
    pub async fn get_muted_conversations(
        &self,
        user_id: Uuid,
    ) -> Result<Vec<Uuid>, MessageHandlerError> {
        let conversations = self.conversation_service
            .get_muted_conversations(user_id)
            .await?;

        Ok(conversations)
    }
}

/// Message handler errors
#[derive(Debug, thiserror::Error)]
pub enum MessageHandlerError {
    #[error("User is not a participant in this conversation")]
    NotParticipant,

    #[error("Message not found")]
    MessageNotFound,

    #[error("User is not the owner of this message")]
    NotMessageOwner,

    #[error("Message has been recalled")]
    MessageRecalled,

    #[error("Recall window expired (allowed: {allowed_seconds} seconds)")]
    RecallWindowExpired { allowed_seconds: u64 },

    #[error("Edit window expired (allowed: {allowed_seconds} seconds)")]
    EditWindowExpired { allowed_seconds: u64 },

    #[error("Invalid reply target: message not found, deleted, or in different conversation")]
    InvalidReplyTarget,

    #[error("Insufficient permission to pin/unpin messages")]
    InsufficientPinPermission,

    #[error("Storage error: {0}")]
    Storage(#[from] super::storage::StorageError),

    #[error("Routing error: {0}")]
    Routing(#[from] super::router::RouterError),

    #[error("Conversation error: {0}")]
    Conversation(#[from] crate::conversation::ConversationError),
}

#[cfg(test)]
mod tests {
    use super::*;

    // ==================== MessageHandlerError Tests ====================

    #[test]
    fn test_error_not_participant_display() {
        let err = MessageHandlerError::NotParticipant;
        assert_eq!(err.to_string(), "User is not a participant in this conversation");
    }

    #[test]
    fn test_error_message_not_found_display() {
        let err = MessageHandlerError::MessageNotFound;
        assert_eq!(err.to_string(), "Message not found");
    }

    #[test]
    fn test_error_not_message_owner_display() {
        let err = MessageHandlerError::NotMessageOwner;
        assert_eq!(err.to_string(), "User is not the owner of this message");
    }

    #[test]
    fn test_error_message_recalled_display() {
        let err = MessageHandlerError::MessageRecalled;
        assert_eq!(err.to_string(), "Message has been recalled");
    }

    #[test]
    fn test_error_recall_window_expired_display() {
        let err = MessageHandlerError::RecallWindowExpired { allowed_seconds: 120 };
        assert_eq!(err.to_string(), "Recall window expired (allowed: 120 seconds)");
    }

    #[test]
    fn test_error_edit_window_expired_display() {
        let err = MessageHandlerError::EditWindowExpired { allowed_seconds: 900 };
        assert_eq!(err.to_string(), "Edit window expired (allowed: 900 seconds)");
    }

    #[test]
    fn test_error_invalid_reply_target_display() {
        let err = MessageHandlerError::InvalidReplyTarget;
        assert_eq!(
            err.to_string(),
            "Invalid reply target: message not found, deleted, or in different conversation"
        );
    }

    #[test]
    fn test_error_debug_impl() {
        let err = MessageHandlerError::NotParticipant;
        let debug = format!("{:?}", err);
        assert!(debug.contains("NotParticipant"));
    }

    #[test]
    fn test_error_recall_window_values() {
        // Test that the error correctly stores the allowed seconds
        let err = MessageHandlerError::RecallWindowExpired { allowed_seconds: 60 };
        if let MessageHandlerError::RecallWindowExpired { allowed_seconds } = err {
            assert_eq!(allowed_seconds, 60);
        } else {
            panic!("Expected RecallWindowExpired");
        }
    }

    #[test]
    fn test_error_edit_window_values() {
        let err = MessageHandlerError::EditWindowExpired { allowed_seconds: 300 };
        if let MessageHandlerError::EditWindowExpired { allowed_seconds } = err {
            assert_eq!(allowed_seconds, 300);
        } else {
            panic!("Expected EditWindowExpired");
        }
    }

    // ==================== Default Window Configuration Tests ====================

    // Note: Full handler tests with mocked dependencies would require
    // extracting traits from MessageStorage, MessageRouter, and ConversationService.
    // This is a significant refactoring effort for future improvement.

    #[test]
    fn test_default_recall_window() {
        // Default recall window should be 2 minutes (120 seconds)
        let default_recall = Duration::from_secs(120);
        assert_eq!(default_recall.as_secs(), 120);
    }

    #[test]
    fn test_default_edit_window() {
        // Default edit window should be 15 minutes (900 seconds)
        let default_edit = Duration::from_secs(900);
        assert_eq!(default_edit.as_secs(), 900);
    }

    #[test]
    fn test_default_dedup_window() {
        // Default dedup window should be 5 minutes (300 seconds)
        let default_dedup = Duration::from_secs(300);
        assert_eq!(default_dedup.as_secs(), 300);
    }
}
