//! GDPR data types and DTOs
//!
//! Defines types for GDPR compliance operations including:
//! - Data export structures
//! - Deletion options and results
//! - Audit log entries

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// GDPR action types for audit logging
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum GdprActionType {
    DataExportRequested,
    DataExportCompleted,
    DataExportFailed,
    DataDeletionRequested,
    DataDeletionCompleted,
    DataDeletionFailed,
    DataAccessRequested,
}

impl GdprActionType {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::DataExportRequested => "DATA_EXPORT_REQUESTED",
            Self::DataExportCompleted => "DATA_EXPORT_COMPLETED",
            Self::DataExportFailed => "DATA_EXPORT_FAILED",
            Self::DataDeletionRequested => "DATA_DELETION_REQUESTED",
            Self::DataDeletionCompleted => "DATA_DELETION_COMPLETED",
            Self::DataDeletionFailed => "DATA_DELETION_FAILED",
            Self::DataAccessRequested => "DATA_ACCESS_REQUESTED",
        }
    }
}

/// Status of a GDPR request
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum GdprRequestStatus {
    Pending,
    Processing,
    Completed,
    Failed,
}

impl GdprRequestStatus {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Pending => "pending",
            Self::Processing => "processing",
            Self::Completed => "completed",
            Self::Failed => "failed",
        }
    }
}

/// Who initiated the GDPR request
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum RequesterType {
    User,
    Admin,
    System,
}

impl RequesterType {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::User => "user",
            Self::Admin => "admin",
            Self::System => "system",
        }
    }
}

/// Context for a GDPR request (for audit logging)
#[derive(Debug, Clone)]
pub struct GdprRequestContext {
    pub request_id: Uuid,
    pub subject_user_id: Uuid,
    pub requester_user_id: Option<Uuid>,
    pub requester_type: RequesterType,
    pub tenant_id: String,
    pub request_ip: Option<String>,
    pub request_user_agent: Option<String>,
}

impl GdprRequestContext {
    pub fn new(subject_user_id: Uuid, tenant_id: String) -> Self {
        Self {
            request_id: Uuid::new_v4(),
            subject_user_id,
            requester_user_id: Some(subject_user_id),
            requester_type: RequesterType::User,
            tenant_id,
            request_ip: None,
            request_user_agent: None,
        }
    }

    pub fn with_requester(mut self, user_id: Uuid, requester_type: RequesterType) -> Self {
        self.requester_user_id = Some(user_id);
        self.requester_type = requester_type;
        self
    }

    pub fn with_request_info(mut self, ip: Option<String>, user_agent: Option<String>) -> Self {
        self.request_ip = ip;
        self.request_user_agent = user_agent;
        self
    }
}

/// Summary of data affected by a GDPR operation
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct AffectedDataSummary {
    pub messages_deleted: u64,
    pub messages_anonymized: u64,
    pub reactions_deleted: u64,
    pub read_receipts_deleted: u64,
    pub attachments_deleted: u64,
    pub attachment_bytes_deleted: u64,
    pub conversations_left: u64,
    pub dm_lookups_deleted: u64,
    pub mentions_removed: u64,
}

/// Options for data deletion
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeletionOptions {
    /// If true, anonymize messages instead of hard deleting content
    #[serde(default = "default_true")]
    pub anonymize_messages: bool,
    /// If true, preserve thread structure (replace content with placeholder)
    #[serde(default = "default_true")]
    pub preserve_thread_structure: bool,
    /// If true, also delete from DM lookup table
    #[serde(default = "default_true")]
    pub delete_dm_lookups: bool,
    /// If true, physically delete attachment files
    #[serde(default = "default_true")]
    pub delete_attachment_files: bool,
}

fn default_true() -> bool {
    true
}

impl Default for DeletionOptions {
    fn default() -> Self {
        Self {
            anonymize_messages: true,
            preserve_thread_structure: true,
            delete_dm_lookups: true,
            delete_attachment_files: true,
        }
    }
}

/// Result of a deletion operation
#[derive(Debug, Clone, Serialize)]
pub struct DeletionResult {
    pub request_id: Uuid,
    pub status: GdprRequestStatus,
    pub affected: AffectedDataSummary,
    pub completed_at: DateTime<Utc>,
}

/// Result of an export operation
#[derive(Debug, Clone, Serialize)]
pub struct ExportResult {
    pub request_id: Uuid,
    pub status: GdprRequestStatus,
    pub export_path: Option<String>,
    pub export_size_bytes: u64,
    pub message_count: u64,
    pub attachment_count: u64,
    pub completed_at: DateTime<Utc>,
}

// ============================================================================
// Export Data Structures
// ============================================================================

/// Complete user data export structure
#[derive(Debug, Serialize)]
pub struct UserDataExport {
    pub export_metadata: ExportMetadata,
    pub profile: UserProfileExport,
    pub conversations: Vec<ConversationExport>,
    pub attachments: Vec<AttachmentExport>,
}

/// Metadata about the export
#[derive(Debug, Serialize)]
pub struct ExportMetadata {
    pub user_id: Uuid,
    pub tenant_id: String,
    pub exported_at: DateTime<Utc>,
    pub format_version: String,
    pub gdpr_request_id: Uuid,
}

/// User profile summary in export
#[derive(Debug, Serialize)]
pub struct UserProfileExport {
    pub user_id: Uuid,
    pub first_message_at: Option<DateTime<Utc>>,
    pub last_message_at: Option<DateTime<Utc>>,
    pub total_messages_sent: u64,
    pub total_conversations: u64,
    pub total_attachments_uploaded: u64,
}

/// Conversation data in export
#[derive(Debug, Serialize)]
pub struct ConversationExport {
    pub conversation_id: Uuid,
    pub conversation_type: String,
    pub name: Option<String>,
    pub joined_at: DateTime<Utc>,
    pub left_at: Option<DateTime<Utc>>,
    pub role: String,
    pub messages: Vec<MessageExport>,
}

/// Message data in export
#[derive(Debug, Serialize)]
pub struct MessageExport {
    pub id: Uuid,
    pub content: String,
    pub content_type: String,
    pub created_at: DateTime<Utc>,
    pub updated_at: Option<DateTime<Utc>>,
    pub reply_to_id: Option<Uuid>,
    pub mentions: Vec<Uuid>,
    pub reactions_received: Vec<ReactionExport>,
    pub attachments: Vec<Uuid>,
}

/// Reaction data in export
#[derive(Debug, Serialize)]
pub struct ReactionExport {
    pub emoji: String,
    pub from_user_id: Uuid,
    pub created_at: DateTime<Utc>,
}

/// Attachment metadata in export
#[derive(Debug, Serialize)]
pub struct AttachmentExport {
    pub id: Uuid,
    pub conversation_id: Uuid,
    pub message_id: Option<Uuid>,
    pub file_name: String,
    pub file_size: i64,
    pub mime_type: String,
    pub created_at: DateTime<Utc>,
    /// Relative path in export archive (if attachments included)
    pub download_path: Option<String>,
}

// ============================================================================
// Audit Log Entry
// ============================================================================

/// Audit log entry from database
#[derive(Debug, Clone, Serialize)]
pub struct AuditLogEntry {
    pub id: Uuid,
    pub action_type: String,
    pub subject_user_id: Uuid,
    pub requester_user_id: Option<Uuid>,
    pub requester_type: String,
    pub request_id: Uuid,
    pub status: String,
    pub affected_data: Option<serde_json::Value>,
    pub error_message: Option<String>,
    pub created_at: DateTime<Utc>,
    pub completed_at: Option<DateTime<Utc>>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_gdpr_action_type_as_str() {
        assert_eq!(GdprActionType::DataExportRequested.as_str(), "DATA_EXPORT_REQUESTED");
        assert_eq!(GdprActionType::DataDeletionCompleted.as_str(), "DATA_DELETION_COMPLETED");
    }

    #[test]
    fn test_request_status_as_str() {
        assert_eq!(GdprRequestStatus::Pending.as_str(), "pending");
        assert_eq!(GdprRequestStatus::Completed.as_str(), "completed");
    }

    #[test]
    fn test_requester_type_as_str() {
        assert_eq!(RequesterType::User.as_str(), "user");
        assert_eq!(RequesterType::Admin.as_str(), "admin");
        assert_eq!(RequesterType::System.as_str(), "system");
    }

    #[test]
    fn test_deletion_options_default() {
        let options = DeletionOptions::default();
        assert!(options.anonymize_messages);
        assert!(options.preserve_thread_structure);
        assert!(options.delete_dm_lookups);
        assert!(options.delete_attachment_files);
    }

    #[test]
    fn test_gdpr_request_context_new() {
        let user_id = Uuid::new_v4();
        let ctx = GdprRequestContext::new(user_id, "default".to_string());

        assert_eq!(ctx.subject_user_id, user_id);
        assert_eq!(ctx.requester_user_id, Some(user_id));
        assert_eq!(ctx.requester_type, RequesterType::User);
        assert_eq!(ctx.tenant_id, "default");
    }

    #[test]
    fn test_affected_data_summary_default() {
        let summary = AffectedDataSummary::default();
        assert_eq!(summary.messages_deleted, 0);
        assert_eq!(summary.messages_anonymized, 0);
        assert_eq!(summary.reactions_deleted, 0);
    }

    #[test]
    fn test_deletion_options_serialization() {
        let options = DeletionOptions {
            anonymize_messages: true,
            preserve_thread_structure: false,
            delete_dm_lookups: true,
            delete_attachment_files: false,
        };

        let json = serde_json::to_string(&options).unwrap();
        assert!(json.contains("\"anonymize_messages\":true"));
        assert!(json.contains("\"preserve_thread_structure\":false"));
    }

    #[test]
    fn test_export_metadata_serialization() {
        let metadata = ExportMetadata {
            user_id: Uuid::new_v4(),
            tenant_id: "test".to_string(),
            exported_at: Utc::now(),
            format_version: "1.0".to_string(),
            gdpr_request_id: Uuid::new_v4(),
        };

        let json = serde_json::to_string(&metadata).unwrap();
        assert!(json.contains("\"format_version\":\"1.0\""));
    }
}
