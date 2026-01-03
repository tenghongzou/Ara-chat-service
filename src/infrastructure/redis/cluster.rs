//! Redis Cluster support for billion-scale distributed caching
//!
//! This module provides Redis Cluster connectivity with automatic slot-based
//! routing, failover support, and shard-aware key prefixing.

use std::sync::Arc;
use std::time::Duration;

use uuid::Uuid;

use crate::sharding::ShardId;

/// Redis Cluster configuration
#[derive(Debug, Clone)]
pub struct RedisClusterConfig {
    /// Cluster node URLs (initial nodes for discovery)
    pub nodes: Vec<String>,
    /// Connection timeout
    pub connection_timeout: Duration,
    /// Response timeout
    pub response_timeout: Duration,
    /// Number of retries for failed commands
    pub retries: u32,
    /// Whether to route read commands to replicas
    pub read_from_replicas: bool,
    /// Key prefix for namespacing
    pub key_prefix: String,
}

impl Default for RedisClusterConfig {
    fn default() -> Self {
        Self {
            nodes: vec!["redis://localhost:7000".to_string()],
            connection_timeout: Duration::from_secs(5),
            response_timeout: Duration::from_secs(2),
            retries: 3,
            read_from_replicas: true,
            key_prefix: "chat".to_string(),
        }
    }
}

/// Redis Cluster connection pool using the standard redis crate
/// with cluster-aware key routing via hash tags
pub struct RedisClusterPool {
    pools: Vec<crate::redis::RedisPool>,
    config: RedisClusterConfig,
}

impl RedisClusterPool {
    /// Create a new Redis Cluster pool
    /// For now, this creates connections to multiple nodes
    pub async fn new(config: RedisClusterConfig) -> Result<Self, RedisClusterError> {
        let mut pools = Vec::new();

        // Create a pool for each node
        for node_url in &config.nodes {
            let redis_config = crate::config::RedisSettings {
                url: node_url.clone(),
                pool_size: 10,
                cluster_enabled: true,
                cluster_nodes: vec![],
            };

            match crate::redis::RedisPool::new(&redis_config) {
                Ok(pool) => pools.push(pool),
                Err(e) => {
                    tracing::warn!(node = %node_url, error = %e, "Failed to connect to cluster node");
                }
            }
        }

        if pools.is_empty() {
            return Err(RedisClusterError::Connection("No cluster nodes available".to_string()));
        }

        tracing::info!(
            nodes = config.nodes.len(),
            connected = pools.len(),
            "Redis Cluster pool created"
        );

        Ok(Self { pools, config })
    }

    /// Get a connection to a specific slot's node
    pub async fn get_connection(&self, slot: u16) -> Result<redis::aio::MultiplexedConnection, RedisClusterError> {
        // Simple round-robin for now (real implementation would use slot mapping)
        let pool_idx = (slot as usize) % self.pools.len();
        self.pools[pool_idx].get_connection().await
            .map_err(|e| RedisClusterError::Connection(e.to_string()))
    }

    /// Get any available connection
    pub async fn get_any_connection(&self) -> Result<redis::aio::MultiplexedConnection, RedisClusterError> {
        self.pools[0].get_connection().await
            .map_err(|e| RedisClusterError::Connection(e.to_string()))
    }

    /// Get the key prefix
    pub fn key_prefix(&self) -> &str {
        &self.config.key_prefix
    }

    /// Build a key with prefix and shard hint
    /// Uses Redis Cluster hash tags to ensure related keys go to the same slot
    pub fn shard_key(&self, shard: ShardId, key: &str) -> String {
        // Use hash tag {shard-XXXX} to route to consistent slot
        format!("{}:{{{}}}{}", self.config.key_prefix, shard, key)
    }

    /// Build a user-scoped key with shard routing
    pub fn user_key(&self, user_id: Uuid, suffix: &str) -> String {
        // Use hash tag {user:UUID} for user-scoped operations
        format!("{}:{{user:{}}}{}", self.config.key_prefix, user_id, suffix)
    }

    /// Build a conversation-scoped key with shard routing
    pub fn conversation_key(&self, conversation_id: Uuid, suffix: &str) -> String {
        format!("{}:{{conv:{}}}{}", self.config.key_prefix, conversation_id, suffix)
    }

    /// Calculate the Redis Cluster slot for a key
    pub fn slot_for_key(key: &str) -> u16 {
        // Extract hash tag if present
        let hash_key = if let Some(start) = key.find('{') {
            if let Some(end) = key[start..].find('}') {
                &key[start + 1..start + end]
            } else {
                key
            }
        } else {
            key
        };

        // CRC16 hash mod 16384
        crc16::State::<crc16::XMODEM>::calculate(hash_key.as_bytes()) % 16384
    }

    /// Check cluster health
    pub async fn health_check(&self) -> ClusterHealth {
        let mut healthy_nodes = 0;

        for pool in &self.pools {
            if pool.is_healthy().await {
                healthy_nodes += 1;
            }
        }

        ClusterHealth {
            state: if healthy_nodes == self.pools.len() {
                ClusterState::Ok
            } else if healthy_nodes > 0 {
                ClusterState::Degraded
            } else {
                ClusterState::Fail
            },
            total_nodes: self.pools.len() as u32,
            healthy_nodes,
            slots_covered: 16384, // Assume full coverage if any node is up
        }
    }
}

/// Shard-aware Redis operations
pub struct ShardedRedisOps {
    pool: Arc<RedisClusterPool>,
}

impl ShardedRedisOps {
    pub fn new(pool: Arc<RedisClusterPool>) -> Self {
        Self { pool }
    }

    /// Set a value with shard routing
    pub async fn set(&self, shard: ShardId, key: &str, value: &str, ttl: Option<Duration>) -> Result<(), RedisClusterError> {
        use redis::AsyncCommands;

        let full_key = self.pool.shard_key(shard, key);
        let slot = RedisClusterPool::slot_for_key(&full_key);
        let mut conn = self.pool.get_connection(slot).await?;

        if let Some(ttl) = ttl {
            conn.set_ex::<_, _, ()>(&full_key, value, ttl.as_secs())
                .await
                .map_err(|e| RedisClusterError::Command(e.to_string()))?;
        } else {
            conn.set::<_, _, ()>(&full_key, value)
                .await
                .map_err(|e| RedisClusterError::Command(e.to_string()))?;
        }

        Ok(())
    }

    /// Get a value with shard routing
    pub async fn get(&self, shard: ShardId, key: &str) -> Result<Option<String>, RedisClusterError> {
        use redis::AsyncCommands;

        let full_key = self.pool.shard_key(shard, key);
        let slot = RedisClusterPool::slot_for_key(&full_key);
        let mut conn = self.pool.get_connection(slot).await?;

        let value: Option<String> = conn.get(&full_key)
            .await
            .map_err(|e| RedisClusterError::Command(e.to_string()))?;

        Ok(value)
    }

    /// Delete a key with shard routing
    pub async fn del(&self, shard: ShardId, key: &str) -> Result<bool, RedisClusterError> {
        use redis::AsyncCommands;

        let full_key = self.pool.shard_key(shard, key);
        let slot = RedisClusterPool::slot_for_key(&full_key);
        let mut conn = self.pool.get_connection(slot).await?;

        let deleted: i64 = conn.del(&full_key)
            .await
            .map_err(|e| RedisClusterError::Command(e.to_string()))?;

        Ok(deleted > 0)
    }

    /// Increment a counter with shard routing
    pub async fn incr(&self, shard: ShardId, key: &str) -> Result<i64, RedisClusterError> {
        use redis::AsyncCommands;

        let full_key = self.pool.shard_key(shard, key);
        let slot = RedisClusterPool::slot_for_key(&full_key);
        let mut conn = self.pool.get_connection(slot).await?;

        let value: i64 = conn.incr(&full_key, 1i64)
            .await
            .map_err(|e| RedisClusterError::Command(e.to_string()))?;

        Ok(value)
    }

    /// Publish to a channel (cluster-wide)
    pub async fn publish(&self, channel: &str, message: &str) -> Result<u64, RedisClusterError> {
        use redis::AsyncCommands;

        let mut conn = self.pool.get_any_connection().await?;

        let receivers: u64 = conn.publish(channel, message)
            .await
            .map_err(|e| RedisClusterError::Command(e.to_string()))?;

        Ok(receivers)
    }
}

/// User-scoped Redis operations
pub struct UserRedisOps {
    pool: Arc<RedisClusterPool>,
}

impl UserRedisOps {
    pub fn new(pool: Arc<RedisClusterPool>) -> Self {
        Self { pool }
    }

    /// Set user presence
    pub async fn set_presence(&self, user_id: Uuid, status: &str, server_id: &str) -> Result<(), RedisClusterError> {
        let key = self.pool.user_key(user_id, ":presence");
        let slot = RedisClusterPool::slot_for_key(&key);
        let mut conn = self.pool.get_connection(slot).await?;

        let now = chrono::Utc::now().timestamp_millis();

        let _: () = redis::pipe()
            .atomic()
            .hset(&key, "status", status)
            .hset(&key, "server_id", server_id)
            .hset(&key, "last_seen", now)
            .expire(&key, 120) // 2 minute TTL
            .query_async(&mut conn)
            .await
            .map_err(|e| RedisClusterError::Command(e.to_string()))?;

        Ok(())
    }

    /// Get user presence
    pub async fn get_presence(&self, user_id: Uuid) -> Result<Option<UserPresence>, RedisClusterError> {
        use redis::AsyncCommands;

        let key = self.pool.user_key(user_id, ":presence");
        let slot = RedisClusterPool::slot_for_key(&key);
        let mut conn = self.pool.get_connection(slot).await?;

        let result: redis::Value = conn.hgetall(&key)
            .await
            .map_err(|e| RedisClusterError::Command(e.to_string()))?;

        if let redis::Value::Array(items) = result {
            if items.is_empty() {
                return Ok(None);
            }

            let mut status = String::new();
            let mut server_id = String::new();
            let mut last_seen = 0i64;

            let mut iter = items.iter();
            while let (Some(key_val), Some(value)) = (iter.next(), iter.next()) {
                if let (redis::Value::BulkString(k), redis::Value::BulkString(v)) = (key_val, value) {
                    let key_str = String::from_utf8_lossy(k);
                    let val_str = String::from_utf8_lossy(v);

                    match key_str.as_ref() {
                        "status" => status = val_str.to_string(),
                        "server_id" => server_id = val_str.to_string(),
                        "last_seen" => last_seen = val_str.parse().unwrap_or(0),
                        _ => {}
                    }
                }
            }

            return Ok(Some(UserPresence {
                user_id,
                status,
                server_id,
                last_seen,
            }));
        }

        Ok(None)
    }

    /// Increment unread count for user in conversation
    pub async fn incr_unread(&self, user_id: Uuid, conversation_id: Uuid) -> Result<u64, RedisClusterError> {
        use redis::AsyncCommands;

        let key = self.pool.user_key(user_id, &format!(":unread:{}", conversation_id));
        let slot = RedisClusterPool::slot_for_key(&key);
        let mut conn = self.pool.get_connection(slot).await?;

        let count: u64 = conn.incr(&key, 1u64)
            .await
            .map_err(|e| RedisClusterError::Command(e.to_string()))?;

        // Set TTL if this is the first unread
        if count == 1 {
            let _: () = conn.expire(&key, 30 * 24 * 3600) // 30 days
                .await
                .map_err(|e| RedisClusterError::Command(e.to_string()))?;
        }

        Ok(count)
    }

    /// Reset unread count for user in conversation
    pub async fn reset_unread(&self, user_id: Uuid, conversation_id: Uuid) -> Result<u64, RedisClusterError> {
        use redis::AsyncCommands;

        let key = self.pool.user_key(user_id, &format!(":unread:{}", conversation_id));
        let slot = RedisClusterPool::slot_for_key(&key);
        let mut conn = self.pool.get_connection(slot).await?;

        // Get current count before deleting
        let count: u64 = conn.get(&key).await.unwrap_or(0);
        let _: () = conn.del(&key)
            .await
            .map_err(|e| RedisClusterError::Command(e.to_string()))?;

        Ok(count)
    }
}

/// User presence data
#[derive(Debug, Clone)]
pub struct UserPresence {
    pub user_id: Uuid,
    pub status: String,
    pub server_id: String,
    pub last_seen: i64,
}

/// Cluster state
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ClusterState {
    Ok,
    Degraded,
    Fail,
}

/// Cluster health information
#[derive(Debug)]
pub struct ClusterHealth {
    pub state: ClusterState,
    pub total_nodes: u32,
    pub healthy_nodes: usize,
    pub slots_covered: u32,
}

impl ClusterHealth {
    pub fn is_healthy(&self) -> bool {
        self.state == ClusterState::Ok
    }

    pub fn is_degraded(&self) -> bool {
        self.state == ClusterState::Degraded
    }
}

/// Redis Cluster errors
#[derive(Debug, thiserror::Error)]
pub enum RedisClusterError {
    #[error("Connection error: {0}")]
    Connection(String),

    #[error("Command error: {0}")]
    Command(String),

    #[error("Serialization error: {0}")]
    Serialization(String),
}
