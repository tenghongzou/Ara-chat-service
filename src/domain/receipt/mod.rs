//! Read receipt domain - tracks message read status

mod tracker;
mod unread;

pub use tracker::ReadReceiptTracker;
pub use unread::UnreadCounter;
