//! Thumbnail generation for images

use image::{imageops::FilterType, DynamicImage, ImageFormat};
use std::io::Cursor;

use super::AttachmentError;

/// Thumbnail generator for images
pub struct ThumbnailGenerator {
    max_dimension: u32,
}

impl ThumbnailGenerator {
    /// Create a new thumbnail generator
    pub fn new(max_dimension: u32) -> Self {
        Self { max_dimension }
    }

    /// Check if a MIME type is a supported image format
    pub fn is_supported(&self, mime_type: &str) -> bool {
        matches!(
            mime_type,
            "image/jpeg" | "image/png" | "image/gif" | "image/webp"
        )
    }

    /// Generate a thumbnail from image data
    pub fn generate(&self, data: &[u8], mime_type: &str) -> Result<Vec<u8>, AttachmentError> {
        if !self.is_supported(mime_type) {
            return Err(AttachmentError::ThumbnailError(format!(
                "Unsupported image format: {}",
                mime_type
            )));
        }

        // Load image
        let img = image::load_from_memory(data).map_err(|e| {
            AttachmentError::ThumbnailError(format!("Failed to load image: {}", e))
        })?;

        // Calculate thumbnail dimensions
        let (width, height) = self.calculate_dimensions(img.width(), img.height());

        // Resize image
        let thumbnail = img.resize(width, height, FilterType::Lanczos3);

        // Encode as JPEG for smaller size
        self.encode_as_jpeg(&thumbnail)
    }

    /// Calculate thumbnail dimensions while preserving aspect ratio
    fn calculate_dimensions(&self, width: u32, height: u32) -> (u32, u32) {
        if width <= self.max_dimension && height <= self.max_dimension {
            return (width, height);
        }

        let ratio = width as f64 / height as f64;

        if width > height {
            let new_width = self.max_dimension;
            let new_height = (new_width as f64 / ratio) as u32;
            (new_width, new_height.max(1))
        } else {
            let new_height = self.max_dimension;
            let new_width = (new_height as f64 * ratio) as u32;
            (new_width.max(1), new_height)
        }
    }

    /// Encode image as JPEG
    fn encode_as_jpeg(&self, img: &DynamicImage) -> Result<Vec<u8>, AttachmentError> {
        let mut buffer = Cursor::new(Vec::new());
        img.write_to(&mut buffer, ImageFormat::Jpeg)
            .map_err(|e| AttachmentError::ThumbnailError(format!("Failed to encode JPEG: {}", e)))?;
        Ok(buffer.into_inner())
    }
}

impl Default for ThumbnailGenerator {
    fn default() -> Self {
        Self::new(200)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_is_supported() {
        let gen = ThumbnailGenerator::new(200);

        assert!(gen.is_supported("image/jpeg"));
        assert!(gen.is_supported("image/png"));
        assert!(gen.is_supported("image/gif"));
        assert!(gen.is_supported("image/webp"));

        assert!(!gen.is_supported("application/pdf"));
        assert!(!gen.is_supported("text/plain"));
        assert!(!gen.is_supported("video/mp4"));
    }

    #[test]
    fn test_calculate_dimensions_landscape() {
        let gen = ThumbnailGenerator::new(200);

        // Landscape image 800x600 should become 200x150
        let (w, h) = gen.calculate_dimensions(800, 600);
        assert_eq!(w, 200);
        assert_eq!(h, 150);
    }

    #[test]
    fn test_calculate_dimensions_portrait() {
        let gen = ThumbnailGenerator::new(200);

        // Portrait image 600x800 should become 150x200
        let (w, h) = gen.calculate_dimensions(600, 800);
        assert_eq!(w, 150);
        assert_eq!(h, 200);
    }

    #[test]
    fn test_calculate_dimensions_small_image() {
        let gen = ThumbnailGenerator::new(200);

        // Small image should not be resized
        let (w, h) = gen.calculate_dimensions(100, 80);
        assert_eq!(w, 100);
        assert_eq!(h, 80);
    }

    #[test]
    fn test_calculate_dimensions_square() {
        let gen = ThumbnailGenerator::new(200);

        // Square image 500x500 should become 200x200
        let (w, h) = gen.calculate_dimensions(500, 500);
        assert_eq!(w, 200);
        assert_eq!(h, 200);
    }

    #[test]
    fn test_generate_unsupported_format() {
        let gen = ThumbnailGenerator::new(200);
        let result = gen.generate(b"not an image", "application/pdf");
        assert!(result.is_err());
    }

    #[test]
    fn test_generate_invalid_image_data() {
        let gen = ThumbnailGenerator::new(200);
        let result = gen.generate(b"not an image", "image/jpeg");
        assert!(result.is_err());
    }

    #[test]
    fn test_default() {
        let gen = ThumbnailGenerator::default();
        assert_eq!(gen.max_dimension, 200);
    }
}
