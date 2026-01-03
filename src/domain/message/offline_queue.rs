//! Offline message queue for users who are not connected
//!
//! Stores pending messages in Redis and delivers them when users reconnect.

use std::sync::Arc;
use std::time::Duration;

use redis::AsyncCommands;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::message::ServerMessage;
use crate::redis::RedisPool;

/// Message stored in offline queue
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QueuedMessage {
    pub id: Uuid,
    pub message: ServerMessage,
    pub queued_at: i64,
}

impl QueuedMessage {
    pub fn new(message: ServerMessage) -> Self {
        Self {
            id: Uuid::new_v4(),
            message,
            queued_at: chrono::Utc::now().timestamp_millis(),
        }
    }
}

/// Offline message queue backed by Redis
pub struct OfflineQueue {
    redis: Option<Arc<RedisPool>>,
    /// Maximum messages to store per user
    max_messages_per_user: usize,
    /// TTL for queued messages (default 7 days)
    message_ttl: Duration,
    prefix: String,
}

impl OfflineQueue {
    pub fn new(redis: Option<Arc<RedisPool>>) -> Self {
        Self {
            redis,
            max_messages_per_user: 1000,
            message_ttl: Duration::from_secs(7 * 24 * 60 * 60), // 7 days
            prefix: "chat:offline".to_string(),
        }
    }

    pub fn with_config(
        redis: Option<Arc<RedisPool>>,
        max_messages_per_user: usize,
        message_ttl: Duration,
    ) -> Self {
        Self {
            redis,
            max_messages_per_user,
            message_ttl,
            prefix: "chat:offline".to_string(),
        }
    }

    fn queue_key(&self, user_id: Uuid) -> String {
        format!("{}:queue:{}", self.prefix, user_id)
    }

    /// Queue a message for an offline user
    pub async fn queue_message(
        &self,
        user_id: Uuid,
        message: ServerMessage,
    ) -> Result<(), OfflineQueueError> {
        let redis = match &self.redis {
            Some(r) => r,
            None => return Ok(()), // No Redis, skip queuing
        };

        let mut conn = redis.get_connection().await
            .map_err(|e| OfflineQueueError::Redis(e.to_string()))?;

        let key = self.queue_key(user_id);
        let queued = QueuedMessage::new(message);
        let serialized = serde_json::to_string(&queued)
            .map_err(|e| OfflineQueueError::Serialization(e.to_string()))?;

        // Add to list (RPUSH for FIFO order)
        let _: () = conn.rpush(&key, &serialized).await
            .map_err(|e| OfflineQueueError::Redis(e.to_string()))?;

        // Trim to max messages (keep latest)
        let _: () = conn.ltrim(&key, -(self.max_messages_per_user as isize), -1).await
            .map_err(|e| OfflineQueueError::Redis(e.to_string()))?;

        // Set/refresh TTL
        let _: () = conn.expire(&key, self.message_ttl.as_secs() as i64).await
            .map_err(|e| OfflineQueueError::Redis(e.to_string()))?;

        tracing::debug!(
            user_id = %user_id,
            message_id = %queued.id,
            "Message queued for offline user"
        );

        Ok(())
    }

    /// Get all pending messages for a user (does not remove them)
    pub async fn peek_messages(
        &self,
        user_id: Uuid,
        limit: Option<usize>,
    ) -> Result<Vec<QueuedMessage>, OfflineQueueError> {
        let redis = match &self.redis {
            Some(r) => r,
            None => return Ok(vec![]),
        };

        let mut conn = redis.get_connection().await
            .map_err(|e| OfflineQueueError::Redis(e.to_string()))?;

        let key = self.queue_key(user_id);
        let end = limit.map(|l| l as isize - 1).unwrap_or(-1);

        let messages: Vec<String> = conn.lrange(&key, 0, end).await
            .map_err(|e| OfflineQueueError::Redis(e.to_string()))?;

        let mut result = Vec::with_capacity(messages.len());
        for msg in messages {
            match serde_json::from_str::<QueuedMessage>(&msg) {
                Ok(queued) => result.push(queued),
                Err(e) => {
                    tracing::warn!(error = %e, "Failed to deserialize queued message");
                }
            }
        }

        Ok(result)
    }

    /// Get and remove all pending messages for a user
    pub async fn drain_messages(
        &self,
        user_id: Uuid,
    ) -> Result<Vec<QueuedMessage>, OfflineQueueError> {
        let redis = match &self.redis {
            Some(r) => r,
            None => return Ok(vec![]),
        };

        let mut conn = redis.get_connection().await
            .map_err(|e| OfflineQueueError::Redis(e.to_string()))?;

        let key = self.queue_key(user_id);

        // Get all messages
        let messages: Vec<String> = conn.lrange(&key, 0, -1).await
            .map_err(|e| OfflineQueueError::Redis(e.to_string()))?;

        // Delete the key
        let _: () = conn.del(&key).await
            .map_err(|e| OfflineQueueError::Redis(e.to_string()))?;

        let mut result = Vec::with_capacity(messages.len());
        for msg in messages {
            match serde_json::from_str::<QueuedMessage>(&msg) {
                Ok(queued) => result.push(queued),
                Err(e) => {
                    tracing::warn!(error = %e, "Failed to deserialize queued message");
                }
            }
        }

        if !result.is_empty() {
            tracing::info!(
                user_id = %user_id,
                count = result.len(),
                "Drained offline message queue"
            );
        }

        Ok(result)
    }

    /// Remove a specific message from the queue (e.g., after successful delivery)
    pub async fn remove_message(
        &self,
        user_id: Uuid,
        message_id: Uuid,
    ) -> Result<bool, OfflineQueueError> {
        let redis = match &self.redis {
            Some(r) => r,
            None => return Ok(false),
        };

        let mut conn = redis.get_connection().await
            .map_err(|e| OfflineQueueError::Redis(e.to_string()))?;

        let key = self.queue_key(user_id);

        // Get all messages
        let messages: Vec<String> = conn.lrange(&key, 0, -1).await
            .map_err(|e| OfflineQueueError::Redis(e.to_string()))?;

        // Find and remove the specific message
        for msg in messages {
            if let Ok(queued) = serde_json::from_str::<QueuedMessage>(&msg) {
                if queued.id == message_id {
                    let removed: i64 = conn.lrem(&key, 1, &msg).await
                        .map_err(|e| OfflineQueueError::Redis(e.to_string()))?;
                    return Ok(removed > 0);
                }
            }
        }

        Ok(false)
    }

    /// Get queue size for a user
    pub async fn queue_size(&self, user_id: Uuid) -> Result<usize, OfflineQueueError> {
        let redis = match &self.redis {
            Some(r) => r,
            None => return Ok(0),
        };

        let mut conn = redis.get_connection().await
            .map_err(|e| OfflineQueueError::Redis(e.to_string()))?;

        let key = self.queue_key(user_id);
        let size: usize = conn.llen(&key).await
            .map_err(|e| OfflineQueueError::Redis(e.to_string()))?;

        Ok(size)
    }

    /// Clear the queue for a user
    pub async fn clear_queue(&self, user_id: Uuid) -> Result<(), OfflineQueueError> {
        let redis = match &self.redis {
            Some(r) => r,
            None => return Ok(()),
        };

        let mut conn = redis.get_connection().await
            .map_err(|e| OfflineQueueError::Redis(e.to_string()))?;

        let key = self.queue_key(user_id);
        let _: () = conn.del(&key).await
            .map_err(|e| OfflineQueueError::Redis(e.to_string()))?;

        Ok(())
    }
}

#[derive(Debug, thiserror::Error)]
pub enum OfflineQueueError {
    #[error("Redis error: {0}")]
    Redis(String),
    #[error("Serialization error: {0}")]
    Serialization(String),
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_queued_message() {
        let msg = ServerMessage::Pong;
        let queued = QueuedMessage::new(msg);

        assert!(!queued.id.is_nil());
        assert!(queued.queued_at > 0);
    }

    #[test]
    fn test_queued_message_serialization() {
        let msg = ServerMessage::Pong;
        let queued = QueuedMessage::new(msg);

        let serialized = serde_json::to_string(&queued).unwrap();
        let deserialized: QueuedMessage = serde_json::from_str(&serialized).unwrap();

        assert_eq!(queued.id, deserialized.id);
        assert_eq!(queued.queued_at, deserialized.queued_at);
    }

    #[test]
    fn test_offline_queue_new_defaults() {
        let queue = OfflineQueue::new(None);
        assert_eq!(queue.max_messages_per_user, 1000);
        assert_eq!(queue.message_ttl, Duration::from_secs(7 * 24 * 60 * 60));
        assert_eq!(queue.prefix, "chat:offline");
    }

    #[test]
    fn test_offline_queue_with_config() {
        let queue = OfflineQueue::with_config(None, 500, Duration::from_secs(3600));
        assert_eq!(queue.max_messages_per_user, 500);
        assert_eq!(queue.message_ttl, Duration::from_secs(3600));
    }

    #[test]
    fn test_queue_key_format() {
        let queue = OfflineQueue::new(None);
        let user_id = Uuid::parse_str("550e8400-e29b-41d4-a716-446655440000").unwrap();
        let key = queue.queue_key(user_id);
        assert_eq!(key, "chat:offline:queue:550e8400-e29b-41d4-a716-446655440000");
    }

    #[tokio::test]
    async fn test_queue_message_no_redis() {
        let queue = OfflineQueue::new(None);
        let user_id = Uuid::new_v4();
        let msg = ServerMessage::Pong;

        // Should succeed silently when no Redis
        let result = queue.queue_message(user_id, msg).await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_peek_messages_no_redis() {
        let queue = OfflineQueue::new(None);
        let user_id = Uuid::new_v4();

        // Should return empty vec when no Redis
        let result = queue.peek_messages(user_id, None).await.unwrap();
        assert!(result.is_empty());
    }

    #[tokio::test]
    async fn test_drain_messages_no_redis() {
        let queue = OfflineQueue::new(None);
        let user_id = Uuid::new_v4();

        // Should return empty vec when no Redis
        let result = queue.drain_messages(user_id).await.unwrap();
        assert!(result.is_empty());
    }

    #[tokio::test]
    async fn test_remove_message_no_redis() {
        let queue = OfflineQueue::new(None);
        let user_id = Uuid::new_v4();
        let message_id = Uuid::new_v4();

        // Should return false when no Redis
        let result = queue.remove_message(user_id, message_id).await.unwrap();
        assert!(!result);
    }

    #[tokio::test]
    async fn test_queue_size_no_redis() {
        let queue = OfflineQueue::new(None);
        let user_id = Uuid::new_v4();

        // Should return 0 when no Redis
        let result = queue.queue_size(user_id).await.unwrap();
        assert_eq!(result, 0);
    }

    #[tokio::test]
    async fn test_clear_queue_no_redis() {
        let queue = OfflineQueue::new(None);
        let user_id = Uuid::new_v4();

        // Should succeed silently when no Redis
        let result = queue.clear_queue(user_id).await;
        assert!(result.is_ok());
    }

    #[test]
    fn test_offline_queue_error_redis_display() {
        let err = OfflineQueueError::Redis("connection refused".to_string());
        assert_eq!(err.to_string(), "Redis error: connection refused");
    }

    #[test]
    fn test_offline_queue_error_serialization_display() {
        let err = OfflineQueueError::Serialization("invalid format".to_string());
        assert_eq!(err.to_string(), "Serialization error: invalid format");
    }

    #[test]
    fn test_offline_queue_error_debug() {
        let err = OfflineQueueError::Redis("test".to_string());
        let debug = format!("{:?}", err);
        assert!(debug.contains("Redis"));
        assert!(debug.contains("test"));
    }
}
