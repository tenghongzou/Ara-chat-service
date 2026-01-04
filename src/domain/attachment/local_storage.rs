//! Local file system storage implementation

use std::path::{Path, PathBuf};

use async_trait::async_trait;
use chrono::{Datelike, Utc};
use tokio::fs;
use tokio::io::AsyncWriteExt;

use super::{AttachmentError, FileStorage};

/// Local file system storage
pub struct LocalFileStorage {
    base_path: PathBuf,
    public_url_prefix: Option<String>,
}

impl LocalFileStorage {
    /// Create a new local file storage
    pub fn new(base_path: impl AsRef<Path>) -> Self {
        Self {
            base_path: base_path.as_ref().to_path_buf(),
            public_url_prefix: None,
        }
    }

    /// Set public URL prefix for generating public URLs
    pub fn with_public_url(mut self, prefix: impl Into<String>) -> Self {
        self.public_url_prefix = Some(prefix.into());
        self
    }

    /// Get full path for a key
    fn full_path(&self, key: &str) -> PathBuf {
        self.base_path.join(key)
    }

    /// Generate storage key for a file
    pub fn generate_key(file_name: &str, extension: &str) -> String {
        let now = Utc::now();
        let uuid = uuid::Uuid::new_v4();
        format!(
            "{}/{:02}/{:02}/{}.{}",
            now.format("%Y"),
            now.month(),
            now.day(),
            uuid,
            extension
        )
    }
}

#[async_trait]
impl FileStorage for LocalFileStorage {
    async fn upload(
        &self,
        key: &str,
        data: &[u8],
        _mime_type: &str,
    ) -> Result<String, AttachmentError> {
        let path = self.full_path(key);

        // Create parent directories
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).await.map_err(|e| {
                AttachmentError::StorageError(format!("Failed to create directory: {}", e))
            })?;
        }

        // Write file
        let mut file = fs::File::create(&path).await.map_err(|e| {
            AttachmentError::StorageError(format!("Failed to create file: {}", e))
        })?;

        file.write_all(data).await.map_err(|e| {
            AttachmentError::StorageError(format!("Failed to write file: {}", e))
        })?;

        file.flush().await.map_err(|e| {
            AttachmentError::StorageError(format!("Failed to flush file: {}", e))
        })?;

        tracing::debug!(key = %key, size = data.len(), "File uploaded to local storage");

        Ok(key.to_string())
    }

    async fn download(&self, key: &str) -> Result<Vec<u8>, AttachmentError> {
        let path = self.full_path(key);
        fs::read(&path).await.map_err(|e| {
            if e.kind() == std::io::ErrorKind::NotFound {
                AttachmentError::StorageError(format!("File not found: {}", key))
            } else {
                AttachmentError::StorageError(format!("Failed to read file: {}", e))
            }
        })
    }

    async fn delete(&self, key: &str) -> Result<(), AttachmentError> {
        let path = self.full_path(key);
        if path.exists() {
            fs::remove_file(&path).await.map_err(|e| {
                AttachmentError::StorageError(format!("Failed to delete file: {}", e))
            })?;
            tracing::debug!(key = %key, "File deleted from local storage");
        }
        Ok(())
    }

    async fn exists(&self, key: &str) -> Result<bool, AttachmentError> {
        let path = self.full_path(key);
        Ok(path.exists())
    }

    fn public_url(&self, key: &str) -> Option<String> {
        self.public_url_prefix
            .as_ref()
            .map(|prefix| format!("{}/{}", prefix.trim_end_matches('/'), key))
    }

    fn backend_name(&self) -> &'static str {
        "local"
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[tokio::test]
    async fn test_upload_and_download() {
        let dir = tempdir().unwrap();
        let storage = LocalFileStorage::new(dir.path());

        let key = "test/file.txt";
        let data = b"Hello, World!";

        // Upload
        let result = storage.upload(key, data, "text/plain").await;
        assert!(result.is_ok());

        // Download
        let downloaded = storage.download(key).await.unwrap();
        assert_eq!(downloaded, data);
    }

    #[tokio::test]
    async fn test_exists() {
        let dir = tempdir().unwrap();
        let storage = LocalFileStorage::new(dir.path());

        assert!(!storage.exists("nonexistent.txt").await.unwrap());

        storage.upload("test.txt", b"data", "text/plain").await.unwrap();
        assert!(storage.exists("test.txt").await.unwrap());
    }

    #[tokio::test]
    async fn test_delete() {
        let dir = tempdir().unwrap();
        let storage = LocalFileStorage::new(dir.path());

        storage.upload("test.txt", b"data", "text/plain").await.unwrap();
        assert!(storage.exists("test.txt").await.unwrap());

        storage.delete("test.txt").await.unwrap();
        assert!(!storage.exists("test.txt").await.unwrap());
    }

    #[tokio::test]
    async fn test_public_url() {
        let storage = LocalFileStorage::new("/uploads")
            .with_public_url("https://example.com/files");

        let url = storage.public_url("test/file.txt");
        assert_eq!(url, Some("https://example.com/files/test/file.txt".to_string()));
    }

    #[test]
    fn test_generate_key() {
        let key = LocalFileStorage::generate_key("test.txt", "txt");
        assert!(key.contains(".txt"));
        assert!(key.contains("/"));
    }

    #[test]
    fn test_backend_name() {
        let storage = LocalFileStorage::new("/tmp");
        assert_eq!(storage.backend_name(), "local");
    }
}
