//! Unread message counter

use std::collections::HashMap;
use std::sync::Arc;

use uuid::Uuid;

use crate::redis::{RedisCache, RedisPool};

/// Manages unread message counts per user per conversation
pub struct UnreadCounter {
    cache: Option<RedisCache>,
}

impl UnreadCounter {
    pub fn new(redis: Option<Arc<RedisPool>>) -> Self {
        let cache = redis.map(|pool| RedisCache::new(pool));
        Self { cache }
    }

    /// Increment unread count for a user in a conversation
    pub async fn increment(
        &self,
        user_id: Uuid,
        conversation_id: Uuid,
    ) -> Result<u64, UnreadError> {
        match &self.cache {
            Some(cache) => cache.increment_unread(user_id, conversation_id).await
                .map_err(|e| UnreadError::Redis(e.to_string())),
            None => Ok(0),
        }
    }

    /// Reset unread count for a user in a conversation (when marked as read)
    pub async fn reset(
        &self,
        user_id: Uuid,
        conversation_id: Uuid,
    ) -> Result<u64, UnreadError> {
        match &self.cache {
            Some(cache) => cache.reset_unread(user_id, conversation_id).await
                .map_err(|e| UnreadError::Redis(e.to_string())),
            None => Ok(0),
        }
    }

    /// Get unread count for a specific conversation
    pub async fn get_count(
        &self,
        user_id: Uuid,
        conversation_id: Uuid,
    ) -> Result<u64, UnreadError> {
        match &self.cache {
            Some(cache) => cache.get_unread(user_id, conversation_id).await
                .map_err(|e| UnreadError::Redis(e.to_string())),
            None => Ok(0),
        }
    }

    /// Get total unread count across all conversations
    pub async fn get_total(&self, user_id: Uuid) -> Result<u64, UnreadError> {
        match &self.cache {
            Some(cache) => cache.get_total_unread(user_id).await
                .map_err(|e| UnreadError::Redis(e.to_string())),
            None => Ok(0),
        }
    }

    /// Get unread counts for all user's conversations
    pub async fn get_all_counts(
        &self,
        user_id: Uuid,
    ) -> Result<HashMap<Uuid, u64>, UnreadError> {
        match &self.cache {
            Some(cache) => cache.get_all_unread_counts(user_id).await
                .map_err(|e| UnreadError::Redis(e.to_string())),
            None => Ok(HashMap::new()),
        }
    }

    /// Sync unread counts to client - returns total and per-conversation counts
    pub async fn sync_to_client(&self, user_id: Uuid) -> Result<(u64, HashMap<Uuid, u64>), UnreadError> {
        match &self.cache {
            Some(cache) => cache.get_unread_sync(user_id).await
                .map_err(|e| UnreadError::Redis(e.to_string())),
            None => Ok((0, HashMap::new())),
        }
    }
}

#[derive(Debug, thiserror::Error)]
pub enum UnreadError {
    #[error("Redis error: {0}")]
    Redis(String),
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_unread_counter_new_no_redis() {
        let counter = UnreadCounter::new(None);
        assert!(counter.cache.is_none());
    }

    #[tokio::test]
    async fn test_increment_no_redis() {
        let counter = UnreadCounter::new(None);
        let user_id = Uuid::new_v4();
        let conv_id = Uuid::new_v4();

        let result = counter.increment(user_id, conv_id).await.unwrap();
        assert_eq!(result, 0);
    }

    #[tokio::test]
    async fn test_reset_no_redis() {
        let counter = UnreadCounter::new(None);
        let user_id = Uuid::new_v4();
        let conv_id = Uuid::new_v4();

        let result = counter.reset(user_id, conv_id).await.unwrap();
        assert_eq!(result, 0);
    }

    #[tokio::test]
    async fn test_get_count_no_redis() {
        let counter = UnreadCounter::new(None);
        let user_id = Uuid::new_v4();
        let conv_id = Uuid::new_v4();

        let result = counter.get_count(user_id, conv_id).await.unwrap();
        assert_eq!(result, 0);
    }

    #[tokio::test]
    async fn test_get_total_no_redis() {
        let counter = UnreadCounter::new(None);
        let user_id = Uuid::new_v4();

        let result = counter.get_total(user_id).await.unwrap();
        assert_eq!(result, 0);
    }

    #[tokio::test]
    async fn test_get_all_counts_no_redis() {
        let counter = UnreadCounter::new(None);
        let user_id = Uuid::new_v4();

        let result = counter.get_all_counts(user_id).await.unwrap();
        assert!(result.is_empty());
    }

    #[tokio::test]
    async fn test_sync_to_client_no_redis() {
        let counter = UnreadCounter::new(None);
        let user_id = Uuid::new_v4();

        let (total, counts) = counter.sync_to_client(user_id).await.unwrap();
        assert_eq!(total, 0);
        assert!(counts.is_empty());
    }

    #[test]
    fn test_unread_error_display() {
        let err = UnreadError::Redis("connection failed".to_string());
        assert_eq!(err.to_string(), "Redis error: connection failed");
    }

    #[test]
    fn test_unread_error_debug() {
        let err = UnreadError::Redis("test".to_string());
        let debug = format!("{:?}", err);
        assert!(debug.contains("Redis"));
        assert!(debug.contains("test"));
    }
}
