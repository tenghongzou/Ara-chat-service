//! File storage abstraction

use async_trait::async_trait;

use super::AttachmentError;

/// File storage trait for abstracting storage backends
#[async_trait]
pub trait FileStorage: Send + Sync {
    /// Upload a file to storage
    ///
    /// Returns the storage path/key
    async fn upload(
        &self,
        key: &str,
        data: &[u8],
        mime_type: &str,
    ) -> Result<String, AttachmentError>;

    /// Download a file from storage
    async fn download(&self, key: &str) -> Result<Vec<u8>, AttachmentError>;

    /// Delete a file from storage
    async fn delete(&self, key: &str) -> Result<(), AttachmentError>;

    /// Check if a file exists
    async fn exists(&self, key: &str) -> Result<bool, AttachmentError>;

    /// Get public URL for a file (if available)
    fn public_url(&self, key: &str) -> Option<String>;

    /// Get the storage backend name
    fn backend_name(&self) -> &'static str;
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;

    // Mock storage for testing
    struct MockStorage;

    #[async_trait]
    impl FileStorage for MockStorage {
        async fn upload(&self, key: &str, _data: &[u8], _mime_type: &str) -> Result<String, AttachmentError> {
            Ok(key.to_string())
        }

        async fn download(&self, _key: &str) -> Result<Vec<u8>, AttachmentError> {
            Ok(vec![])
        }

        async fn delete(&self, _key: &str) -> Result<(), AttachmentError> {
            Ok(())
        }

        async fn exists(&self, _key: &str) -> Result<bool, AttachmentError> {
            Ok(true)
        }

        fn public_url(&self, key: &str) -> Option<String> {
            Some(format!("https://example.com/{}", key))
        }

        fn backend_name(&self) -> &'static str {
            "mock"
        }
    }

    #[tokio::test]
    async fn test_mock_storage_upload() {
        let storage = MockStorage;
        let result = storage.upload("test.txt", b"hello", "text/plain").await;
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), "test.txt");
    }

    #[tokio::test]
    async fn test_trait_object() {
        let storage: Arc<dyn FileStorage> = Arc::new(MockStorage);
        assert_eq!(storage.backend_name(), "mock");
    }
}
