//! Connection manager - tracks active WebSocket connections

use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;

use dashmap::DashMap;
use smallvec::SmallVec;
use uuid::Uuid;

use super::subscription::ConnectionSubscriptions;
use super::types::{Connection, ConnectionInfo, ConnectionLimits};
use crate::message::OutboundMessage;

/// Manages active WebSocket connections using lock-free data structures
pub struct ConnectionManager {
    /// All connections indexed by connection ID
    connections: DashMap<Uuid, Arc<Connection>>,
    /// User ID -> connection IDs mapping (most users have 1-2 connections)
    user_connections: DashMap<Uuid, SmallVec<[Uuid; 2]>>,
    /// Connection limits
    limits: ConnectionLimits,
    /// Total connection count
    total_connections: AtomicUsize,
}

impl ConnectionManager {
    pub fn new() -> Self {
        Self::with_limits(ConnectionLimits::default())
    }

    pub fn with_limits(limits: ConnectionLimits) -> Self {
        Self {
            connections: DashMap::new(),
            user_connections: DashMap::new(),
            limits,
            total_connections: AtomicUsize::new(0),
        }
    }

    /// Register a new connection
    pub fn register(&self, connection: Connection) -> Result<(), ConnectionError> {
        let user_id = connection.user_id;
        let connection_id = connection.id;

        // Check total limit
        if self.total_connections.load(Ordering::Relaxed) >= self.limits.max_connections {
            return Err(ConnectionError::TotalLimitExceeded);
        }

        // Check per-user limit
        if let Some(user_conns) = self.user_connections.get(&user_id) {
            if user_conns.len() >= self.limits.max_connections_per_user {
                return Err(ConnectionError::UserLimitExceeded);
            }
        }

        // Store connection
        let connection = Arc::new(connection);
        self.connections.insert(connection_id, connection);

        // Update user mapping
        self.user_connections
            .entry(user_id)
            .or_default()
            .push(connection_id);

        self.total_connections.fetch_add(1, Ordering::Relaxed);

        tracing::debug!(
            connection_id = %connection_id,
            user_id = %user_id,
            total = self.total_connections.load(Ordering::Relaxed),
            "Connection registered"
        );

        Ok(())
    }

    /// Unregister a connection
    pub fn unregister(&self, connection_id: Uuid) -> Option<Arc<Connection>> {
        let connection = self.connections.remove(&connection_id).map(|(_, c)| c)?;

        // Remove from user mapping
        if let Some(mut user_conns) = self.user_connections.get_mut(&connection.user_id) {
            user_conns.retain(|id| *id != connection_id);
            if user_conns.is_empty() {
                drop(user_conns);
                self.user_connections.remove(&connection.user_id);
            }
        }

        self.total_connections.fetch_sub(1, Ordering::Relaxed);

        tracing::debug!(
            connection_id = %connection_id,
            user_id = %connection.user_id,
            total = self.total_connections.load(Ordering::Relaxed),
            "Connection unregistered"
        );

        Some(connection)
    }

    /// Check if a user has any active connections
    pub fn has_user(&self, user_id: &Uuid) -> bool {
        self.user_connections.contains_key(user_id)
    }

    /// Get connection count for a user
    pub fn user_connection_count(&self, user_id: &Uuid) -> usize {
        self.user_connections
            .get(user_id)
            .map(|c| c.len())
            .unwrap_or(0)
    }

    /// Send message to all connections of a user
    pub async fn send_to_user(&self, user_id: &Uuid, message: OutboundMessage) {
        if let Some(connection_ids) = self.user_connections.get(user_id) {
            for conn_id in connection_ids.iter() {
                if let Some(conn) = self.connections.get(conn_id) {
                    let _ = conn.send(message.clone());
                }
            }
        }
    }

    /// Send message to a specific connection
    pub fn send_to_connection(&self, connection_id: &Uuid, message: OutboundMessage) -> bool {
        if let Some(conn) = self.connections.get(connection_id) {
            conn.send(message).is_ok()
        } else {
            false
        }
    }

    /// Broadcast message to all connections
    pub async fn broadcast(&self, message: OutboundMessage) {
        for conn in self.connections.iter() {
            let _ = conn.send(message.clone());
        }
    }

    /// Get total connection count
    pub fn total_connections(&self) -> usize {
        self.total_connections.load(Ordering::Relaxed)
    }

    /// Get unique user count
    pub fn unique_users(&self) -> usize {
        self.user_connections.len()
    }

    /// Get all connection IDs
    pub fn all_connection_ids(&self) -> Vec<Uuid> {
        self.connections.iter().map(|r| *r.key()).collect()
    }

    /// Get connection info
    pub fn get_connection_info(&self, connection_id: &Uuid) -> Option<ConnectionInfo> {
        self.connections.get(connection_id).map(|conn| ConnectionInfo {
            connection_id: conn.id,
            user_id: conn.user_id,
            tenant_id: conn.tenant_id.clone(),
            connected_at: conn.connected_at,
            last_active_at: conn.connected_at, // TODO: Track last activity
        })
    }

    /// Cleanup stale connections that are no longer alive or have timed out
    pub async fn cleanup_stale_connections(&self, _timeout: std::time::Duration) -> usize {
        let mut stale_ids = Vec::new();

        // Find connections that are no longer alive (sender closed)
        for entry in self.connections.iter() {
            if !entry.value().is_alive() {
                stale_ids.push(*entry.key());
            }
        }

        // Unregister stale connections
        let count = stale_ids.len();
        for id in stale_ids {
            self.unregister(id);
        }

        count
    }

    // ==================== Subscription-Aware Methods ====================

    /// Send message to a user with subscription filtering
    ///
    /// Only delivers to connections that are subscribed to the conversation,
    /// or in legacy mode (receive all), or if it's a system message.
    ///
    /// Returns the number of connections that received the message.
    pub async fn send_to_user_filtered(
        &self,
        user_id: &Uuid,
        conversation_id: &Uuid,
        message: OutboundMessage,
        is_system_message: bool,
    ) -> usize {
        let mut delivered = 0;

        if let Some(connection_ids) = self.user_connections.get(user_id) {
            for conn_id in connection_ids.iter() {
                if let Some(conn) = self.connections.get(conn_id) {
                    // Check subscription filter
                    if conn.subscriptions.should_receive(conversation_id, is_system_message) {
                        if conn.send(message.clone()).is_ok() {
                            delivered += 1;
                        }
                    } else {
                        // Track filtered messages (metrics)
                        crate::metrics::SUBSCRIPTION_FILTERED.inc();
                    }
                }
            }
        }

        delivered
    }

    /// Get the subscriptions for a specific connection
    pub fn get_subscriptions(&self, connection_id: &Uuid) -> Option<Arc<ConnectionSubscriptions>> {
        self.connections
            .get(connection_id)
            .map(|conn| conn.subscriptions.clone())
    }

    /// Check if any connection for a user is subscribed to a conversation
    ///
    /// Returns true if:
    /// - Any connection is in legacy mode (receives all), or
    /// - Any connection is explicitly subscribed to the conversation
    pub fn has_user_subscribed(&self, user_id: &Uuid, conversation_id: &Uuid) -> bool {
        if let Some(connection_ids) = self.user_connections.get(user_id) {
            for conn_id in connection_ids.iter() {
                if let Some(conn) = self.connections.get(conn_id) {
                    if conn.subscriptions.is_legacy_mode()
                        || conn.subscriptions.is_subscribed(conversation_id)
                    {
                        return true;
                    }
                }
            }
        }
        false
    }
}

impl Default for ConnectionManager {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Debug, thiserror::Error)]
pub enum ConnectionError {
    #[error("Maximum total connections exceeded")]
    TotalLimitExceeded,

    #[error("Maximum connections per user exceeded")]
    UserLimitExceeded,
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::sync::mpsc;

    fn create_test_connection(user_id: Uuid) -> (Connection, mpsc::UnboundedReceiver<OutboundMessage>) {
        let (tx, rx) = mpsc::unbounded_channel();
        let conn = Connection::new(Uuid::new_v4(), user_id, "test-tenant".to_string(), tx, 100);
        (conn, rx)
    }

    #[test]
    fn test_new_creates_empty_manager() {
        let manager = ConnectionManager::new();
        assert_eq!(manager.total_connections(), 0);
        assert_eq!(manager.unique_users(), 0);
    }

    #[test]
    fn test_with_limits_applies_config() {
        let limits = ConnectionLimits {
            max_connections: 50,
            max_connections_per_user: 3,
        };
        let manager = ConnectionManager::with_limits(limits);
        assert_eq!(manager.total_connections(), 0);
    }

    #[test]
    fn test_register_stores_connection() {
        let manager = ConnectionManager::new();
        let user_id = Uuid::new_v4();
        let (conn, _rx) = create_test_connection(user_id);
        let conn_id = conn.id;

        manager.register(conn).unwrap();

        assert_eq!(manager.total_connections(), 1);
        assert!(manager.get_connection_info(&conn_id).is_some());
    }

    #[test]
    fn test_register_updates_user_mapping() {
        let manager = ConnectionManager::new();
        let user_id = Uuid::new_v4();
        let (conn, _rx) = create_test_connection(user_id);

        manager.register(conn).unwrap();

        assert!(manager.has_user(&user_id));
        assert_eq!(manager.user_connection_count(&user_id), 1);
    }

    #[test]
    fn test_register_fails_total_limit() {
        let limits = ConnectionLimits {
            max_connections: 2,
            max_connections_per_user: 5,
        };
        let manager = ConnectionManager::with_limits(limits);

        // Register 2 connections
        for _ in 0..2 {
            let user_id = Uuid::new_v4();
            let (conn, _rx) = create_test_connection(user_id);
            manager.register(conn).unwrap();
        }

        // Third should fail
        let user_id = Uuid::new_v4();
        let (conn, _rx) = create_test_connection(user_id);
        let result = manager.register(conn);

        assert!(matches!(result, Err(ConnectionError::TotalLimitExceeded)));
    }

    #[test]
    fn test_register_fails_user_limit() {
        let limits = ConnectionLimits {
            max_connections: 100,
            max_connections_per_user: 2,
        };
        let manager = ConnectionManager::with_limits(limits);
        let user_id = Uuid::new_v4();

        // Register 2 connections for same user
        for _ in 0..2 {
            let (conn, _rx) = create_test_connection(user_id);
            manager.register(conn).unwrap();
        }

        // Third for same user should fail
        let (conn, _rx) = create_test_connection(user_id);
        let result = manager.register(conn);

        assert!(matches!(result, Err(ConnectionError::UserLimitExceeded)));
    }

    #[test]
    fn test_unregister_removes_connection() {
        let manager = ConnectionManager::new();
        let user_id = Uuid::new_v4();
        let (conn, _rx) = create_test_connection(user_id);
        let conn_id = conn.id;

        manager.register(conn).unwrap();
        assert_eq!(manager.total_connections(), 1);

        let removed = manager.unregister(conn_id);
        assert!(removed.is_some());
        assert_eq!(manager.total_connections(), 0);
        assert!(manager.get_connection_info(&conn_id).is_none());
    }

    #[test]
    fn test_unregister_cleans_user_mapping() {
        let manager = ConnectionManager::new();
        let user_id = Uuid::new_v4();
        let (conn, _rx) = create_test_connection(user_id);
        let conn_id = conn.id;

        manager.register(conn).unwrap();
        assert!(manager.has_user(&user_id));

        manager.unregister(conn_id);
        assert!(!manager.has_user(&user_id));
    }

    #[test]
    fn test_unregister_nonexistent() {
        let manager = ConnectionManager::new();
        let fake_id = Uuid::new_v4();

        let result = manager.unregister(fake_id);
        assert!(result.is_none());
    }

    #[test]
    fn test_has_user_true() {
        let manager = ConnectionManager::new();
        let user_id = Uuid::new_v4();
        let (conn, _rx) = create_test_connection(user_id);

        manager.register(conn).unwrap();

        assert!(manager.has_user(&user_id));
    }

    #[test]
    fn test_has_user_false() {
        let manager = ConnectionManager::new();
        let user_id = Uuid::new_v4();

        assert!(!manager.has_user(&user_id));
    }

    #[test]
    fn test_user_connection_count() {
        let manager = ConnectionManager::new();
        let user_id = Uuid::new_v4();

        // Initially zero
        assert_eq!(manager.user_connection_count(&user_id), 0);

        // Add connections
        let (conn1, _rx1) = create_test_connection(user_id);
        let (conn2, _rx2) = create_test_connection(user_id);

        manager.register(conn1).unwrap();
        assert_eq!(manager.user_connection_count(&user_id), 1);

        manager.register(conn2).unwrap();
        assert_eq!(manager.user_connection_count(&user_id), 2);
    }

    #[test]
    fn test_total_connections() {
        let manager = ConnectionManager::new();

        // Add connections for different users
        for i in 0..5 {
            let user_id = Uuid::new_v4();
            let (conn, _rx) = create_test_connection(user_id);
            manager.register(conn).unwrap();
            assert_eq!(manager.total_connections(), i + 1);
        }
    }

    #[test]
    fn test_unique_users() {
        let manager = ConnectionManager::new();

        let user1 = Uuid::new_v4();
        let user2 = Uuid::new_v4();

        let (conn1, _rx1) = create_test_connection(user1);
        let (conn2, _rx2) = create_test_connection(user1); // Same user
        let (conn3, _rx3) = create_test_connection(user2); // Different user

        manager.register(conn1).unwrap();
        manager.register(conn2).unwrap();
        manager.register(conn3).unwrap();

        assert_eq!(manager.unique_users(), 2);
    }

    #[test]
    fn test_all_connection_ids() {
        let manager = ConnectionManager::new();

        let user_id = Uuid::new_v4();
        let (conn1, _rx1) = create_test_connection(user_id);
        let (conn2, _rx2) = create_test_connection(user_id);
        let id1 = conn1.id;
        let id2 = conn2.id;

        manager.register(conn1).unwrap();
        manager.register(conn2).unwrap();

        let ids = manager.all_connection_ids();
        assert_eq!(ids.len(), 2);
        assert!(ids.contains(&id1));
        assert!(ids.contains(&id2));
    }

    #[test]
    fn test_get_connection_info() {
        let manager = ConnectionManager::new();
        let user_id = Uuid::new_v4();
        let (conn, _rx) = create_test_connection(user_id);
        let conn_id = conn.id;

        manager.register(conn).unwrap();

        let info = manager.get_connection_info(&conn_id).unwrap();
        assert_eq!(info.connection_id, conn_id);
        assert_eq!(info.user_id, user_id);
        assert_eq!(info.tenant_id, "test-tenant");
    }

    #[test]
    fn test_default_impl() {
        let manager = ConnectionManager::default();
        assert_eq!(manager.total_connections(), 0);
    }

    #[test]
    fn test_connection_limits_default() {
        let limits = ConnectionLimits::default();
        assert_eq!(limits.max_connections, 100_000);
        assert_eq!(limits.max_connections_per_user, 5);
    }
}
