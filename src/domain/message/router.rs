//! Message router - routes messages to recipients across the cluster

use std::sync::Arc;

use uuid::Uuid;

use super::types::{ChatMessage, OutboundMessage, ServerMessage};
use crate::blocking::BlockingService;
use crate::cluster::ClusterRouter;
use crate::connection::ConnectionManager;
use crate::conversation::ConversationService;
use crate::notification::{NotificationPayload, NotificationPublisher};

/// Routes messages to conversation participants
pub struct MessageRouter {
    connection_manager: Arc<ConnectionManager>,
    cluster_router: Option<Arc<ClusterRouter>>,
    conversation_service: Arc<ConversationService>,
    notification_publisher: Option<Arc<NotificationPublisher>>,
    blocking_service: Option<Arc<BlockingService>>,
}

impl MessageRouter {
    pub fn new(
        connection_manager: Arc<ConnectionManager>,
        cluster_router: Option<Arc<ClusterRouter>>,
        conversation_service: Arc<ConversationService>,
    ) -> Self {
        Self {
            connection_manager,
            cluster_router,
            conversation_service,
            notification_publisher: None,
            blocking_service: None,
        }
    }

    /// Set notification publisher for push notifications to offline users
    pub fn with_notification_publisher(mut self, publisher: Arc<NotificationPublisher>) -> Self {
        self.notification_publisher = Some(publisher);
        self
    }

    /// Set blocking service for filtering blocked users
    pub fn with_blocking_service(mut self, service: Arc<BlockingService>) -> Self {
        self.blocking_service = Some(service);
        self
    }

    /// Get the connection manager reference
    pub fn connection_manager(&self) -> &Arc<ConnectionManager> {
        &self.connection_manager
    }

    /// Check if there is a blocking relationship between two users
    /// Returns true if either user has blocked the other
    pub async fn is_blocked(&self, user_id: Uuid, other_user_id: Uuid) -> bool {
        if let Some(ref blocking_service) = self.blocking_service {
            blocking_service
                .is_mutually_blocked(user_id, other_user_id)
                .await
                .unwrap_or(false)
        } else {
            false
        }
    }

    /// Route a message to all participants in the conversation
    pub async fn route_message(&self, message: &ChatMessage) -> Result<(), RouterError> {
        // Get all participants in the conversation
        let participants = self.conversation_service
            .get_participant_ids(message.conversation_id)
            .await
            .map_err(|e| RouterError::ConversationError(e.to_string()))?;

        // Get muted user IDs for filtering push notifications
        // Muted users still receive WebSocket messages, just no push notifications
        let muted_users = self.conversation_service
            .get_muted_user_ids(message.conversation_id)
            .await
            .unwrap_or_default();

        // Get blocked user IDs for the sender (bidirectional - includes users
        // blocked by sender AND users who blocked the sender)
        let blocked_users = if let Some(ref blocking_service) = self.blocking_service {
            blocking_service
                .get_all_blocked_user_ids(message.sender_id)
                .await
                .unwrap_or_default()
        } else {
            Vec::new()
        };

        // Pre-serialize the message for efficient multi-send
        let server_message = ServerMessage::Message {
            message: message.clone(),
        };
        let outbound = OutboundMessage::preserialized(&server_message)
            .map_err(|e| RouterError::Serialization(e.to_string()))?;

        // Track offline users for push notifications (excluding muted users)
        let mut offline_users: Vec<Uuid> = Vec::new();

        // Send to each participant
        for user_id in participants {
            // Skip sender - they already have confirmation
            if user_id == message.sender_id {
                continue;
            }

            // Skip blocked users - neither blocked nor blocker should see messages
            if blocked_users.contains(&user_id) {
                continue;
            }

            // Try local delivery first - muted users still receive WebSocket messages
            if self.connection_manager.has_user(&user_id) {
                self.connection_manager.send_to_user(&user_id, outbound.clone()).await;
            } else if let Some(ref cluster_router) = self.cluster_router {
                // Route through cluster with offline queue support
                cluster_router.route_to_user_with_queue(
                    user_id,
                    outbound.clone(),
                    server_message.clone(),
                ).await.map_err(|e| RouterError::ClusterError(e.to_string()))?;
            } else {
                // User is offline - only add to push notification list if NOT muted
                if !muted_users.contains(&user_id) {
                    offline_users.push(user_id);
                }
            }
        }

        // Send push notifications to offline users (muted users already filtered out)
        if !offline_users.is_empty() {
            if let Some(ref publisher) = self.notification_publisher {
                let content_preview: String = message.content.chars().take(100).collect();
                let payload = NotificationPayload::new_message(
                    message.conversation_id,
                    message.id,
                    message.sender_id,
                    Some(content_preview),
                );

                for user_id in offline_users {
                    publisher.notify_new_message(user_id, payload.clone()).await;
                }
            }
        }

        Ok(())
    }

    /// Route a message recall notification
    pub async fn route_recall(
        &self,
        conversation_id: Uuid,
        message_id: Uuid,
        recalled_by: Uuid,
    ) -> Result<(), RouterError> {
        let participants = self.conversation_service
            .get_participant_ids(conversation_id)
            .await
            .map_err(|e| RouterError::ConversationError(e.to_string()))?;

        let server_message = ServerMessage::MessageRecalled {
            conversation_id,
            message_id,
            recalled_by,
        };
        let outbound = OutboundMessage::preserialized(&server_message)
            .map_err(|e| RouterError::Serialization(e.to_string()))?;

        for user_id in participants {
            if self.connection_manager.has_user(&user_id) {
                self.connection_manager.send_to_user(&user_id, outbound.clone()).await;
            } else if let Some(ref cluster_router) = self.cluster_router {
                cluster_router.route_to_user_with_queue(
                    user_id,
                    outbound.clone(),
                    server_message.clone(),
                ).await.map_err(|e| RouterError::ClusterError(e.to_string()))?;
            }
        }

        Ok(())
    }

    /// Notify mentioned users
    pub async fn notify_mentions(
        &self,
        message: &ChatMessage,
        mentions: &[Uuid],
    ) -> Result<(), RouterError> {
        let server_message = ServerMessage::Mention {
            conversation_id: message.conversation_id,
            message_id: message.id,
            sender_id: message.sender_id,
            sender_name: String::new(), // TODO: Get sender name from user service
            preview: message.content.chars().take(100).collect(),
        };
        let outbound = OutboundMessage::preserialized(&server_message)
            .map_err(|e| RouterError::Serialization(e.to_string()))?;

        for &user_id in mentions {
            if self.connection_manager.has_user(&user_id) {
                self.connection_manager.send_to_user(&user_id, outbound.clone()).await;
            } else if let Some(ref cluster_router) = self.cluster_router {
                cluster_router.route_to_user_with_queue(
                    user_id,
                    outbound.clone(),
                    server_message.clone(),
                ).await.map_err(|e| RouterError::ClusterError(e.to_string()))?;
            }
        }

        Ok(())
    }

    /// Route a message edit notification to all participants
    pub async fn route_message_edit(&self, message: &ChatMessage) -> Result<(), RouterError> {
        let participants = self.conversation_service
            .get_participant_ids(message.conversation_id)
            .await
            .map_err(|e| RouterError::ConversationError(e.to_string()))?;

        let server_message = ServerMessage::MessageEdited {
            conversation_id: message.conversation_id,
            message_id: message.id,
            new_content: message.content.clone(),
            edited_at: message.updated_at.unwrap_or(message.created_at),
            mentions: message.mentions.clone(),
        };
        let outbound = OutboundMessage::preserialized(&server_message)
            .map_err(|e| RouterError::Serialization(e.to_string()))?;

        for user_id in participants {
            if self.connection_manager.has_user(&user_id) {
                self.connection_manager.send_to_user(&user_id, outbound.clone()).await;
            } else if let Some(ref cluster_router) = self.cluster_router {
                cluster_router.route_to_user_with_queue(
                    user_id,
                    outbound.clone(),
                    server_message.clone(),
                ).await.map_err(|e| RouterError::ClusterError(e.to_string()))?;
            }
        }

        Ok(())
    }

    /// Route typing indicator to conversation participants
    pub async fn route_typing(
        &self,
        conversation_id: Uuid,
        user_id: Uuid,
        is_typing: bool,
    ) -> Result<(), RouterError> {
        let participants = self.conversation_service
            .get_participant_ids(conversation_id)
            .await
            .map_err(|e| RouterError::ConversationError(e.to_string()))?;

        let server_message = ServerMessage::Typing {
            conversation_id,
            user_id,
            is_typing,
        };
        let outbound = OutboundMessage::preserialized(&server_message)
            .map_err(|e| RouterError::Serialization(e.to_string()))?;

        for participant_id in participants {
            // Don't send to the typing user
            if participant_id == user_id {
                continue;
            }

            if self.connection_manager.has_user(&participant_id) {
                self.connection_manager.send_to_user(&participant_id, outbound.clone()).await;
            } else if let Some(ref cluster_router) = self.cluster_router {
                cluster_router.route_to_user(participant_id, outbound.clone()).await
                    .map_err(|e| RouterError::ClusterError(e.to_string()))?;
            }
        }

        Ok(())
    }

    /// Route thread updated notification to conversation participants
    /// Called when a new reply is added to a message thread
    pub async fn route_thread_updated(
        &self,
        conversation_id: Uuid,
        message_id: Uuid,
        reply_count: i32,
        last_reply_at: i64,
        last_reply_sender_id: Uuid,
    ) -> Result<(), RouterError> {
        let participants = self.conversation_service
            .get_participant_ids(conversation_id)
            .await
            .map_err(|e| RouterError::ConversationError(e.to_string()))?;

        let server_message = ServerMessage::ThreadUpdated {
            conversation_id,
            message_id,
            reply_count,
            last_reply_at,
            last_reply_sender_id,
        };
        let outbound = OutboundMessage::preserialized(&server_message)
            .map_err(|e| RouterError::Serialization(e.to_string()))?;

        for participant_id in participants {
            if self.connection_manager.has_user(&participant_id) {
                self.connection_manager.send_to_user(&participant_id, outbound.clone()).await;
            } else if let Some(ref cluster_router) = self.cluster_router {
                // Thread updates are transient, no need to queue for offline users
                cluster_router.route_to_user(participant_id, outbound.clone()).await
                    .map_err(|e| RouterError::ClusterError(e.to_string()))?;
            }
        }

        Ok(())
    }

    /// Route message pinned notification to all participants
    pub async fn route_message_pinned(
        &self,
        conversation_id: Uuid,
        message_id: Uuid,
        pinned_by: Uuid,
        pinned_at: i64,
    ) -> Result<(), RouterError> {
        let participants = self.conversation_service
            .get_participant_ids(conversation_id)
            .await
            .map_err(|e| RouterError::ConversationError(e.to_string()))?;

        let server_message = ServerMessage::MessagePinned {
            conversation_id,
            message_id,
            pinned_by,
            pinned_at,
        };
        let outbound = OutboundMessage::preserialized(&server_message)
            .map_err(|e| RouterError::Serialization(e.to_string()))?;

        for participant_id in participants {
            if self.connection_manager.has_user(&participant_id) {
                self.connection_manager.send_to_user(&participant_id, outbound.clone()).await;
            } else if let Some(ref cluster_router) = self.cluster_router {
                // Pin notifications should be queued for offline users
                cluster_router.route_to_user_with_queue(
                    participant_id,
                    outbound.clone(),
                    server_message.clone(),
                ).await.map_err(|e| RouterError::ClusterError(e.to_string()))?;
            }
        }

        Ok(())
    }

    /// Route message unpinned notification to all participants
    pub async fn route_message_unpinned(
        &self,
        conversation_id: Uuid,
        message_id: Uuid,
        unpinned_by: Uuid,
    ) -> Result<(), RouterError> {
        let participants = self.conversation_service
            .get_participant_ids(conversation_id)
            .await
            .map_err(|e| RouterError::ConversationError(e.to_string()))?;

        let server_message = ServerMessage::MessageUnpinned {
            conversation_id,
            message_id,
            unpinned_by,
        };
        let outbound = OutboundMessage::preserialized(&server_message)
            .map_err(|e| RouterError::Serialization(e.to_string()))?;

        for participant_id in participants {
            if self.connection_manager.has_user(&participant_id) {
                self.connection_manager.send_to_user(&participant_id, outbound.clone()).await;
            } else if let Some(ref cluster_router) = self.cluster_router {
                // Unpin notifications should be queued for offline users
                cluster_router.route_to_user_with_queue(
                    participant_id,
                    outbound.clone(),
                    server_message.clone(),
                ).await.map_err(|e| RouterError::ClusterError(e.to_string()))?;
            }
        }

        Ok(())
    }
}

/// Router errors
#[derive(Debug, thiserror::Error)]
pub enum RouterError {
    #[error("Serialization error: {0}")]
    Serialization(String),

    #[error("Conversation error: {0}")]
    ConversationError(String),

    #[error("Cluster routing error: {0}")]
    ClusterError(String),
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_router_error_serialization_display() {
        let err = RouterError::Serialization("invalid JSON".to_string());
        assert_eq!(err.to_string(), "Serialization error: invalid JSON");
    }

    #[test]
    fn test_router_error_conversation_display() {
        let err = RouterError::ConversationError("not found".to_string());
        assert_eq!(err.to_string(), "Conversation error: not found");
    }

    #[test]
    fn test_router_error_cluster_display() {
        let err = RouterError::ClusterError("connection refused".to_string());
        assert_eq!(err.to_string(), "Cluster routing error: connection refused");
    }

    #[test]
    fn test_router_error_debug() {
        let err = RouterError::Serialization("test".to_string());
        let debug = format!("{:?}", err);
        assert!(debug.contains("Serialization"));
        assert!(debug.contains("test"));
    }
}
