//! Graceful shutdown handling

use std::sync::Arc;
use std::time::{Duration, Instant};

use tokio::sync::broadcast;

use crate::connection::ConnectionManager;
use crate::message::{OutboundMessage, ServerMessage};

/// Handles graceful server shutdown
pub struct GracefulShutdown {
    connection_manager: Arc<ConnectionManager>,
    shutdown_tx: broadcast::Sender<()>,
}

impl GracefulShutdown {
    pub fn new(
        connection_manager: Arc<ConnectionManager>,
        shutdown_tx: broadcast::Sender<()>,
    ) -> Self {
        Self {
            connection_manager,
            shutdown_tx,
        }
    }

    /// Execute graceful shutdown sequence
    pub async fn execute(&self, reason: &str) -> ShutdownResult {
        let start = Instant::now();

        // Notify all connected clients
        let shutdown_msg = ServerMessage::shutdown(reason, Some(30));
        let outbound = OutboundMessage::preserialized(&shutdown_msg)
            .unwrap_or(OutboundMessage::Raw(shutdown_msg));

        self.connection_manager.broadcast(outbound).await;

        let clients_notified = self.connection_manager.total_connections();

        // Give clients a moment to disconnect gracefully
        tokio::time::sleep(Duration::from_millis(500)).await;

        // Signal all background tasks to stop
        let _ = self.shutdown_tx.send(());

        // Wait for connections to close
        tokio::time::sleep(Duration::from_secs(2)).await;

        let connections_closed = clients_notified - self.connection_manager.total_connections();

        ShutdownResult {
            clients_notified,
            connections_closed,
            duration: start.elapsed(),
        }
    }
}

/// Result of graceful shutdown
pub struct ShutdownResult {
    pub clients_notified: usize,
    pub connections_closed: usize,
    pub duration: Duration,
}
