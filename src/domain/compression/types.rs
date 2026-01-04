//! Compression types and errors

use thiserror::Error;

/// Compression algorithm
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Algorithm {
    #[default]
    Zstd,
    None,
}

impl Algorithm {
    /// Get the algorithm identifier for the flags byte
    pub fn flag_bits(&self) -> u8 {
        match self {
            Algorithm::Zstd => 0x00,
            Algorithm::None => 0x00,
        }
    }

    /// Parse algorithm from flag bits
    pub fn from_flag_bits(bits: u8) -> Option<Self> {
        match bits & 0x06 {
            0x00 => Some(Algorithm::Zstd),
            _ => None,
        }
    }
}

/// Compression flags byte layout:
/// - bit 0: compressed (1) or raw (0)
/// - bit 1-2: algorithm (00=zstd)
/// - bit 3-7: reserved
pub struct CompressionFlags;

impl CompressionFlags {
    /// Flag indicating data is compressed
    pub const COMPRESSED: u8 = 0x01;

    /// Create flags for compressed data
    pub fn compressed(algorithm: Algorithm) -> u8 {
        Self::COMPRESSED | algorithm.flag_bits()
    }

    /// Create flags for uncompressed data
    pub fn uncompressed() -> u8 {
        0x00
    }

    /// Check if flags indicate compressed data
    pub fn is_compressed(flags: u8) -> bool {
        flags & Self::COMPRESSED != 0
    }
}

/// Compression error types
#[derive(Debug, Error)]
pub enum CompressionError {
    #[error("Empty input data")]
    EmptyInput,

    #[error("Compression failed: {0}")]
    CompressionFailed(String),

    #[error("Decompression failed: {0}")]
    DecompressionFailed(String),

    #[error("Decompressed data exceeds maximum size limit")]
    DecompressedTooLarge,

    #[error("Unsupported compression algorithm")]
    UnsupportedAlgorithm,

    #[error("Invalid compressed data format")]
    InvalidFormat,
}

impl From<std::io::Error> for CompressionError {
    fn from(e: std::io::Error) -> Self {
        CompressionError::DecompressionFailed(e.to_string())
    }
}

/// Client compression capabilities
#[derive(Debug, Clone)]
pub struct ClientCapabilities {
    /// Supported compression algorithms
    pub compression: Vec<Algorithm>,
    /// Maximum message size client can handle
    pub max_message_size: Option<usize>,
}

impl Default for ClientCapabilities {
    fn default() -> Self {
        Self {
            compression: vec![],
            max_message_size: None,
        }
    }
}

/// Server compression acknowledgment
#[derive(Debug, Clone)]
pub struct ServerCapabilities {
    /// Selected compression algorithm (None if compression disabled)
    pub compression: Option<Algorithm>,
    /// Compression threshold in bytes
    pub threshold: usize,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_compression_flags_compressed() {
        let flags = CompressionFlags::compressed(Algorithm::Zstd);
        assert!(CompressionFlags::is_compressed(flags));
        assert_eq!(Algorithm::from_flag_bits(flags), Some(Algorithm::Zstd));
    }

    #[test]
    fn test_compression_flags_uncompressed() {
        let flags = CompressionFlags::uncompressed();
        assert!(!CompressionFlags::is_compressed(flags));
    }

    #[test]
    fn test_algorithm_flag_bits() {
        assert_eq!(Algorithm::Zstd.flag_bits(), 0x00);
    }
}
