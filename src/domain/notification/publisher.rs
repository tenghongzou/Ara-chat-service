//! Notification publisher - sends events to the Notification Service via Redis Pub/Sub
//!
//! Publishes chat events to Redis channels that the Notification Service subscribes to.
//! Channel format: `notification:user:{user_id}`

use std::sync::Arc;

use redis::AsyncCommands;
use uuid::Uuid;

use super::types::{ChatNotificationType, NotificationEvent, NotificationPayload};
use crate::infrastructure::redis::RedisFallback;

/// Configuration for the notification publisher
#[derive(Debug, Clone)]
pub struct NotificationPublisherConfig {
    /// Whether notifications are enabled
    pub enabled: bool,
    /// Default TTL for notifications (in seconds)
    pub ttl_seconds: u32,
    /// Whether to send new message notifications
    pub notify_new_messages: bool,
    /// Whether to send mention notifications
    pub notify_mentions: bool,
    /// Whether to send reaction notifications
    pub notify_reactions: bool,
}

impl Default for NotificationPublisherConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            ttl_seconds: 3600,
            notify_new_messages: true,
            notify_mentions: true,
            notify_reactions: true,
        }
    }
}

/// Publisher for sending notifications to the Notification Service
pub struct NotificationPublisher {
    redis: Arc<RedisFallback>,
    config: NotificationPublisherConfig,
}

impl NotificationPublisher {
    /// Create a new notification publisher
    pub fn new(redis: Arc<RedisFallback>, config: NotificationPublisherConfig) -> Self {
        Self { redis, config }
    }

    /// Check if the publisher is enabled
    pub fn is_enabled(&self) -> bool {
        self.config.enabled
    }

    /// Get the Redis channel for a user
    fn channel_for_user(user_id: Uuid) -> String {
        format!("notification:user:{}", user_id)
    }

    /// Publish a notification event to Redis
    async fn publish(&self, user_id: Uuid, event: NotificationEvent) {
        if !self.config.enabled {
            return;
        }

        let channel = Self::channel_for_user(user_id);
        let message = match event.to_json() {
            Ok(json) => json,
            Err(e) => {
                tracing::error!(
                    error = %e,
                    user_id = %user_id,
                    "Failed to serialize notification event"
                );
                return;
            }
        };

        // Use Redis fallback for resilient publishing
        let result = self
            .redis
            .with_fallback(|redis| {
                let channel = channel.clone();
                let message = message.clone();
                async move {
                    let mut conn = redis
                        .get_connection()
                        .await
                        .map_err(|e| e.to_string())?;

                    conn.publish::<_, _, ()>(&channel, &message)
                        .await
                        .map_err(|e| e.to_string())
                }
            })
            .await;

        match result {
            Some(_) => {
                tracing::debug!(
                    user_id = %user_id,
                    channel = %channel,
                    event_type = %event.event.event_type,
                    "Published notification to Redis"
                );
            }
            None => {
                tracing::warn!(
                    user_id = %user_id,
                    event_type = %event.event.event_type,
                    "Failed to publish notification (Redis unavailable)"
                );
            }
        }
    }

    /// Send a new message notification to offline users
    ///
    /// This should only be called for users who are not currently connected
    /// to the chat service (offline users).
    pub async fn notify_new_message(&self, user_id: Uuid, payload: NotificationPayload) {
        if !self.config.notify_new_messages {
            return;
        }

        match NotificationEvent::for_user(
            user_id,
            ChatNotificationType::Message,
            payload,
            Some(self.config.ttl_seconds),
        ) {
            Ok(event) => self.publish(user_id, event).await,
            Err(e) => {
                tracing::error!(
                    error = %e,
                    user_id = %user_id,
                    "Failed to create new message notification"
                );
            }
        }
    }

    /// Send mention notifications to multiple users
    pub async fn notify_mentions(&self, user_ids: &[Uuid], payload: NotificationPayload) {
        if !self.config.notify_mentions || user_ids.is_empty() {
            return;
        }

        for &user_id in user_ids {
            match NotificationEvent::for_user(
                user_id,
                ChatNotificationType::Mention,
                payload.clone(),
                Some(self.config.ttl_seconds),
            ) {
                Ok(event) => self.publish(user_id, event).await,
                Err(e) => {
                    tracing::error!(
                        error = %e,
                        user_id = %user_id,
                        "Failed to create mention notification"
                    );
                }
            }
        }

        tracing::debug!(
            mention_count = user_ids.len(),
            "Sent mention notifications"
        );
    }

    /// Send a reaction notification to the message author
    ///
    /// Only sends notification when someone adds a reaction (not removes).
    pub async fn notify_reaction(&self, message_author_id: Uuid, payload: NotificationPayload) {
        if !self.config.notify_reactions {
            return;
        }

        // Only notify on "add" actions
        if payload.action.as_deref() != Some("add") {
            return;
        }

        match NotificationEvent::for_user(
            message_author_id,
            ChatNotificationType::Reaction,
            payload,
            Some(self.config.ttl_seconds),
        ) {
            Ok(event) => self.publish(message_author_id, event).await,
            Err(e) => {
                tracing::error!(
                    error = %e,
                    user_id = %message_author_id,
                    "Failed to create reaction notification"
                );
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_config_default() {
        let config = NotificationPublisherConfig::default();
        assert!(config.enabled);
        assert_eq!(config.ttl_seconds, 3600);
        assert!(config.notify_new_messages);
        assert!(config.notify_mentions);
        assert!(config.notify_reactions);
    }

    #[test]
    fn test_channel_for_user() {
        let user_id = Uuid::parse_str("550e8400-e29b-41d4-a716-446655440000").unwrap();
        let channel = NotificationPublisher::channel_for_user(user_id);
        assert_eq!(channel, "notification:user:550e8400-e29b-41d4-a716-446655440000");
    }

    #[test]
    fn test_publisher_disabled() {
        let redis = Arc::new(RedisFallback::new(None));
        let config = NotificationPublisherConfig {
            enabled: false,
            ..Default::default()
        };
        let publisher = NotificationPublisher::new(redis, config);
        assert!(!publisher.is_enabled());
    }

    #[test]
    fn test_publisher_enabled() {
        let redis = Arc::new(RedisFallback::new(None));
        let config = NotificationPublisherConfig::default();
        let publisher = NotificationPublisher::new(redis, config);
        assert!(publisher.is_enabled());
    }

    #[tokio::test]
    async fn test_notify_new_message_disabled() {
        let redis = Arc::new(RedisFallback::new(None));
        let config = NotificationPublisherConfig {
            notify_new_messages: false,
            ..Default::default()
        };
        let publisher = NotificationPublisher::new(redis, config);

        let payload = NotificationPayload::new_message(
            Uuid::new_v4(),
            Uuid::new_v4(),
            Uuid::new_v4(),
            Some("test".to_string()),
        );

        // Should not panic even without Redis
        publisher.notify_new_message(Uuid::new_v4(), payload).await;
    }

    #[tokio::test]
    async fn test_notify_mentions_empty() {
        let redis = Arc::new(RedisFallback::new(None));
        let publisher = NotificationPublisher::new(redis, NotificationPublisherConfig::default());

        let payload = NotificationPayload::mention(
            Uuid::new_v4(),
            Uuid::new_v4(),
            Uuid::new_v4(),
            None,
            None,
        );

        // Empty list should not cause issues
        publisher.notify_mentions(&[], payload).await;
    }

    #[tokio::test]
    async fn test_notify_reaction_only_add() {
        let redis = Arc::new(RedisFallback::new(None));
        let publisher = NotificationPublisher::new(redis, NotificationPublisherConfig::default());

        // Remove action should be ignored
        let payload = NotificationPayload::reaction(
            Uuid::new_v4(),
            Uuid::new_v4(),
            Uuid::new_v4(),
            "👍".to_string(),
            "remove",
        );

        // Should not panic, and should be a no-op
        publisher.notify_reaction(Uuid::new_v4(), payload).await;
    }
}
