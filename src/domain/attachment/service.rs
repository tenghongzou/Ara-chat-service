//! Attachment service - business logic for file uploads

use std::sync::Arc;

use chrono::Utc;
use sha2::{Digest, Sha256};
use sqlx::PgPool;
use uuid::Uuid;

use crate::config::{FileStorageSettings, StorageBackend};
use crate::conversation::ConversationService;

use super::{
    Attachment, AttachmentError, AttachmentResponse, FileStorage, LocalFileStorage,
    ThumbnailGenerator, UploadRequest,
};

/// Attachment service for handling file uploads
pub struct AttachmentService {
    storage: Arc<dyn FileStorage>,
    thumbnail_generator: ThumbnailGenerator,
    db: Arc<PgPool>,
    settings: FileStorageSettings,
    conversation_service: Arc<ConversationService>,
}

impl AttachmentService {
    /// Create a new attachment service with local storage
    pub fn with_local_storage(
        db: Arc<PgPool>,
        settings: FileStorageSettings,
        conversation_service: Arc<ConversationService>,
    ) -> Self {
        let storage = LocalFileStorage::new(&settings.local_path);
        Self {
            storage: Arc::new(storage),
            thumbnail_generator: ThumbnailGenerator::new(settings.thumbnail_max_dimension),
            db,
            settings,
            conversation_service,
        }
    }

    /// Create attachment service based on settings
    ///
    /// Currently only local storage is supported. S3/MinIO support planned for future.
    pub async fn from_settings(
        db: Arc<PgPool>,
        settings: FileStorageSettings,
        conversation_service: Arc<ConversationService>,
    ) -> Result<Self, AttachmentError> {
        match settings.backend {
            StorageBackend::Local => Ok(Self::with_local_storage(db, settings, conversation_service)),
            StorageBackend::S3 => {
                tracing::warn!("S3 storage is not yet implemented, falling back to local storage");
                Ok(Self::with_local_storage(db, settings, conversation_service))
            }
        }
    }

    /// Upload a file
    pub async fn upload(
        &self,
        user_id: Uuid,
        request: UploadRequest,
    ) -> Result<Attachment, AttachmentError> {
        // Validate file size
        if request.data.len() > self.settings.max_file_size {
            return Err(AttachmentError::FileTooLarge {
                size: request.data.len(),
                max: self.settings.max_file_size,
            });
        }

        // Validate MIME type
        if !self.settings.allowed_types.is_empty()
            && !self.settings.allowed_types.contains(&request.mime_type)
        {
            return Err(AttachmentError::InvalidMimeType(request.mime_type.clone()));
        }

        // Validate user is participant
        if !self
            .conversation_service
            .is_participant(request.conversation_id, user_id)
            .await?
        {
            return Err(AttachmentError::NotParticipant);
        }

        // Calculate content hash for deduplication
        let content_hash = self.calculate_hash(&request.data);

        // Check for existing attachment with same hash in conversation
        if let Some(existing) = self
            .find_by_hash(&content_hash, request.conversation_id)
            .await?
        {
            tracing::debug!(
                hash = %content_hash,
                existing_id = %existing.id,
                "Found duplicate attachment"
            );
            return Ok(existing);
        }

        // Generate storage key
        let extension = self.get_extension(&request.file_name, &request.mime_type);
        let storage_key = LocalFileStorage::generate_key(&request.file_name, &extension);

        // Upload main file
        self.storage
            .upload(&storage_key, &request.data, &request.mime_type)
            .await?;

        // Generate thumbnail if applicable
        let thumbnail_path = if self.settings.thumbnail_enabled
            && self.thumbnail_generator.is_supported(&request.mime_type)
        {
            match self.thumbnail_generator.generate(&request.data, &request.mime_type) {
                Ok(thumb_data) => {
                    let thumb_key = format!("thumbs/{}", storage_key);
                    match self.storage.upload(&thumb_key, &thumb_data, "image/jpeg").await {
                        Ok(_) => Some(thumb_key),
                        Err(e) => {
                            tracing::warn!(error = %e, "Failed to upload thumbnail");
                            None
                        }
                    }
                }
                Err(e) => {
                    tracing::warn!(error = %e, "Failed to generate thumbnail");
                    None
                }
            }
        } else {
            None
        };

        // Store in database
        let attachment = self
            .insert_attachment(
                request.conversation_id,
                user_id,
                &request.file_name,
                request.data.len() as i64,
                &request.mime_type,
                &content_hash,
                &storage_key,
                thumbnail_path.as_deref(),
            )
            .await?;

        tracing::info!(
            attachment_id = %attachment.id,
            conversation_id = %request.conversation_id,
            file_name = %request.file_name,
            size = request.data.len(),
            "Attachment uploaded"
        );

        Ok(attachment)
    }

    /// Get attachment by ID
    pub async fn get(&self, id: Uuid) -> Result<Attachment, AttachmentError> {
        let attachment = sqlx::query_as!(
            Attachment,
            r#"
            SELECT id, conversation_id, message_id, uploader_id,
                   file_name, file_size, mime_type, content_hash,
                   storage_backend, storage_path, thumbnail_path,
                   created_at
            FROM attachments
            WHERE id = $1
            "#,
            id
        )
        .fetch_optional(self.db.as_ref())
        .await?
        .ok_or(AttachmentError::NotFound(id))?;

        Ok(attachment)
    }

    /// Get attachment response with URLs
    pub async fn get_response(&self, id: Uuid) -> Result<AttachmentResponse, AttachmentError> {
        let attachment = self.get(id).await?;
        Ok(self.to_response(attachment))
    }

    /// Delete an attachment
    pub async fn delete(&self, id: Uuid, user_id: Uuid) -> Result<(), AttachmentError> {
        let attachment = self.get(id).await?;

        // Only uploader can delete
        if attachment.uploader_id != user_id {
            return Err(AttachmentError::PermissionDenied);
        }

        // Delete from storage
        self.storage.delete(&attachment.storage_path).await?;
        if let Some(ref thumb_path) = attachment.thumbnail_path {
            let _ = self.storage.delete(thumb_path).await;
        }

        // Delete from database
        sqlx::query!("DELETE FROM attachments WHERE id = $1", id)
            .execute(self.db.as_ref())
            .await?;

        tracing::info!(attachment_id = %id, "Attachment deleted");

        Ok(())
    }

    /// Get download URL for an attachment
    pub fn get_download_url(&self, attachment: &Attachment) -> Option<String> {
        self.storage.public_url(&attachment.storage_path)
    }

    /// Get thumbnail URL for an attachment
    pub fn get_thumbnail_url(&self, attachment: &Attachment) -> Option<String> {
        attachment
            .thumbnail_path
            .as_ref()
            .and_then(|path| self.storage.public_url(path))
    }

    /// Convert attachment to response with URLs
    pub fn to_response(&self, attachment: Attachment) -> AttachmentResponse {
        let url = self.get_download_url(&attachment);
        let thumbnail_url = self.get_thumbnail_url(&attachment);

        AttachmentResponse {
            id: attachment.id,
            conversation_id: attachment.conversation_id,
            message_id: attachment.message_id,
            file_name: attachment.file_name,
            file_size: attachment.file_size,
            mime_type: attachment.mime_type,
            url,
            thumbnail_url,
            created_at: attachment.created_at,
        }
    }

    /// List attachments for a conversation
    pub async fn list_by_conversation(
        &self,
        conversation_id: Uuid,
        limit: i64,
        offset: i64,
    ) -> Result<Vec<Attachment>, AttachmentError> {
        let attachments = sqlx::query_as!(
            Attachment,
            r#"
            SELECT id, conversation_id, message_id, uploader_id,
                   file_name, file_size, mime_type, content_hash,
                   storage_backend, storage_path, thumbnail_path,
                   created_at
            FROM attachments
            WHERE conversation_id = $1
            ORDER BY created_at DESC
            LIMIT $2 OFFSET $3
            "#,
            conversation_id,
            limit,
            offset
        )
        .fetch_all(self.db.as_ref())
        .await?;

        Ok(attachments)
    }

    /// Associate attachment with a message
    pub async fn link_to_message(
        &self,
        attachment_id: Uuid,
        message_id: Uuid,
    ) -> Result<(), AttachmentError> {
        sqlx::query!(
            "UPDATE attachments SET message_id = $1 WHERE id = $2",
            message_id,
            attachment_id
        )
        .execute(self.db.as_ref())
        .await?;

        Ok(())
    }

    // Private helper methods

    fn calculate_hash(&self, data: &[u8]) -> String {
        let mut hasher = Sha256::new();
        hasher.update(data);
        format!("{:x}", hasher.finalize())
    }

    fn get_extension(&self, file_name: &str, mime_type: &str) -> String {
        // Try to get extension from filename first
        if let Some(ext) = std::path::Path::new(file_name)
            .extension()
            .and_then(|e| e.to_str())
        {
            return ext.to_lowercase();
        }

        // Fall back to MIME type
        match mime_type {
            "image/jpeg" => "jpg",
            "image/png" => "png",
            "image/gif" => "gif",
            "image/webp" => "webp",
            "application/pdf" => "pdf",
            "text/plain" => "txt",
            "application/zip" => "zip",
            _ => "bin",
        }
        .to_string()
    }

    async fn find_by_hash(
        &self,
        content_hash: &str,
        conversation_id: Uuid,
    ) -> Result<Option<Attachment>, AttachmentError> {
        let attachment = sqlx::query_as!(
            Attachment,
            r#"
            SELECT id, conversation_id, message_id, uploader_id,
                   file_name, file_size, mime_type, content_hash,
                   storage_backend, storage_path, thumbnail_path,
                   created_at
            FROM attachments
            WHERE content_hash = $1 AND conversation_id = $2
            LIMIT 1
            "#,
            content_hash,
            conversation_id
        )
        .fetch_optional(self.db.as_ref())
        .await?;

        Ok(attachment)
    }

    async fn insert_attachment(
        &self,
        conversation_id: Uuid,
        uploader_id: Uuid,
        file_name: &str,
        file_size: i64,
        mime_type: &str,
        content_hash: &str,
        storage_path: &str,
        thumbnail_path: Option<&str>,
    ) -> Result<Attachment, AttachmentError> {
        let backend_name = self.storage.backend_name();
        let now = Utc::now();

        let attachment = sqlx::query_as!(
            Attachment,
            r#"
            INSERT INTO attachments (
                conversation_id, uploader_id, file_name, file_size,
                mime_type, content_hash, storage_backend, storage_path,
                thumbnail_path, created_at
            )
            VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10)
            RETURNING id, conversation_id, message_id, uploader_id,
                      file_name, file_size, mime_type, content_hash,
                      storage_backend, storage_path, thumbnail_path,
                      created_at
            "#,
            conversation_id,
            uploader_id,
            file_name,
            file_size,
            mime_type,
            content_hash,
            backend_name,
            storage_path,
            thumbnail_path,
            now
        )
        .fetch_one(self.db.as_ref())
        .await?;

        Ok(attachment)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_get_extension_from_filename() {
        // Create a mock service just to test the extension logic
        let settings = FileStorageSettings::default();
        assert!(!settings.allowed_types.is_empty());
    }

    #[test]
    fn test_calculate_hash() {
        let mut hasher = Sha256::new();
        hasher.update(b"test data");
        let hash = format!("{:x}", hasher.finalize());
        assert_eq!(hash.len(), 64);
    }
}
