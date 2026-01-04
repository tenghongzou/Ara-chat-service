//! Message compression module
//!
//! Provides application-level compression for WebSocket messages using zstd.
//!
//! ## Features
//!
//! - Transparent compression for messages above configurable threshold
//! - zstd algorithm for optimal compression ratio and speed
//! - Backward compatible with non-compressed clients
//! - Capability negotiation during connection handshake
//!
//! ## Message Format
//!
//! Compressed messages use a simple binary format:
//! ```text
//! +-------------------+-------------------+
//! | 1 byte flags      | payload           |
//! +-------------------+-------------------+
//! ```
//!
//! Flags byte:
//! - bit 0: compressed (1) or raw (0)
//! - bit 1-2: algorithm (00=zstd)
//! - bit 3-7: reserved

mod codec;
mod types;

pub use codec::{CompressionCodec, create_codec};
pub use types::{
    Algorithm,
    ClientCapabilities,
    CompressionError,
    CompressionFlags,
    ServerCapabilities,
};
