//! Ara Chat Service - Real-time instant messaging with WebSocket support
//!
//! This service provides:
//! - Private (1:1) and group chat
//! - Permanent message history storage
//! - Read receipts
//! - Message recall
//! - @mentions
//! - Emoji reactions
//! - Presence tracking
//!
//! Designed for 100M DAU and 10M peak concurrent connections.

// Infrastructure layer (shared components)
pub mod infrastructure;

// Re-export infrastructure modules
pub use infrastructure::auth;
pub use infrastructure::circuit_breaker;
pub use infrastructure::config;
pub use infrastructure::metrics;
pub use infrastructure::postgres;
pub use infrastructure::ratelimit;
pub use infrastructure::redis;
pub use infrastructure::sharding;

// Domain layer (business logic)
pub mod domain;

// Re-export domain modules
pub use domain::attachment;
pub use domain::blocking;
pub use domain::cluster;
pub use domain::connection;
pub use domain::conversation;
pub use domain::gdpr;
pub use domain::link_preview;
pub use domain::markdown;
pub use domain::mention;
pub use domain::message;
pub use domain::notification;
pub use domain::presence;
pub use domain::reaction;
pub use domain::receipt;

// Application layer
pub mod api;
pub mod server;

// Supporting modules
pub mod shutdown;
pub mod tasks;
pub mod telemetry;
