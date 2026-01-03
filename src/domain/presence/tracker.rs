//! Presence tracker - tracks user online status in Redis

use std::sync::Arc;

use chrono::Utc;
use redis::AsyncCommands;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::redis::RedisPool;
use crate::message::PresenceStatus;

/// User presence information stored in Redis
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PresenceInfo {
    pub user_id: Uuid,
    pub status: PresenceStatus,
    pub server_id: String,
    pub last_seen: i64,
    pub connections: u32,
}

/// Tracks user presence (online/offline status) in Redis
pub struct PresenceTracker {
    redis: Option<Arc<RedisPool>>,
    server_id: String,
    prefix: String,
    ttl_seconds: i64,
}

impl PresenceTracker {
    pub fn new(redis: Option<Arc<RedisPool>>, server_id: String) -> Self {
        Self {
            redis,
            server_id,
            prefix: "chat:presence".to_string(),
            ttl_seconds: 120, // 2 minutes
        }
    }

    fn presence_key(&self, user_id: Uuid) -> String {
        format!("{}:{}", self.prefix, user_id)
    }

    /// Update user presence
    pub async fn update_presence(
        &self,
        user_id: Uuid,
        status: PresenceStatus,
    ) -> Result<(), PresenceError> {
        let redis = match &self.redis {
            Some(r) => r,
            None => return Ok(()), // No Redis, skip presence tracking
        };

        let mut conn = redis.get_connection().await
            .map_err(|e| PresenceError::Redis(e.to_string()))?;

        let key = self.presence_key(user_id);
        let now = Utc::now().timestamp_millis();

        let status_str = match status {
            PresenceStatus::Online => "online",
            PresenceStatus::Away => "away",
            PresenceStatus::Busy => "busy",
            PresenceStatus::Offline => "offline",
        };

        // Store as hash with TTL
        let _: () = redis::pipe()
            .atomic()
            .hset(&key, "status", status_str)
            .hset(&key, "server_id", &self.server_id)
            .hset(&key, "last_seen", now)
            .expire(&key, self.ttl_seconds)
            .query_async(&mut conn)
            .await
            .map_err(|e| PresenceError::Redis(e.to_string()))?;

        tracing::debug!(
            user_id = %user_id,
            status = ?status,
            "Updated presence"
        );

        Ok(())
    }

    /// Mark user as online (called on connection)
    pub async fn mark_online(&self, user_id: Uuid) -> Result<(), PresenceError> {
        let redis = match &self.redis {
            Some(r) => r,
            None => return Ok(()),
        };

        let mut conn = redis.get_connection().await
            .map_err(|e| PresenceError::Redis(e.to_string()))?;

        let key = self.presence_key(user_id);
        let now = Utc::now().timestamp_millis();

        // Increment connection count or set to 1
        let _: () = redis::pipe()
            .atomic()
            .hset(&key, "status", "online")
            .hset(&key, "server_id", &self.server_id)
            .hset(&key, "last_seen", now)
            .hincr(&key, "connections", 1i64)
            .expire(&key, self.ttl_seconds)
            .query_async(&mut conn)
            .await
            .map_err(|e| PresenceError::Redis(e.to_string()))?;

        Ok(())
    }

    /// Mark user as offline (called on disconnect)
    pub async fn mark_offline(&self, user_id: Uuid) -> Result<bool, PresenceError> {
        let redis = match &self.redis {
            Some(r) => r,
            None => return Ok(true),
        };

        let mut conn = redis.get_connection().await
            .map_err(|e| PresenceError::Redis(e.to_string()))?;

        let key = self.presence_key(user_id);
        let now = Utc::now().timestamp_millis();

        // Decrement connection count
        let connections: i64 = conn.hincr(&key, "connections", -1i64).await
            .map_err(|e| PresenceError::Redis(e.to_string()))?;

        if connections <= 0 {
            // No more connections, mark as offline
            let _: () = redis::pipe()
                .atomic()
                .hset(&key, "status", "offline")
                .hset(&key, "last_seen", now)
                .hset(&key, "connections", 0i64)
                .expire(&key, self.ttl_seconds)
                .query_async(&mut conn)
                .await
                .map_err(|e| PresenceError::Redis(e.to_string()))?;

            return Ok(true); // User is now fully offline
        }

        Ok(false) // User still has other connections
    }

    /// Get user's current presence
    pub async fn get_presence(&self, user_id: Uuid) -> Result<Option<PresenceInfo>, PresenceError> {
        let redis = match &self.redis {
            Some(r) => r,
            None => return Ok(None),
        };

        let mut conn = redis.get_connection().await
            .map_err(|e| PresenceError::Redis(e.to_string()))?;

        let key = self.presence_key(user_id);

        let result: redis::Value = conn.hgetall(&key).await
            .map_err(|e| PresenceError::Redis(e.to_string()))?;

        // Parse the hash
        if let redis::Value::Array(items) = result {
            if items.is_empty() {
                return Ok(None);
            }

            let mut status = PresenceStatus::Offline;
            let mut server_id = String::new();
            let mut last_seen = 0i64;
            let mut connections = 0u32;

            let mut iter = items.iter();
            while let (Some(key_val), Some(value)) = (iter.next(), iter.next()) {
                if let (redis::Value::BulkString(k), redis::Value::BulkString(v)) = (key_val, value) {
                    let key_str = String::from_utf8_lossy(k);
                    let val_str = String::from_utf8_lossy(v);

                    match key_str.as_ref() {
                        "status" => {
                            status = match val_str.as_ref() {
                                "online" => PresenceStatus::Online,
                                "away" => PresenceStatus::Away,
                                "busy" => PresenceStatus::Busy,
                                _ => PresenceStatus::Offline,
                            };
                        }
                        "server_id" => {
                            server_id = val_str.to_string();
                        }
                        "last_seen" => {
                            last_seen = val_str.parse().unwrap_or(0);
                        }
                        "connections" => {
                            connections = val_str.parse().unwrap_or(0);
                        }
                        _ => {}
                    }
                }
            }

            return Ok(Some(PresenceInfo {
                user_id,
                status,
                server_id,
                last_seen,
                connections,
            }));
        }

        Ok(None)
    }

    /// Get presence for multiple users (batch)
    pub async fn get_presences(&self, user_ids: &[Uuid]) -> Result<Vec<PresenceInfo>, PresenceError> {
        if user_ids.is_empty() {
            return Ok(vec![]);
        }

        let mut results = Vec::with_capacity(user_ids.len());
        for user_id in user_ids {
            if let Some(info) = self.get_presence(*user_id).await? {
                results.push(info);
            }
        }

        Ok(results)
    }

    /// Refresh TTL for a user (called periodically)
    pub async fn refresh(&self, user_id: Uuid) -> Result<(), PresenceError> {
        let redis = match &self.redis {
            Some(r) => r,
            None => return Ok(()),
        };

        let mut conn = redis.get_connection().await
            .map_err(|e| PresenceError::Redis(e.to_string()))?;

        let key = self.presence_key(user_id);
        let _: () = conn.expire(&key, self.ttl_seconds).await
            .map_err(|e| PresenceError::Redis(e.to_string()))?;

        Ok(())
    }

    // --- Subscription Management ---

    fn subscriber_key(&self, target_user_id: Uuid) -> String {
        format!("{}:subscribers:{}", self.prefix, target_user_id)
    }

    fn subscriptions_key(&self, subscriber_id: Uuid) -> String {
        format!("{}:subscriptions:{}", self.prefix, subscriber_id)
    }

    /// Subscribe a user to presence updates of other users
    pub async fn subscribe(
        &self,
        subscriber_id: Uuid,
        target_user_ids: &[Uuid],
    ) -> Result<(), PresenceError> {
        if target_user_ids.is_empty() {
            return Ok(());
        }

        let redis = match &self.redis {
            Some(r) => r,
            None => return Ok(()),
        };

        let mut conn = redis.get_connection().await
            .map_err(|e| PresenceError::Redis(e.to_string()))?;

        let subscriber_str = subscriber_id.to_string();
        let subscription_key = self.subscriptions_key(subscriber_id);

        // Add subscriber to each target's subscriber list
        // Also add targets to subscriber's subscription list
        let mut pipe = redis::pipe();
        pipe.atomic();

        for &target_id in target_user_ids {
            // Don't subscribe to self
            if target_id == subscriber_id {
                continue;
            }

            let target_str = target_id.to_string();
            let target_key = self.subscriber_key(target_id);

            // Add subscriber to target's list
            pipe.sadd(&target_key, &subscriber_str);
            pipe.expire(&target_key, 86400); // 24 hours TTL

            // Add target to subscriber's list
            pipe.sadd(&subscription_key, &target_str);
        }

        // Set TTL on subscriber's subscription list
        pipe.expire(&subscription_key, 86400);

        let _: () = pipe.query_async(&mut conn).await
            .map_err(|e| PresenceError::Redis(e.to_string()))?;

        tracing::debug!(
            subscriber_id = %subscriber_id,
            target_count = target_user_ids.len(),
            "Added presence subscriptions"
        );

        Ok(())
    }

    /// Unsubscribe a user from presence updates
    pub async fn unsubscribe(
        &self,
        subscriber_id: Uuid,
        target_user_ids: &[Uuid],
    ) -> Result<(), PresenceError> {
        if target_user_ids.is_empty() {
            return Ok(());
        }

        let redis = match &self.redis {
            Some(r) => r,
            None => return Ok(()),
        };

        let mut conn = redis.get_connection().await
            .map_err(|e| PresenceError::Redis(e.to_string()))?;

        let subscriber_str = subscriber_id.to_string();
        let subscription_key = self.subscriptions_key(subscriber_id);

        let mut pipe = redis::pipe();
        pipe.atomic();

        for &target_id in target_user_ids {
            let target_str = target_id.to_string();
            let target_key = self.subscriber_key(target_id);

            // Remove subscriber from target's list
            pipe.srem(&target_key, &subscriber_str);
            // Remove target from subscriber's list
            pipe.srem(&subscription_key, &target_str);
        }

        let _: () = pipe.query_async(&mut conn).await
            .map_err(|e| PresenceError::Redis(e.to_string()))?;

        tracing::debug!(
            subscriber_id = %subscriber_id,
            target_count = target_user_ids.len(),
            "Removed presence subscriptions"
        );

        Ok(())
    }

    /// Clear all subscriptions for a user (called on disconnect)
    pub async fn clear_subscriptions(&self, subscriber_id: Uuid) -> Result<(), PresenceError> {
        let redis = match &self.redis {
            Some(r) => r,
            None => return Ok(()),
        };

        let mut conn = redis.get_connection().await
            .map_err(|e| PresenceError::Redis(e.to_string()))?;

        let subscription_key = self.subscriptions_key(subscriber_id);
        let subscriber_str = subscriber_id.to_string();

        // Get all subscriptions
        let targets: Vec<String> = conn.smembers(&subscription_key).await
            .unwrap_or_default();

        if targets.is_empty() {
            return Ok(());
        }

        // Remove subscriber from all target lists
        let mut pipe = redis::pipe();
        pipe.atomic();

        for target_str in &targets {
            if let Ok(target_id) = Uuid::parse_str(target_str) {
                let target_key = self.subscriber_key(target_id);
                pipe.srem(&target_key, &subscriber_str);
            }
        }

        // Delete subscription list
        pipe.del(&subscription_key);

        let _: () = pipe.query_async(&mut conn).await
            .map_err(|e| PresenceError::Redis(e.to_string()))?;

        tracing::debug!(
            subscriber_id = %subscriber_id,
            cleared_count = targets.len(),
            "Cleared all presence subscriptions"
        );

        Ok(())
    }

    /// Get all users subscribed to a user's presence
    pub async fn get_subscribers(&self, user_id: Uuid) -> Result<Vec<Uuid>, PresenceError> {
        let redis = match &self.redis {
            Some(r) => r,
            None => return Ok(vec![]),
        };

        let mut conn = redis.get_connection().await
            .map_err(|e| PresenceError::Redis(e.to_string()))?;

        let key = self.subscriber_key(user_id);

        let members: Vec<String> = conn.smembers(&key).await
            .unwrap_or_default();

        let subscribers: Vec<Uuid> = members
            .iter()
            .filter_map(|s| Uuid::parse_str(s).ok())
            .collect();

        Ok(subscribers)
    }
}

#[derive(Debug, thiserror::Error)]
pub enum PresenceError {
    #[error("Redis error: {0}")]
    Redis(String),
}
