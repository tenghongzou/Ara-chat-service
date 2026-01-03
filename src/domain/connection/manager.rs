//! Connection manager - tracks active WebSocket connections

use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;

use dashmap::DashMap;
use smallvec::SmallVec;
use uuid::Uuid;

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
