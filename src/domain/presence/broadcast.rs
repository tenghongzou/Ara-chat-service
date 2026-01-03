//! Presence broadcaster - broadcasts presence changes to interested users

use std::sync::Arc;

use uuid::Uuid;

use crate::connection::ConnectionManager;
use crate::cluster::ClusterRouter;
use crate::message::{OutboundMessage, PresenceStatus, ServerMessage};
use super::tracker::PresenceTracker;

/// Broadcasts presence changes to relevant users
pub struct PresenceBroadcaster {
    connection_manager: Arc<ConnectionManager>,
    cluster_router: Arc<ClusterRouter>,
    presence_tracker: Arc<PresenceTracker>,
}

impl PresenceBroadcaster {
    pub fn new(
        connection_manager: Arc<ConnectionManager>,
        cluster_router: Arc<ClusterRouter>,
        presence_tracker: Arc<PresenceTracker>,
    ) -> Self {
        Self {
            connection_manager,
            cluster_router,
            presence_tracker,
        }
    }

    /// Broadcast presence change to users who care
    pub async fn broadcast_presence_change(
        &self,
        user_id: Uuid,
        status: PresenceStatus,
        interested_users: &[Uuid],
    ) -> Result<(), BroadcastError> {
        let last_seen = if status == PresenceStatus::Offline {
            Some(chrono::Utc::now().timestamp_millis())
        } else {
            None
        };

        let message = ServerMessage::Presence {
            user_id,
            status,
            last_seen,
        };

        let outbound = OutboundMessage::preserialized(&message)
            .map_err(|e| BroadcastError::Serialization(e.to_string()))?;

        for &target_user in interested_users {
            // Don't send to self
            if target_user == user_id {
                continue;
            }

            if self.connection_manager.has_user(&target_user) {
                self.connection_manager.send_to_user(&target_user, outbound.clone()).await;
            } else {
                let _ = self.cluster_router.route_to_user(target_user, outbound.clone()).await;
            }
        }

        Ok(())
    }

    /// Get list of users who should receive presence updates for a user
    /// This uses the subscription system to find interested users
    pub async fn get_interested_users(&self, user_id: Uuid) -> Result<Vec<Uuid>, BroadcastError> {
        self.presence_tracker
            .get_subscribers(user_id)
            .await
            .map_err(|e| BroadcastError::Routing(e.to_string()))
    }

    /// Broadcast presence change to all subscribers
    pub async fn broadcast_to_subscribers(
        &self,
        user_id: Uuid,
        status: PresenceStatus,
    ) -> Result<(), BroadcastError> {
        let subscribers = self.get_interested_users(user_id).await?;

        if subscribers.is_empty() {
            return Ok(());
        }

        self.broadcast_presence_change(user_id, status, &subscribers).await
    }

    /// Send current presence status of multiple users to a single subscriber
    /// (used when subscribing to get initial state)
    pub async fn send_initial_presence(
        &self,
        subscriber_id: Uuid,
        target_user_ids: &[Uuid],
    ) -> Result<(), BroadcastError> {
        let presences = self.presence_tracker
            .get_presences(target_user_ids)
            .await
            .map_err(|e| BroadcastError::Routing(e.to_string()))?;

        for presence in presences {
            let last_seen = if presence.status == PresenceStatus::Offline {
                Some(presence.last_seen)
            } else {
                None
            };

            let message = ServerMessage::Presence {
                user_id: presence.user_id,
                status: presence.status,
                last_seen,
            };

            let outbound = OutboundMessage::preserialized(&message)
                .map_err(|e| BroadcastError::Serialization(e.to_string()))?;

            if self.connection_manager.has_user(&subscriber_id) {
                self.connection_manager.send_to_user(&subscriber_id, outbound).await;
            } else {
                let _ = self.cluster_router.route_to_user(subscriber_id, outbound).await;
            }
        }

        Ok(())
    }
}

#[derive(Debug, thiserror::Error)]
pub enum BroadcastError {
    #[error("Serialization error: {0}")]
    Serialization(String),

    #[error("Routing error: {0}")]
    Routing(String),
}
