//! Connection management domain

mod manager;
mod types;

pub use manager::ConnectionManager;
pub use types::{Connection, ConnectionInfo, ConnectionLimits};
