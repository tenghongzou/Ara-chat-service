//! Connection types

use std::sync::Arc;

use tokio::sync::mpsc;
use uuid::Uuid;

use crate::message::OutboundMessage;

/// Configuration limits for connections
#[derive(Debug, Clone)]
pub struct ConnectionLimits {
    /// Maximum total connections
    pub max_connections: usize,
    /// Maximum connections per user
    pub max_connections_per_user: usize,
}

impl Default for ConnectionLimits {
    fn default() -> Self {
        Self {
            max_connections: 100_000,
            max_connections_per_user: 5,
        }
    }
}

/// Information about a connection
#[derive(Debug, Clone)]
pub struct ConnectionInfo {
    pub connection_id: Uuid,
    pub user_id: Uuid,
    pub tenant_id: String,
    pub connected_at: i64,
    pub last_active_at: i64,
}

/// A WebSocket connection
pub struct Connection {
    pub id: Uuid,
    pub user_id: Uuid,
    pub tenant_id: String,
    pub sender: mpsc::UnboundedSender<OutboundMessage>,
    pub connected_at: i64,
}

impl Connection {
    pub fn new(
        id: Uuid,
        user_id: Uuid,
        tenant_id: String,
        sender: mpsc::UnboundedSender<OutboundMessage>,
    ) -> Self {
        Self {
            id,
            user_id,
            tenant_id,
            sender,
            connected_at: chrono::Utc::now().timestamp_millis(),
        }
    }

    /// Send a message to this connection
    pub fn send(&self, message: OutboundMessage) -> Result<(), mpsc::error::SendError<OutboundMessage>> {
        self.sender.send(message)
    }

    /// Check if the connection is still alive
    pub fn is_alive(&self) -> bool {
        !self.sender.is_closed()
    }
}
