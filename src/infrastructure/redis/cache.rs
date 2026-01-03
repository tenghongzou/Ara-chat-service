//! Redis cache utilities for chat service

use std::sync::Arc;
use std::collections::HashMap;

use redis::AsyncCommands;
use uuid::Uuid;

use super::pool::RedisPool;

/// Redis cache for chat data
pub struct RedisCache {
    pool: Arc<RedisPool>,
    prefix: String,
}

impl RedisCache {
    pub fn new(pool: Arc<RedisPool>) -> Self {
        Self {
            pool,
            prefix: "chat".to_string(),
        }
    }

    pub fn with_prefix(pool: Arc<RedisPool>, prefix: String) -> Self {
        Self { pool, prefix }
    }

    // ==================== Unread Counts ====================

    /// Get unread count key
    fn unread_key(&self, user_id: Uuid, conversation_id: Uuid) -> String {
        format!("{}:unread:{}:{}", self.prefix, user_id, conversation_id)
    }

    /// Get total unread key
    fn total_unread_key(&self, user_id: Uuid) -> String {
        format!("{}:unread:total:{}", self.prefix, user_id)
    }

    /// Increment unread count for a user in a conversation
    pub async fn increment_unread(
        &self,
        user_id: Uuid,
        conversation_id: Uuid,
    ) -> Result<u64, CacheError> {
        let mut conn = self.pool.get_connection().await
            .map_err(|e| CacheError::Redis(e.to_string()))?;

        let key = self.unread_key(user_id, conversation_id);
        let total_key = self.total_unread_key(user_id);

        // Increment both keys atomically
        let (count, _): (u64, u64) = redis::pipe()
            .atomic()
            .incr(&key, 1u64)
            .incr(&total_key, 1u64)
            .query_async(&mut conn)
            .await
            .map_err(|e| CacheError::Redis(e.to_string()))?;

        // Set TTL (30 days)
        let _: () = redis::pipe()
            .expire(&key, 30 * 24 * 60 * 60)
            .expire(&total_key, 30 * 24 * 60 * 60)
            .query_async(&mut conn)
            .await
            .map_err(|e| CacheError::Redis(e.to_string()))?;

        Ok(count)
    }

    /// Reset unread count for a user in a conversation
    pub async fn reset_unread(
        &self,
        user_id: Uuid,
        conversation_id: Uuid,
    ) -> Result<u64, CacheError> {
        let mut conn = self.pool.get_connection().await
            .map_err(|e| CacheError::Redis(e.to_string()))?;

        let key = self.unread_key(user_id, conversation_id);
        let total_key = self.total_unread_key(user_id);

        // Get current count before deleting
        let count: u64 = conn.get(&key).await.unwrap_or(0);

        if count > 0 {
            // Delete specific key and decrement total
            let _: () = redis::pipe()
                .atomic()
                .del(&key)
                .decr(&total_key, count)
                .query_async(&mut conn)
                .await
                .map_err(|e| CacheError::Redis(e.to_string()))?;
        }

        Ok(count)
    }

    /// Get unread count for a specific conversation
    pub async fn get_unread(
        &self,
        user_id: Uuid,
        conversation_id: Uuid,
    ) -> Result<u64, CacheError> {
        let mut conn = self.pool.get_connection().await
            .map_err(|e| CacheError::Redis(e.to_string()))?;

        let key = self.unread_key(user_id, conversation_id);
        let count: u64 = conn.get(&key).await.unwrap_or(0);

        Ok(count)
    }

    /// Get total unread count for a user
    pub async fn get_total_unread(&self, user_id: Uuid) -> Result<u64, CacheError> {
        let mut conn = self.pool.get_connection().await
            .map_err(|e| CacheError::Redis(e.to_string()))?;

        let key = self.total_unread_key(user_id);
        let count: u64 = conn.get(&key).await.unwrap_or(0);

        Ok(count)
    }

    /// Get unread count key pattern for scanning
    fn unread_pattern(&self, user_id: Uuid) -> String {
        format!("{}:unread:{}:*", self.prefix, user_id)
    }

    /// Get all unread counts for a user across all conversations
    pub async fn get_all_unread_counts(
        &self,
        user_id: Uuid,
    ) -> Result<HashMap<Uuid, u64>, CacheError> {
        let mut conn = self.pool.get_connection().await
            .map_err(|e| CacheError::Redis(e.to_string()))?;

        let pattern = self.unread_pattern(user_id);
        let prefix_len = format!("{}:unread:{}:", self.prefix, user_id).len();

        // Use SCAN to find all matching keys
        let mut cursor = 0u64;
        let mut all_keys: Vec<String> = Vec::new();

        loop {
            let (new_cursor, keys): (u64, Vec<String>) = redis::cmd("SCAN")
                .arg(cursor)
                .arg("MATCH")
                .arg(&pattern)
                .arg("COUNT")
                .arg(100)
                .query_async(&mut conn)
                .await
                .map_err(|e| CacheError::Redis(e.to_string()))?;

            all_keys.extend(keys);
            cursor = new_cursor;

            if cursor == 0 {
                break;
            }
        }

        if all_keys.is_empty() {
            return Ok(HashMap::new());
        }

        // Get all values at once with MGET
        let values: Vec<Option<u64>> = conn.mget(&all_keys).await
            .map_err(|e| CacheError::Redis(e.to_string()))?;

        let mut result = HashMap::new();
        for (key, value) in all_keys.iter().zip(values.iter()) {
            if let Some(count) = value {
                // Extract conversation_id from key
                let conv_str = &key[prefix_len..];
                if let Ok(conv_id) = Uuid::parse_str(conv_str) {
                    if *count > 0 {
                        result.insert(conv_id, *count);
                    }
                }
            }
        }

        Ok(result)
    }

    /// Get unread sync data: total and per-conversation counts
    pub async fn get_unread_sync(
        &self,
        user_id: Uuid,
    ) -> Result<(u64, HashMap<Uuid, u64>), CacheError> {
        let total = self.get_total_unread(user_id).await?;
        let per_conversation = self.get_all_unread_counts(user_id).await?;
        Ok((total, per_conversation))
    }

    // ==================== Conversation Members Cache ====================

    /// Get conversation members key
    fn members_key(&self, conversation_id: Uuid) -> String {
        format!("{}:conv:members:{}", self.prefix, conversation_id)
    }

    /// Cache conversation members
    pub async fn cache_members(
        &self,
        conversation_id: Uuid,
        member_ids: &[Uuid],
    ) -> Result<(), CacheError> {
        let mut conn = self.pool.get_connection().await
            .map_err(|e| CacheError::Redis(e.to_string()))?;

        let key = self.members_key(conversation_id);
        let member_strs: Vec<String> = member_ids.iter().map(|id| id.to_string()).collect();

        // Clear and set new members
        let _: () = redis::pipe()
            .atomic()
            .del(&key)
            .sadd(&key, &member_strs)
            .expire(&key, 60 * 60) // 1 hour TTL
            .query_async(&mut conn)
            .await
            .map_err(|e| CacheError::Redis(e.to_string()))?;

        Ok(())
    }

    /// Get cached conversation members
    pub async fn get_cached_members(
        &self,
        conversation_id: Uuid,
    ) -> Result<Option<Vec<Uuid>>, CacheError> {
        let mut conn = self.pool.get_connection().await
            .map_err(|e| CacheError::Redis(e.to_string()))?;

        let key = self.members_key(conversation_id);
        let members: Vec<String> = conn.smembers(&key).await.unwrap_or_default();

        if members.is_empty() {
            return Ok(None);
        }

        let uuids: Vec<Uuid> = members
            .iter()
            .filter_map(|s| Uuid::parse_str(s).ok())
            .collect();

        Ok(Some(uuids))
    }

    // ==================== Typing Indicators ====================

    /// Get typing key
    fn typing_key(&self, conversation_id: Uuid) -> String {
        format!("{}:typing:{}", self.prefix, conversation_id)
    }

    /// Set user typing status
    pub async fn set_typing(
        &self,
        conversation_id: Uuid,
        user_id: Uuid,
        is_typing: bool,
    ) -> Result<(), CacheError> {
        let mut conn = self.pool.get_connection().await
            .map_err(|e| CacheError::Redis(e.to_string()))?;

        let key = self.typing_key(conversation_id);

        if is_typing {
            let now = chrono::Utc::now().timestamp();
            let _: () = redis::pipe()
                .hset(&key, user_id.to_string(), now)
                .expire(&key, 10) // 10 second TTL
                .query_async(&mut conn)
                .await
                .map_err(|e| CacheError::Redis(e.to_string()))?;
        } else {
            let _: () = conn.hdel(&key, user_id.to_string()).await
                .map_err(|e| CacheError::Redis(e.to_string()))?;
        }

        Ok(())
    }

    /// Get users currently typing in a conversation
    pub async fn get_typing_users(
        &self,
        conversation_id: Uuid,
    ) -> Result<Vec<Uuid>, CacheError> {
        let mut conn = self.pool.get_connection().await
            .map_err(|e| CacheError::Redis(e.to_string()))?;

        let key = self.typing_key(conversation_id);
        let typing: HashMap<String, i64> = conn.hgetall(&key).await.unwrap_or_default();

        let now = chrono::Utc::now().timestamp();
        let users: Vec<Uuid> = typing
            .iter()
            .filter(|(_, &ts)| now - ts < 10) // Only show if typed within last 10 seconds
            .filter_map(|(id, _)| Uuid::parse_str(id).ok())
            .collect();

        Ok(users)
    }

    // ==================== User Conversations Cache ====================

    /// Get user conversations sorted set key
    fn user_convs_key(&self, user_id: Uuid) -> String {
        format!("{}:user:convs:{}", self.prefix, user_id)
    }

    /// Add/update conversation in user's list
    pub async fn update_user_conversation(
        &self,
        user_id: Uuid,
        conversation_id: Uuid,
        last_message_at: i64,
    ) -> Result<(), CacheError> {
        let mut conn = self.pool.get_connection().await
            .map_err(|e| CacheError::Redis(e.to_string()))?;

        let key = self.user_convs_key(user_id);

        let _: () = redis::pipe()
            .zadd(&key, conversation_id.to_string(), last_message_at as f64)
            .expire(&key, 60 * 60) // 1 hour TTL
            .query_async(&mut conn)
            .await
            .map_err(|e| CacheError::Redis(e.to_string()))?;

        Ok(())
    }

    /// Get user's recent conversation IDs
    pub async fn get_user_conversations(
        &self,
        user_id: Uuid,
        limit: usize,
    ) -> Result<Vec<Uuid>, CacheError> {
        let mut conn = self.pool.get_connection().await
            .map_err(|e| CacheError::Redis(e.to_string()))?;

        let key = self.user_convs_key(user_id);

        let conv_ids: Vec<String> = conn
            .zrevrange(&key, 0, (limit - 1) as isize)
            .await
            .unwrap_or_default();

        let uuids: Vec<Uuid> = conv_ids
            .iter()
            .filter_map(|s| Uuid::parse_str(s).ok())
            .collect();

        Ok(uuids)
    }
}

#[derive(Debug, thiserror::Error)]
pub enum CacheError {
    #[error("Redis error: {0}")]
    Redis(String),
}
