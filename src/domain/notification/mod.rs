//! Notification service integration via Redis Pub/Sub
//!
//! This module provides integration with the Ara Notification Service,
//! enabling push notifications for chat events such as:
//! - New messages (for offline users)
//! - @mentions
//! - Emoji reactions
//!
//! Events are published to Redis channels that the Notification Service subscribes to.
//! Channel format: `notification:user:{user_id}`

mod publisher;
mod types;

pub use publisher::{NotificationPublisher, NotificationPublisherConfig};
pub use types::{
    ChatNotificationType, EventPayload, NotificationEvent, NotificationPayload,
    NotificationPriority,
};
