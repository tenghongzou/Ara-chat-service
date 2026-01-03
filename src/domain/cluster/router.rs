//! Cluster router - routes messages across server instances

use std::sync::Arc;

use redis::AsyncCommands;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::connection::ConnectionManager;
use crate::message::{OfflineQueue, OutboundMessage, ServerMessage};
use crate::redis::RedisPool;
use super::session_store::SessionStore;

/// Routed message payload for pub/sub
#[derive(Debug, Clone, Serialize, Deserialize)]
struct RoutedPayload {
    user_id: Uuid,
    message_json: String,
}

/// Routes messages to users across the cluster
pub struct ClusterRouter {
    connection_manager: Arc<ConnectionManager>,
    session_store: Arc<dyn SessionStore>,
    redis_pool: Option<Arc<RedisPool>>,
    offline_queue: Option<Arc<OfflineQueue>>,
    server_id: String,
    routing_channel: String,
}

impl ClusterRouter {
    pub fn new(
        connection_manager: Arc<ConnectionManager>,
        session_store: Arc<dyn SessionStore>,
        server_id: String,
    ) -> Self {
        Self {
            connection_manager,
            session_store,
            redis_pool: None,
            offline_queue: None,
            server_id,
            routing_channel: "chat:cluster:route".to_string(),
        }
    }

    pub fn with_redis(
        connection_manager: Arc<ConnectionManager>,
        session_store: Arc<dyn SessionStore>,
        redis_pool: Arc<RedisPool>,
        server_id: String,
    ) -> Self {
        Self {
            connection_manager,
            session_store,
            redis_pool: Some(redis_pool),
            offline_queue: None,
            server_id,
            routing_channel: "chat:cluster:route".to_string(),
        }
    }

    /// Set the offline queue for storing messages to offline users
    pub fn with_offline_queue(mut self, queue: Arc<OfflineQueue>) -> Self {
        self.offline_queue = Some(queue);
        self
    }

    /// Route a message to a specific user
    pub async fn route_to_user(
        &self,
        user_id: Uuid,
        message: OutboundMessage,
    ) -> Result<(), ClusterRouterError> {
        self.route_to_user_internal(user_id, message, None).await
    }

    /// Route a message to a specific user, with optional offline queuing
    pub async fn route_to_user_with_queue(
        &self,
        user_id: Uuid,
        message: OutboundMessage,
        server_message: ServerMessage,
    ) -> Result<(), ClusterRouterError> {
        self.route_to_user_internal(user_id, message, Some(server_message)).await
    }

    /// Internal routing with optional offline queue support
    async fn route_to_user_internal(
        &self,
        user_id: Uuid,
        message: OutboundMessage,
        server_message: Option<ServerMessage>,
    ) -> Result<(), ClusterRouterError> {
        // Check if user is on this server
        if self.connection_manager.has_user(&user_id) {
            self.connection_manager.send_to_user(&user_id, message).await;
            return Ok(());
        }

        // Find which servers have this user
        let servers = self.session_store
            .find_user_servers(&user_id)
            .await
            .map_err(|e| ClusterRouterError::SessionStore(e.to_string()))?;

        if servers.is_empty() {
            // User not connected anywhere - queue for offline delivery if enabled
            if let (Some(queue), Some(msg)) = (&self.offline_queue, server_message) {
                if let Err(e) = queue.queue_message(user_id, msg).await {
                    tracing::warn!(
                        user_id = %user_id,
                        error = %e,
                        "Failed to queue message for offline user"
                    );
                } else {
                    tracing::debug!(user_id = %user_id, "Queued message for offline user");
                }
            } else {
                tracing::debug!(user_id = %user_id, "User not connected to any server");
            }
            return Ok(());
        }

        // Route through Redis pub/sub
        for server_id in servers {
            if server_id == self.server_id {
                // Deliver locally
                self.connection_manager.send_to_user(&user_id, message.clone()).await;
            } else {
                // Publish to remote server's channel
                self.publish_to_server(&server_id, user_id, &message).await?;
            }
        }

        Ok(())
    }

    /// Publish a message to a specific server via Redis pub/sub
    async fn publish_to_server(
        &self,
        target_server: &str,
        user_id: Uuid,
        message: &OutboundMessage,
    ) -> Result<(), ClusterRouterError> {
        let redis_pool = self.redis_pool.as_ref()
            .ok_or_else(|| ClusterRouterError::NoRedis)?;

        let mut conn = redis_pool.get_connection().await
            .map_err(|e| ClusterRouterError::RedisPublish(e.to_string()))?;

        let message_json = message.to_json()
            .map_err(|e| ClusterRouterError::Serialization(e.to_string()))?;

        let payload = RoutedPayload {
            user_id,
            message_json,
        };

        let payload_json = serde_json::to_string(&payload)
            .map_err(|e| ClusterRouterError::Serialization(e.to_string()))?;

        // Publish to server-specific channel
        let channel = format!("{}:{}", self.routing_channel, target_server);
        let _: () = conn.publish(&channel, &payload_json).await
            .map_err(|e| ClusterRouterError::RedisPublish(e.to_string()))?;

        tracing::debug!(
            target_server = %target_server,
            user_id = %user_id,
            "Published message to remote server"
        );

        Ok(())
    }

    /// Handle incoming routed message from another server
    pub async fn handle_routed_message(
        &self,
        payload_json: &str,
    ) -> Result<(), ClusterRouterError> {
        let payload: RoutedPayload = serde_json::from_str(payload_json)
            .map_err(|e| ClusterRouterError::Serialization(e.to_string()))?;

        let message = OutboundMessage::Serialized(Arc::from(payload.message_json));
        self.connection_manager.send_to_user(&payload.user_id, message).await;

        tracing::debug!(
            user_id = %payload.user_id,
            "Delivered routed message from remote server"
        );

        Ok(())
    }

    /// Get the routing channel for this server
    pub fn routing_channel(&self) -> String {
        format!("{}:{}", self.routing_channel, self.server_id)
    }

    /// Get the current server ID
    pub fn server_id(&self) -> &str {
        &self.server_id
    }
}

#[derive(Debug, thiserror::Error)]
pub enum ClusterRouterError {
    #[error("Session store error: {0}")]
    SessionStore(String),

    #[error("Redis not configured for cluster mode")]
    NoRedis,

    #[error("Redis publish error: {0}")]
    RedisPublish(String),

    #[error("Serialization error: {0}")]
    Serialization(String),
}
