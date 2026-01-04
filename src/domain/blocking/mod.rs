//! User blocking module
//!
//! Provides user blocking functionality for the chat service:
//! - Block/unblock users
//! - Check block status between users
//! - Filter blocked users in message routing

mod service;
mod types;

pub use service::{BlockingError, BlockingService};
pub use types::{BlockedUserInfo, UserBlock};
