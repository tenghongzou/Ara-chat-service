//! Attachment types

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::config::StorageBackend;

/// Stored attachment record
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Attachment {
    pub id: Uuid,
    pub conversation_id: Uuid,
    pub message_id: Option<Uuid>,
    pub uploader_id: Uuid,
    pub file_name: String,
    pub file_size: i64,
    pub mime_type: String,
    pub content_hash: String,
    pub storage_backend: String,
    pub storage_path: String,
    pub thumbnail_path: Option<String>,
    pub created_at: DateTime<Utc>,
}

/// Request to upload a file
#[derive(Debug)]
pub struct UploadRequest {
    pub conversation_id: Uuid,
    pub file_name: String,
    pub data: Vec<u8>,
    pub mime_type: String,
}

/// Storage location information
#[derive(Debug, Clone)]
pub struct StorageInfo {
    pub backend: StorageBackend,
    pub path: String,
    pub thumbnail_path: Option<String>,
}

/// API response for attachment
#[derive(Debug, Clone, Serialize)]
pub struct AttachmentResponse {
    pub id: Uuid,
    pub conversation_id: Uuid,
    pub message_id: Option<Uuid>,
    pub file_name: String,
    pub file_size: i64,
    pub mime_type: String,
    pub url: Option<String>,
    pub thumbnail_url: Option<String>,
    pub created_at: DateTime<Utc>,
}

impl From<Attachment> for AttachmentResponse {
    fn from(a: Attachment) -> Self {
        Self {
            id: a.id,
            conversation_id: a.conversation_id,
            message_id: a.message_id,
            file_name: a.file_name,
            file_size: a.file_size,
            mime_type: a.mime_type,
            url: None, // Will be filled by service
            thumbnail_url: None,
            created_at: a.created_at,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_attachment_response_from_attachment() {
        let attachment = Attachment {
            id: Uuid::new_v4(),
            conversation_id: Uuid::new_v4(),
            message_id: Some(Uuid::new_v4()),
            uploader_id: Uuid::new_v4(),
            file_name: "test.jpg".to_string(),
            file_size: 1024,
            mime_type: "image/jpeg".to_string(),
            content_hash: "abc123".to_string(),
            storage_backend: "local".to_string(),
            storage_path: "/uploads/test.jpg".to_string(),
            thumbnail_path: Some("/uploads/test_thumb.jpg".to_string()),
            created_at: Utc::now(),
        };

        let response: AttachmentResponse = attachment.clone().into();
        assert_eq!(response.id, attachment.id);
        assert_eq!(response.file_name, attachment.file_name);
        assert_eq!(response.file_size, attachment.file_size);
    }

    #[test]
    fn test_upload_request() {
        let req = UploadRequest {
            conversation_id: Uuid::new_v4(),
            file_name: "test.png".to_string(),
            data: vec![0u8; 100],
            mime_type: "image/png".to_string(),
        };
        assert_eq!(req.data.len(), 100);
    }
}
