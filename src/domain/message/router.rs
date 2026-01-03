//! Message router - routes messages to recipients across the cluster

use std::sync::Arc;

use uuid::Uuid;

use super::types::{ChatMessage, OutboundMessage, ServerMessage};
use crate::cluster::ClusterRouter;
use crate::connection::ConnectionManager;
use crate::conversation::ConversationService;

/// Routes messages to conversation participants
pub struct MessageRouter {
    connection_manager: Arc<ConnectionManager>,
    cluster_router: Option<Arc<ClusterRouter>>,
    conversation_service: Arc<ConversationService>,
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
        }
    }

    /// Route a message to all participants in the conversation
    pub async fn route_message(&self, message: &ChatMessage) -> Result<(), RouterError> {
        // Get all participants in the conversation
        let participants = self.conversation_service
            .get_participant_ids(message.conversation_id)
            .await
            .map_err(|e| RouterError::ConversationError(e.to_string()))?;

        // Pre-serialize the message for efficient multi-send
        let server_message = ServerMessage::Message {
            message: message.clone(),
        };
        let outbound = OutboundMessage::preserialized(&server_message)
            .map_err(|e| RouterError::Serialization(e.to_string()))?;

        // Send to each participant
        for user_id in participants {
            // Skip sender - they already have confirmation
            if user_id == message.sender_id {
                continue;
            }

            // Try local delivery first
            if self.connection_manager.has_user(&user_id) {
                self.connection_manager.send_to_user(&user_id, outbound.clone()).await;
            } else if let Some(ref cluster_router) = self.cluster_router {
                // Route through cluster with offline queue support
                cluster_router.route_to_user_with_queue(
                    user_id,
                    outbound.clone(),
                    server_message.clone(),
                ).await.map_err(|e| RouterError::ClusterError(e.to_string()))?;
            }
            // If no cluster router and user not local, message will be delivered when they reconnect
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
