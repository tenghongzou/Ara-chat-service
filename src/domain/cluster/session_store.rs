//! Session store - distributed session tracking

use std::sync::Arc;

use async_trait::async_trait;
use redis::AsyncCommands;
use uuid::Uuid;

use crate::redis::RedisPool;

/// Trait for distributed session storage
#[async_trait]
pub trait SessionStore: Send + Sync {
    /// Register a user session on this server
    async fn register_session(&self, user_id: Uuid) -> Result<(), SessionStoreError>;

    /// Unregister a user session from this server
    async fn unregister_session(&self, user_id: Uuid) -> Result<(), SessionStoreError>;

    /// Find which servers have a user connected
    async fn find_user_servers(&self, user_id: &Uuid) -> Result<Vec<String>, SessionStoreError>;

    /// Refresh session TTL
    async fn refresh_sessions(&self) -> Result<usize, SessionStoreError>;

    /// Get total user count across cluster
    async fn cluster_user_count(&self) -> Result<usize, SessionStoreError>;

    /// Get server ID
    fn server_id(&self) -> &str;
}

/// Redis-backed distributed session store
pub struct RedisSessionStore {
    server_id: String,
    pool: Arc<RedisPool>,
    prefix: String,
    ttl_seconds: i64,
}

impl RedisSessionStore {
    pub fn new(server_id: String, pool: Arc<RedisPool>) -> Self {
        Self {
            server_id,
            pool,
            prefix: "chat:cluster:sessions".to_string(),
            ttl_seconds: 120,
        }
    }

    fn user_servers_key(&self, user_id: &Uuid) -> String {
        format!("{}:user:{}", self.prefix, user_id)
    }

    fn server_users_key(&self) -> String {
        format!("{}:server:{}", self.prefix, self.server_id)
    }
}

#[async_trait]
impl SessionStore for RedisSessionStore {
    async fn register_session(&self, user_id: Uuid) -> Result<(), SessionStoreError> {
        let mut conn = self.pool.get_connection().await
            .map_err(|e| SessionStoreError::Redis(e.to_string()))?;

        let user_key = self.user_servers_key(&user_id);
        let server_key = self.server_users_key();

        // Add server to user's server set and user to server's user set
        let _: () = redis::pipe()
            .atomic()
            .sadd(&user_key, &self.server_id)
            .expire(&user_key, self.ttl_seconds)
            .sadd(&server_key, user_id.to_string())
            .expire(&server_key, self.ttl_seconds)
            .query_async(&mut conn)
            .await
            .map_err(|e| SessionStoreError::Redis(e.to_string()))?;

        tracing::debug!(
            user_id = %user_id,
            server_id = %self.server_id,
            "Registered session in cluster"
        );

        Ok(())
    }

    async fn unregister_session(&self, user_id: Uuid) -> Result<(), SessionStoreError> {
        let mut conn = self.pool.get_connection().await
            .map_err(|e| SessionStoreError::Redis(e.to_string()))?;

        let user_key = self.user_servers_key(&user_id);
        let server_key = self.server_users_key();

        // Remove server from user's set and user from server's set
        let _: () = redis::pipe()
            .atomic()
            .srem(&user_key, &self.server_id)
            .srem(&server_key, user_id.to_string())
            .query_async(&mut conn)
            .await
            .map_err(|e| SessionStoreError::Redis(e.to_string()))?;

        tracing::debug!(
            user_id = %user_id,
            server_id = %self.server_id,
            "Unregistered session from cluster"
        );

        Ok(())
    }

    async fn find_user_servers(&self, user_id: &Uuid) -> Result<Vec<String>, SessionStoreError> {
        let mut conn = self.pool.get_connection().await
            .map_err(|e| SessionStoreError::Redis(e.to_string()))?;

        let key = self.user_servers_key(user_id);
        let servers: Vec<String> = conn.smembers(&key).await
            .map_err(|e| SessionStoreError::Redis(e.to_string()))?;

        Ok(servers)
    }

    async fn refresh_sessions(&self) -> Result<usize, SessionStoreError> {
        let mut conn = self.pool.get_connection().await
            .map_err(|e| SessionStoreError::Redis(e.to_string()))?;

        let server_key = self.server_users_key();

        // Refresh TTL on server's user set
        let _: () = conn.expire(&server_key, self.ttl_seconds)
            .await
            .map_err(|e| SessionStoreError::Redis(e.to_string()))?;

        // Get all users on this server and refresh their keys
        let users: Vec<String> = conn.smembers(&server_key).await
            .map_err(|e| SessionStoreError::Redis(e.to_string()))?;

        for user_str in &users {
            if let Ok(user_id) = Uuid::parse_str(user_str) {
                let user_key = self.user_servers_key(&user_id);
                let _: () = conn.expire(&user_key, self.ttl_seconds)
                    .await
                    .map_err(|e| SessionStoreError::Redis(e.to_string()))?;
            }
        }

        Ok(users.len())
    }

    async fn cluster_user_count(&self) -> Result<usize, SessionStoreError> {
        let mut conn = self.pool.get_connection().await
            .map_err(|e| SessionStoreError::Redis(e.to_string()))?;

        // Scan for all server keys and sum their members
        let pattern = format!("{}:server:*", self.prefix);
        let keys: Vec<String> = redis::cmd("KEYS")
            .arg(&pattern)
            .query_async(&mut conn)
            .await
            .map_err(|e| SessionStoreError::Redis(e.to_string()))?;

        let mut total = 0;
        for key in keys {
            let count: usize = conn.scard(&key).await
                .map_err(|e| SessionStoreError::Redis(e.to_string()))?;
            total += count;
        }

        Ok(total)
    }

    fn server_id(&self) -> &str {
        &self.server_id
    }
}

/// In-memory session store (for single-server mode)
pub struct MemorySessionStore {
    server_id: String,
}

impl MemorySessionStore {
    pub fn new(server_id: String) -> Self {
        Self { server_id }
    }
}

#[async_trait]
impl SessionStore for MemorySessionStore {
    async fn register_session(&self, _user_id: Uuid) -> Result<(), SessionStoreError> {
        Ok(())
    }

    async fn unregister_session(&self, _user_id: Uuid) -> Result<(), SessionStoreError> {
        Ok(())
    }

    async fn find_user_servers(&self, _user_id: &Uuid) -> Result<Vec<String>, SessionStoreError> {
        // In single-server mode, always return this server
        Ok(vec![self.server_id.clone()])
    }

    async fn refresh_sessions(&self) -> Result<usize, SessionStoreError> {
        Ok(0)
    }

    async fn cluster_user_count(&self) -> Result<usize, SessionStoreError> {
        Ok(0)
    }

    fn server_id(&self) -> &str {
        &self.server_id
    }
}

#[derive(Debug, thiserror::Error)]
pub enum SessionStoreError {
    #[error("Redis error: {0}")]
    Redis(String),

    #[error("Serialization error: {0}")]
    Serialization(String),
}

#[cfg(test)]
mod tests {
    use super::*;

    // ==================== MemorySessionStore Tests ====================

    #[test]
    fn test_memory_store_new() {
        let store = MemorySessionStore::new("server-1".to_string());
        assert_eq!(store.server_id(), "server-1");
    }

    #[tokio::test]
    async fn test_memory_store_register_session() {
        let store = MemorySessionStore::new("server-1".to_string());
        let user_id = Uuid::new_v4();

        let result = store.register_session(user_id).await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_memory_store_unregister_session() {
        let store = MemorySessionStore::new("server-1".to_string());
        let user_id = Uuid::new_v4();

        let result = store.unregister_session(user_id).await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_memory_store_find_user_servers() {
        let store = MemorySessionStore::new("server-1".to_string());
        let user_id = Uuid::new_v4();

        let servers = store.find_user_servers(&user_id).await.unwrap();
        // In single-server mode, always returns this server
        assert_eq!(servers.len(), 1);
        assert_eq!(servers[0], "server-1");
    }

    #[tokio::test]
    async fn test_memory_store_refresh_sessions() {
        let store = MemorySessionStore::new("server-1".to_string());

        let count = store.refresh_sessions().await.unwrap();
        assert_eq!(count, 0);
    }

    #[tokio::test]
    async fn test_memory_store_cluster_user_count() {
        let store = MemorySessionStore::new("server-1".to_string());

        let count = store.cluster_user_count().await.unwrap();
        assert_eq!(count, 0);
    }

    #[test]
    fn test_memory_store_server_id() {
        let store = MemorySessionStore::new("test-server".to_string());
        assert_eq!(store.server_id(), "test-server");
    }

    // ==================== RedisSessionStore Key Format Tests ====================

    // Note: RedisSessionStore requires a real Redis connection for full testing
    // These tests verify the key format methods

    #[test]
    fn test_session_store_error_redis_display() {
        let err = SessionStoreError::Redis("connection failed".to_string());
        assert_eq!(err.to_string(), "Redis error: connection failed");
    }

    #[test]
    fn test_session_store_error_serialization_display() {
        let err = SessionStoreError::Serialization("invalid format".to_string());
        assert_eq!(err.to_string(), "Serialization error: invalid format");
    }

    #[test]
    fn test_session_store_error_debug() {
        let err = SessionStoreError::Redis("test".to_string());
        let debug = format!("{:?}", err);
        assert!(debug.contains("Redis"));
        assert!(debug.contains("test"));
    }
}
