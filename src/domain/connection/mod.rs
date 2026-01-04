//! Connection management domain

mod manager;
mod subscription;
mod types;

pub use manager::{ConnectionError, ConnectionManager};
pub use subscription::{ConnectionSubscriptions, SubscriptionMode, SubscriptionResult};
pub use types::{Connection, ConnectionInfo, ConnectionLimits};
