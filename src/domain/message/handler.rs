//! Message handler - processes incoming chat messages

use std::sync::Arc;
use std::time::Duration;

use chrono::Utc;
use uuid::Uuid;

use super::types::{ChatMessage, ContentType};
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

        // Create and store message
        let message = self.storage.create_message(
            conversation_id,
            sender_id,
            content,
            content_type,
            reply_to,
            client_message_id,
            valid_mentions.clone(),
        ).await?;

        // Route message to all participants
        self.router.route_message(&message).await?;

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

    #[error("Storage error: {0}")]
    Storage(#[from] super::storage::StorageError),

    #[error("Routing error: {0}")]
    Routing(#[from] super::router::RouterError),

    #[error("Conversation error: {0}")]
    Conversation(#[from] crate::conversation::ConversationError),
}
