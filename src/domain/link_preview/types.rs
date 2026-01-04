//! Link Preview types

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// Link preview metadata for a URL in a message
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LinkPreview {
    pub id: Uuid,
    pub message_id: Uuid,
    pub url: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub image_url: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub site_name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub favicon_url: Option<String>,
    pub status: PreviewStatus,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub fetched_at: Option<DateTime<Utc>>,
    pub created_at: DateTime<Utc>,
}

/// Status of a link preview fetch operation
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PreviewStatus {
    Pending,
    Success,
    Failed,
}

impl PreviewStatus {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Pending => "pending",
            Self::Success => "success",
            Self::Failed => "failed",
        }
    }

    pub fn from_str(s: &str) -> Self {
        match s {
            "success" => Self::Success,
            "failed" => Self::Failed,
            _ => Self::Pending,
        }
    }
}

/// Open Graph metadata extracted from a URL
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct OpenGraphData {
    pub title: Option<String>,
    pub description: Option<String>,
    pub image: Option<String>,
    pub site_name: Option<String>,
    pub url: Option<String>,
    pub favicon: Option<String>,
}

/// Database row representation for link previews
#[derive(Debug, Clone)]
pub struct LinkPreviewRow {
    pub id: Uuid,
    pub message_id: Uuid,
    pub url: String,
    pub url_hash: String,
    pub title: Option<String>,
    pub description: Option<String>,
    pub image_url: Option<String>,
    pub site_name: Option<String>,
    pub favicon_url: Option<String>,
    pub status: String,
    pub error: Option<String>,
    pub fetched_at: Option<DateTime<Utc>>,
    pub created_at: DateTime<Utc>,
}

impl LinkPreviewRow {
    /// Convert database row to domain type
    pub fn into_link_preview(self) -> LinkPreview {
        LinkPreview {
            id: self.id,
            message_id: self.message_id,
            url: self.url,
            title: self.title,
            description: self.description,
            image_url: self.image_url,
            site_name: self.site_name,
            favicon_url: self.favicon_url,
            status: PreviewStatus::from_str(&self.status),
            error: self.error,
            fetched_at: self.fetched_at,
            created_at: self.created_at,
        }
    }
}

/// Pending preview record for background processing
#[derive(Debug, Clone)]
pub struct PendingPreview {
    pub id: Uuid,
    pub message_id: Uuid,
    pub url: String,
    pub url_hash: String,
    pub created_at: DateTime<Utc>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_preview_status_serialization() {
        assert_eq!(serde_json::to_string(&PreviewStatus::Pending).unwrap(), "\"pending\"");
        assert_eq!(serde_json::to_string(&PreviewStatus::Success).unwrap(), "\"success\"");
        assert_eq!(serde_json::to_string(&PreviewStatus::Failed).unwrap(), "\"failed\"");
    }

    #[test]
    fn test_preview_status_from_str() {
        assert_eq!(PreviewStatus::from_str("pending"), PreviewStatus::Pending);
        assert_eq!(PreviewStatus::from_str("success"), PreviewStatus::Success);
        assert_eq!(PreviewStatus::from_str("failed"), PreviewStatus::Failed);
        assert_eq!(PreviewStatus::from_str("unknown"), PreviewStatus::Pending);
    }

    #[test]
    fn test_link_preview_serialization() {
        let preview = LinkPreview {
            id: Uuid::new_v4(),
            message_id: Uuid::new_v4(),
            url: "https://example.com".to_string(),
            title: Some("Example".to_string()),
            description: Some("Description".to_string()),
            image_url: Some("https://example.com/og.png".to_string()),
            site_name: Some("Example Site".to_string()),
            favicon_url: None,
            status: PreviewStatus::Success,
            error: None,
            fetched_at: Some(Utc::now()),
            created_at: Utc::now(),
        };

        let json = serde_json::to_string(&preview).unwrap();
        assert!(json.contains("\"url\":\"https://example.com\""));
        assert!(json.contains("\"status\":\"success\""));
        // favicon_url should be omitted since it's None
        assert!(!json.contains("favicon_url"));
    }
}
