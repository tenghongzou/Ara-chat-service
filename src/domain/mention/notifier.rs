//! Mention notifier - sends notifications for @mentions

use std::sync::Arc;

use uuid::Uuid;

use crate::message::{OfflineQueue, OutboundMessage, ServerMessage};
use crate::connection::ConnectionManager;
use crate::cluster::ClusterRouter;
use crate::notification::{NotificationPayload, NotificationPublisher};

/// Notifies users when they are mentioned
pub struct MentionNotifier {
    connection_manager: Arc<ConnectionManager>,
    cluster_router: Arc<ClusterRouter>,
    offline_queue: Option<Arc<OfflineQueue>>,
    notification_publisher: Option<Arc<NotificationPublisher>>,
}

impl MentionNotifier {
    pub fn new(
        connection_manager: Arc<ConnectionManager>,
        cluster_router: Arc<ClusterRouter>,
    ) -> Self {
        Self {
            connection_manager,
            cluster_router,
            offline_queue: None,
            notification_publisher: None,
        }
    }

    /// Set offline queue for storing mentions to offline users
    pub fn with_offline_queue(mut self, queue: Arc<OfflineQueue>) -> Self {
        self.offline_queue = Some(queue);
        self
    }

    /// Set notification publisher for push notifications
    pub fn with_notification_publisher(mut self, publisher: Arc<NotificationPublisher>) -> Self {
        self.notification_publisher = Some(publisher);
        self
    }

    /// Notify mentioned users about a message
    pub async fn notify_mentions(
        &self,
        conversation_id: Uuid,
        message_id: Uuid,
        sender_id: Uuid,
        sender_name: &str,
        content_preview: &str,
        mentioned_users: &[Uuid],
    ) -> Result<(), MentionNotifyError> {
        if mentioned_users.is_empty() {
            return Ok(());
        }

        let preview = if content_preview.len() > 100 {
            format!("{}...", &content_preview[..97])
        } else {
            content_preview.to_string()
        };

        let message = ServerMessage::Mention {
            conversation_id,
            message_id,
            sender_id,
            sender_name: sender_name.to_string(),
            preview: preview.clone(),
        };

        let outbound = OutboundMessage::preserialized(&message)
            .map_err(|e| MentionNotifyError::Serialization(e.to_string()))?;

        for &user_id in mentioned_users {
            // Don't notify the sender
            if user_id == sender_id {
                continue;
            }

            // Try local delivery first
            if self.connection_manager.has_user(&user_id) {
                self.connection_manager.send_to_user(&user_id, outbound.clone()).await;
            } else {
                // Route through cluster with offline queue support
                // Mentions are important - queue for offline users
                let _ = self.cluster_router.route_to_user_with_queue(
                    user_id,
                    outbound.clone(),
                    message.clone(),
                ).await;
            }
        }

        tracing::debug!(
            conversation_id = %conversation_id,
            message_id = %message_id,
            mentioned_count = mentioned_users.len(),
            "Sent mention notifications"
        );

        // Send push notifications via Notification Service
        if let Some(ref publisher) = self.notification_publisher {
            let payload = NotificationPayload::mention(
                conversation_id,
                message_id,
                sender_id,
                Some(sender_name.to_string()),
                Some(preview),
            );

            // Filter out sender from notification targets
            let notify_targets: Vec<Uuid> = mentioned_users
                .iter()
                .filter(|&&uid| uid != sender_id)
                .copied()
                .collect();

            publisher.notify_mentions(&notify_targets, payload).await;
        }

        Ok(())
    }
}

#[derive(Debug, thiserror::Error)]
pub enum MentionNotifyError {
    #[error("Serialization error: {0}")]
    Serialization(String),

    #[error("Routing error: {0}")]
    Routing(String),
}
