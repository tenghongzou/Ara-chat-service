//! Message domain - core chat message handling

mod types;
mod handler;
mod offline_queue;
mod router;
mod storage;

pub use types::*;
pub use handler::MessageHandler;
pub use offline_queue::{OfflineQueue, OfflineQueueError, QueuedMessage};
pub use router::MessageRouter;
pub use storage::{MessageStorage, MessageSearchResult};
