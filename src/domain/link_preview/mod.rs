//! Link Preview domain module
//!
//! Provides URL extraction and Open Graph metadata fetching for message content.
//! Features:
//! - Extract URLs from message content
//! - Fetch Open Graph metadata asynchronously
//! - Cache previews in Redis (24-hour TTL)
//! - Store in PostgreSQL for persistence
//! - Real-time WebSocket updates when previews are ready

mod error;
mod parser;
mod service;
mod types;

pub use error::LinkPreviewError;
pub use parser::{extract_urls, url_hash};
pub use service::LinkPreviewService;
pub use types::{LinkPreview, OpenGraphData, PreviewStatus};
