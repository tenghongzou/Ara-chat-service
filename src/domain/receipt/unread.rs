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
