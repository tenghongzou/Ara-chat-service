//! Rate limiting for chat service

use std::sync::Arc;
use std::time::Duration;

use redis::AsyncCommands;
use uuid::Uuid;

use crate::redis::RedisPool;

/// Rate limiter using Redis for distributed rate limiting
pub struct RateLimiter {
    redis: Option<Arc<RedisPool>>,
    /// Messages per window
    max_messages: u32,
    /// Window duration in seconds
    window_seconds: u64,
    prefix: String,
}

impl RateLimiter {
    pub fn new(redis: Option<Arc<RedisPool>>) -> Self {
        Self {
            redis,
            max_messages: 60,  // 60 messages
            window_seconds: 60, // per minute
            prefix: "chat:ratelimit".to_string(),
        }
    }

    pub fn with_limits(
        redis: Option<Arc<RedisPool>>,
        max_messages: u32,
        window_seconds: u64,
    ) -> Self {
        Self {
            redis,
            max_messages,
            window_seconds,
            prefix: "chat:ratelimit".to_string(),
        }
    }

    fn user_key(&self, user_id: Uuid) -> String {
        format!("{}:user:{}", self.prefix, user_id)
    }

    fn conversation_key(&self, conversation_id: Uuid) -> String {
        format!("{}:conv:{}", self.prefix, conversation_id)
    }

    /// Check if user can send a message (returns remaining quota)
    pub async fn check_user_rate(
        &self,
        user_id: Uuid,
    ) -> Result<RateLimitResult, RateLimitError> {
        let redis = match &self.redis {
            Some(r) => r,
            None => return Ok(RateLimitResult::allowed(self.max_messages)), // No Redis, allow
        };

        let mut conn = redis.get_connection().await
            .map_err(|e| RateLimitError::Redis(e.to_string()))?;

        let key = self.user_key(user_id);

        // Use Redis INCR with EXPIRE for sliding window
        let count: u32 = conn.incr(&key, 1u32).await
            .map_err(|e| RateLimitError::Redis(e.to_string()))?;

        // Set expiry on first increment
        if count == 1 {
            let _: () = conn.expire(&key, self.window_seconds as i64).await
                .map_err(|e| RateLimitError::Redis(e.to_string()))?;
        }

        // Get TTL for retry-after
        let ttl: i64 = conn.ttl(&key).await
            .map_err(|e| RateLimitError::Redis(e.to_string()))?;

        if count > self.max_messages {
            Ok(RateLimitResult::limited(
                0,
                Duration::from_secs(ttl.max(1) as u64),
            ))
        } else {
            Ok(RateLimitResult::allowed(self.max_messages - count))
        }
    }

    /// Check if user can send to a specific conversation
    pub async fn check_conversation_rate(
        &self,
        user_id: Uuid,
        conversation_id: Uuid,
    ) -> Result<RateLimitResult, RateLimitError> {
        // Check user rate first
        let user_result = self.check_user_rate(user_id).await?;
        if !user_result.allowed {
            return Ok(user_result);
        }

        // Could add conversation-specific limits here if needed
        Ok(user_result)
    }

    /// Record a message send (decrement quota)
    pub async fn record_message(
        &self,
        user_id: Uuid,
        _conversation_id: Uuid,
    ) -> Result<(), RateLimitError> {
        // The check_user_rate already increments the counter
        // This method is here for explicit recording if check was skipped
        let _ = self.check_user_rate(user_id).await?;
        Ok(())
    }

    /// Get current usage for a user
    pub async fn get_usage(&self, user_id: Uuid) -> Result<u32, RateLimitError> {
        let redis = match &self.redis {
            Some(r) => r,
            None => return Ok(0),
        };

        let mut conn = redis.get_connection().await
            .map_err(|e| RateLimitError::Redis(e.to_string()))?;

        let key = self.user_key(user_id);
        let count: u32 = conn.get(&key).await.unwrap_or(0);

        Ok(count)
    }

    /// Reset rate limit for a user (admin function)
    pub async fn reset_user(&self, user_id: Uuid) -> Result<(), RateLimitError> {
        let redis = match &self.redis {
            Some(r) => r,
            None => return Ok(()),
        };

        let mut conn = redis.get_connection().await
            .map_err(|e| RateLimitError::Redis(e.to_string()))?;

        let key = self.user_key(user_id);
        let _: () = conn.del(&key).await
            .map_err(|e| RateLimitError::Redis(e.to_string()))?;

        Ok(())
    }
}

/// Result of a rate limit check
#[derive(Debug, Clone)]
pub struct RateLimitResult {
    pub allowed: bool,
    pub remaining: u32,
    pub retry_after: Option<Duration>,
}

impl RateLimitResult {
    pub fn allowed(remaining: u32) -> Self {
        Self {
            allowed: true,
            remaining,
            retry_after: None,
        }
    }

    pub fn limited(remaining: u32, retry_after: Duration) -> Self {
        Self {
            allowed: false,
            remaining,
            retry_after: Some(retry_after),
        }
    }
}

#[derive(Debug, thiserror::Error)]
pub enum RateLimitError {
    #[error("Redis error: {0}")]
    Redis(String),
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_rate_limit_result() {
        let allowed = RateLimitResult::allowed(10);
        assert!(allowed.allowed);
        assert_eq!(allowed.remaining, 10);
        assert!(allowed.retry_after.is_none());

        let limited = RateLimitResult::limited(0, Duration::from_secs(60));
        assert!(!limited.allowed);
        assert_eq!(limited.remaining, 0);
        assert_eq!(limited.retry_after, Some(Duration::from_secs(60)));
    }
}
