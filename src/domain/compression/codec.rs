//! Compression codec for message compression/decompression

use std::io::{Read, Write};
use std::sync::Arc;

use crate::config::CompressionAlgorithm;
use super::types::{Algorithm, CompressionError, CompressionFlags};

/// Compression codec for encoding/decoding messages
#[derive(Debug, Clone)]
pub struct CompressionCodec {
    /// Compression algorithm to use
    algorithm: Algorithm,
    /// Compression level (1-22 for zstd)
    level: i32,
    /// Minimum message size to compress (bytes)
    threshold: usize,
    /// Maximum decompressed message size (bytes)
    max_decompressed_size: usize,
}

impl CompressionCodec {
    /// Create a new compression codec
    pub fn new(
        algorithm: Algorithm,
        level: u32,
        threshold: usize,
        max_decompressed_size: usize,
    ) -> Self {
        Self {
            algorithm,
            level: level.min(22) as i32,
            threshold,
            max_decompressed_size,
        }
    }

    /// Create codec from config settings
    pub fn from_config(
        algorithm: &CompressionAlgorithm,
        level: u32,
        threshold: usize,
        max_decompressed_size: usize,
    ) -> Self {
        let algo = match algorithm {
            CompressionAlgorithm::Zstd => Algorithm::Zstd,
            CompressionAlgorithm::None => Algorithm::None,
        };
        Self::new(algo, level, threshold, max_decompressed_size)
    }

    /// Get the compression algorithm
    pub fn algorithm(&self) -> Algorithm {
        self.algorithm
    }

    /// Get the compression threshold
    pub fn threshold(&self) -> usize {
        self.threshold
    }

    /// Compress data if it exceeds the threshold
    ///
    /// Returns a Vec<u8> with:
    /// - First byte: flags (bit 0 = compressed, bits 1-2 = algorithm)
    /// - Rest: payload (compressed or raw)
    pub fn compress(&self, data: &[u8]) -> Result<Vec<u8>, CompressionError> {
        // Don't compress small messages or if algorithm is None
        if data.len() < self.threshold || self.algorithm == Algorithm::None {
            let mut result = Vec::with_capacity(data.len() + 1);
            result.push(CompressionFlags::uncompressed());
            result.extend_from_slice(data);
            return Ok(result);
        }

        match self.algorithm {
            Algorithm::Zstd => {
                let compressed = zstd::encode_all(data, self.level)
                    .map_err(|e| CompressionError::CompressionFailed(e.to_string()))?;

                // Only use compression if it actually saves space
                if compressed.len() >= data.len() {
                    let mut result = Vec::with_capacity(data.len() + 1);
                    result.push(CompressionFlags::uncompressed());
                    result.extend_from_slice(data);
                    return Ok(result);
                }

                let mut result = Vec::with_capacity(compressed.len() + 1);
                result.push(CompressionFlags::compressed(Algorithm::Zstd));
                result.extend(compressed);
                Ok(result)
            }
            Algorithm::None => {
                let mut result = Vec::with_capacity(data.len() + 1);
                result.push(CompressionFlags::uncompressed());
                result.extend_from_slice(data);
                Ok(result)
            }
        }
    }

    /// Decompress data
    ///
    /// Expects input format:
    /// - First byte: flags
    /// - Rest: payload
    pub fn decompress(&self, data: &[u8]) -> Result<Vec<u8>, CompressionError> {
        if data.is_empty() {
            return Err(CompressionError::EmptyInput);
        }

        let flags = data[0];
        let payload = &data[1..];

        if !CompressionFlags::is_compressed(flags) {
            // Data is not compressed, return as-is
            return Ok(payload.to_vec());
        }

        let algorithm = Algorithm::from_flag_bits(flags)
            .ok_or(CompressionError::UnsupportedAlgorithm)?;

        match algorithm {
            Algorithm::Zstd => {
                // Use streaming decoder with size limit
                let mut decoder = zstd::Decoder::new(payload)
                    .map_err(|e| CompressionError::DecompressionFailed(e.to_string()))?;

                let mut decompressed = Vec::new();
                let mut buffer = [0u8; 8192];
                let mut total_read = 0;

                loop {
                    let bytes_read = decoder.read(&mut buffer)
                        .map_err(|e| CompressionError::DecompressionFailed(e.to_string()))?;

                    if bytes_read == 0 {
                        break;
                    }

                    total_read += bytes_read;
                    if total_read > self.max_decompressed_size {
                        return Err(CompressionError::DecompressedTooLarge);
                    }

                    decompressed.extend_from_slice(&buffer[..bytes_read]);
                }

                Ok(decompressed)
            }
            Algorithm::None => Ok(payload.to_vec()),
        }
    }

    /// Compress a JSON string, returning the compressed bytes
    pub fn compress_json(&self, json: &str) -> Result<Vec<u8>, CompressionError> {
        self.compress(json.as_bytes())
    }

    /// Decompress bytes to a JSON string
    pub fn decompress_to_string(&self, data: &[u8]) -> Result<String, CompressionError> {
        let decompressed = self.decompress(data)?;
        String::from_utf8(decompressed)
            .map_err(|e| CompressionError::DecompressionFailed(e.to_string()))
    }
}

/// Create a shared compression codec wrapped in Arc
pub fn create_codec(
    algorithm: &CompressionAlgorithm,
    level: u32,
    threshold: usize,
    max_decompressed_size: usize,
) -> Arc<CompressionCodec> {
    Arc::new(CompressionCodec::from_config(
        algorithm,
        level,
        threshold,
        max_decompressed_size,
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_codec() -> CompressionCodec {
        CompressionCodec::new(Algorithm::Zstd, 3, 100, 10_485_760)
    }

    #[test]
    fn test_compress_small_message_no_compression() {
        let codec = test_codec();
        let data = b"Hello, World!";

        let compressed = codec.compress(data).unwrap();

        // First byte should indicate uncompressed
        assert_eq!(compressed[0], CompressionFlags::uncompressed());
        // Rest should be original data
        assert_eq!(&compressed[1..], data);
    }

    #[test]
    fn test_compress_large_message() {
        let codec = test_codec();
        // Create data larger than threshold
        let data = "x".repeat(1000);

        let compressed = codec.compress(data.as_bytes()).unwrap();

        // First byte should indicate compressed
        assert!(CompressionFlags::is_compressed(compressed[0]));
        // Compressed data should be smaller
        assert!(compressed.len() < data.len());
    }

    #[test]
    fn test_roundtrip() {
        let codec = test_codec();
        let original = "This is a test message that needs to be compressed. ".repeat(20);

        let compressed = codec.compress(original.as_bytes()).unwrap();
        let decompressed = codec.decompress(&compressed).unwrap();

        assert_eq!(String::from_utf8(decompressed).unwrap(), original);
    }

    #[test]
    fn test_decompress_uncompressed() {
        let codec = test_codec();
        let data = b"Hello, World!";

        // Manually create uncompressed format
        let mut input = vec![CompressionFlags::uncompressed()];
        input.extend_from_slice(data);

        let decompressed = codec.decompress(&input).unwrap();
        assert_eq!(decompressed, data);
    }

    #[test]
    fn test_decompress_empty_fails() {
        let codec = test_codec();
        let result = codec.decompress(&[]);
        assert!(matches!(result, Err(CompressionError::EmptyInput)));
    }

    #[test]
    fn test_decompress_too_large_fails() {
        let codec = CompressionCodec::new(Algorithm::Zstd, 3, 100, 100); // Max 100 bytes
        let original = "x".repeat(200);

        let compressed = CompressionCodec::new(Algorithm::Zstd, 3, 100, 10_485_760)
            .compress(original.as_bytes())
            .unwrap();

        let result = codec.decompress(&compressed);
        assert!(matches!(result, Err(CompressionError::DecompressedTooLarge)));
    }

    #[test]
    fn test_json_roundtrip() {
        let codec = test_codec();
        let json = r#"{"type":"message","payload":{"id":"123","content":"Hello, World!"}}"#.repeat(10);

        let compressed = codec.compress_json(&json).unwrap();
        let decompressed = codec.decompress_to_string(&compressed).unwrap();

        assert_eq!(decompressed, json);
    }

    #[test]
    fn test_algorithm_none_no_compression() {
        let codec = CompressionCodec::new(Algorithm::None, 3, 100, 10_485_760);
        let data = "x".repeat(1000);

        let compressed = codec.compress(data.as_bytes()).unwrap();

        // Should not compress
        assert!(!CompressionFlags::is_compressed(compressed[0]));
        assert_eq!(&compressed[1..], data.as_bytes());
    }

    #[test]
    fn test_compression_not_beneficial() {
        let codec = CompressionCodec::new(Algorithm::Zstd, 3, 10, 10_485_760);
        // Random-like data that doesn't compress well
        let data: Vec<u8> = (0..200).map(|i| (i * 17 + 13) as u8).collect();

        let compressed = codec.compress(&data).unwrap();

        // If compression doesn't help, should store uncompressed
        if !CompressionFlags::is_compressed(compressed[0]) {
            assert_eq!(&compressed[1..], &data[..]);
        }
    }
}
