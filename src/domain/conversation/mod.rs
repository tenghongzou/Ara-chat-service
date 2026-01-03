//! Conversation domain - manages chat conversations

mod types;
mod service;
mod direct_lookup;

pub use types::*;
pub use service::{ConversationService, ConversationError};
pub use direct_lookup::DirectMessageLookup;
