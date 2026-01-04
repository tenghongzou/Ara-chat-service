//! GDPR Data Exporter
//!
//! Exports user data in a machine-readable JSON format for GDPR Art. 20 compliance.

use std::path::PathBuf;
use std::sync::Arc;

use chrono::{DateTime, Utc};
use sqlx::PgPool;
use tokio::fs;
use uuid::Uuid;

use super::audit::GdprAuditLogger;
use super::error::GdprError;
use super::types::{
    AffectedDataSummary, AttachmentExport, ConversationExport, ExportMetadata, ExportResult,
    GdprActionType, GdprRequestContext, GdprRequestStatus, MessageExport, ReactionExport,
    UserDataExport, UserProfileExport,
};

/// Database row types for queries
#[derive(Debug, sqlx::FromRow)]
struct ConversationRow {
    conversation_id: Uuid,
    conversation_type: String,
    name: Option<String>,
    joined_at: DateTime<Utc>,
    left_at: Option<DateTime<Utc>>,
    role: String,
}

#[derive(Debug, sqlx::FromRow)]
struct MessageRow {
    id: Uuid,
    content: String,
    content_type: String,
    created_at: DateTime<Utc>,
    updated_at: Option<DateTime<Utc>>,
    reply_to_id: Option<Uuid>,
    mentions: Vec<Uuid>,
}

#[derive(Debug, sqlx::FromRow)]
struct ReactionRow {
    emoji: String,
    user_id: Uuid,
    created_at: DateTime<Utc>,
}

#[derive(Debug, sqlx::FromRow)]
struct AttachmentRow {
    id: Uuid,
    conversation_id: Uuid,
    message_id: Option<Uuid>,
    file_name: String,
    file_size: i64,
    mime_type: String,
    storage_path: String,
    created_at: DateTime<Utc>,
}

#[derive(Debug, sqlx::FromRow)]
struct ProfileStatsRow {
    message_count: i64,
    first_message: Option<DateTime<Utc>>,
    last_message: Option<DateTime<Utc>>,
    conversation_count: i64,
    attachment_count: i64,
}

/// Data exporter for GDPR compliance
pub struct DataExporter {
    pool: Arc<PgPool>,
    audit_logger: Arc<GdprAuditLogger>,
    export_base_path: PathBuf,
    tenant_id: String,
}

impl DataExporter {
    pub fn new(
        pool: Arc<PgPool>,
        audit_logger: Arc<GdprAuditLogger>,
        export_base_path: PathBuf,
    ) -> Self {
        Self {
            pool,
            audit_logger,
            export_base_path,
            tenant_id: "default".to_string(),
        }
    }

    pub fn with_tenant(mut self, tenant_id: String) -> Self {
        self.tenant_id = tenant_id;
        self
    }

    /// Export all user data
    pub async fn export_user_data(
        &self,
        ctx: GdprRequestContext,
        include_attachments: bool,
    ) -> Result<ExportResult, GdprError> {
        // Check for existing pending export
        if self
            .audit_logger
            .has_pending_request(&ctx.tenant_id, ctx.subject_user_id, "DATA_EXPORT")
            .await?
        {
            return Err(GdprError::AlreadyProcessing(ctx.subject_user_id));
        }

        // Log start
        let log_id = self
            .audit_logger
            .log_start(&ctx, GdprActionType::DataExportRequested)
            .await?;

        let result = self.perform_export(&ctx, include_attachments).await;

        match &result {
            Ok(export_result) => {
                let summary = AffectedDataSummary {
                    messages_deleted: export_result.message_count,
                    attachments_deleted: export_result.attachment_count,
                    ..Default::default()
                };
                self.audit_logger.log_completed(log_id, &summary).await?;
            }
            Err(e) => {
                self.audit_logger.log_failed(log_id, &e.to_string()).await?;
            }
        }

        result
    }

    async fn perform_export(
        &self,
        ctx: &GdprRequestContext,
        include_attachments: bool,
    ) -> Result<ExportResult, GdprError> {
        let user_id = ctx.subject_user_id;
        let tenant_id = &ctx.tenant_id;

        // Get user profile statistics
        let profile = self.get_user_profile(user_id, tenant_id).await?;

        // Get all conversations user participated in
        let conversations = self.get_user_conversations(user_id, tenant_id).await?;

        // Build conversation exports with messages
        let mut conversation_exports = Vec::with_capacity(conversations.len());
        let mut total_messages = 0u64;

        for conv in conversations {
            let messages = self
                .get_user_messages_in_conversation(user_id, conv.conversation_id, tenant_id)
                .await?;
            total_messages += messages.len() as u64;

            conversation_exports.push(ConversationExport {
                conversation_id: conv.conversation_id,
                conversation_type: conv.conversation_type,
                name: conv.name,
                joined_at: conv.joined_at,
                left_at: conv.left_at,
                role: conv.role,
                messages,
            });
        }

        // Get attachments
        let attachments = self.get_user_attachments(user_id, tenant_id).await?;
        let attachment_count = attachments.len() as u64;

        // Build export structure
        let export = UserDataExport {
            export_metadata: ExportMetadata {
                user_id,
                tenant_id: tenant_id.clone(),
                exported_at: Utc::now(),
                format_version: "1.0".to_string(),
                gdpr_request_id: ctx.request_id,
            },
            profile,
            conversations: conversation_exports,
            attachments,
        };

        // Create export directory
        let export_dir = self
            .export_base_path
            .join(tenant_id)
            .join(ctx.request_id.to_string());
        fs::create_dir_all(&export_dir).await?;

        // Write JSON export
        let json_path = export_dir.join("data.json");
        let json_content = serde_json::to_string_pretty(&export)?;
        let export_size = json_content.len() as u64;

        fs::write(&json_path, &json_content).await?;

        // Copy attachment files if requested
        if include_attachments {
            let attachments_dir = export_dir.join("attachments");
            if !export.attachments.is_empty() {
                fs::create_dir_all(&attachments_dir).await?;
                // Note: Actual file copying would require FileStorage integration
                // For now, just create the directory structure
            }
        }

        tracing::info!(
            request_id = %ctx.request_id,
            user_id = %user_id,
            messages = total_messages,
            attachments = attachment_count,
            size_bytes = export_size,
            "User data export completed"
        );

        Ok(ExportResult {
            request_id: ctx.request_id,
            status: GdprRequestStatus::Completed,
            export_path: Some(json_path.to_string_lossy().to_string()),
            export_size_bytes: export_size,
            message_count: total_messages,
            attachment_count,
            completed_at: Utc::now(),
        })
    }

    async fn get_user_profile(
        &self,
        user_id: Uuid,
        tenant_id: &str,
    ) -> Result<UserProfileExport, GdprError> {
        let stats: ProfileStatsRow = sqlx::query_as(
            r#"
            SELECT
                COALESCE((SELECT COUNT(*) FROM messages WHERE sender_id = $1 AND tenant_id = $2 AND deleted_at IS NULL), 0) as message_count,
                (SELECT MIN(created_at) FROM messages WHERE sender_id = $1 AND tenant_id = $2 AND deleted_at IS NULL) as first_message,
                (SELECT MAX(created_at) FROM messages WHERE sender_id = $1 AND tenant_id = $2 AND deleted_at IS NULL) as last_message,
                COALESCE((SELECT COUNT(DISTINCT conversation_id) FROM conversation_participants WHERE user_id = $1 AND tenant_id = $2), 0) as conversation_count,
                COALESCE((SELECT COUNT(*) FROM attachments WHERE uploader_id = $1), 0) as attachment_count
            "#,
        )
        .bind(user_id)
        .bind(tenant_id)
        .fetch_one(self.pool.as_ref())
        .await?;

        Ok(UserProfileExport {
            user_id,
            first_message_at: stats.first_message,
            last_message_at: stats.last_message,
            total_messages_sent: stats.message_count as u64,
            total_conversations: stats.conversation_count as u64,
            total_attachments_uploaded: stats.attachment_count as u64,
        })
    }

    async fn get_user_conversations(
        &self,
        user_id: Uuid,
        tenant_id: &str,
    ) -> Result<Vec<ConversationRow>, GdprError> {
        let rows: Vec<ConversationRow> = sqlx::query_as(
            r#"
            SELECT
                cp.conversation_id,
                c.type as conversation_type,
                c.name,
                cp.joined_at,
                cp.left_at,
                cp.role
            FROM conversation_participants cp
            JOIN conversations c ON c.id = cp.conversation_id AND c.tenant_id = cp.tenant_id
            WHERE cp.user_id = $1 AND cp.tenant_id = $2
            ORDER BY cp.joined_at DESC
            "#,
        )
        .bind(user_id)
        .bind(tenant_id)
        .fetch_all(self.pool.as_ref())
        .await?;

        Ok(rows)
    }

    async fn get_user_messages_in_conversation(
        &self,
        user_id: Uuid,
        conversation_id: Uuid,
        tenant_id: &str,
    ) -> Result<Vec<MessageExport>, GdprError> {
        let message_rows: Vec<MessageRow> = sqlx::query_as(
            r#"
            SELECT
                id, content, content_type, created_at, updated_at,
                reply_to_id, mentions
            FROM messages
            WHERE sender_id = $1 AND conversation_id = $2 AND tenant_id = $3 AND deleted_at IS NULL
            ORDER BY created_at ASC
            "#,
        )
        .bind(user_id)
        .bind(conversation_id)
        .bind(tenant_id)
        .fetch_all(self.pool.as_ref())
        .await?;

        let mut messages = Vec::with_capacity(message_rows.len());

        for msg in message_rows {
            // Get reactions on this message
            let reactions = self.get_message_reactions(msg.id).await?;

            // Get attachment IDs for this message
            let attachment_ids = self.get_message_attachment_ids(msg.id).await?;

            messages.push(MessageExport {
                id: msg.id,
                content: msg.content,
                content_type: msg.content_type,
                created_at: msg.created_at,
                updated_at: msg.updated_at,
                reply_to_id: msg.reply_to_id,
                mentions: msg.mentions,
                reactions_received: reactions,
                attachments: attachment_ids,
            });
        }

        Ok(messages)
    }

    async fn get_message_reactions(&self, message_id: Uuid) -> Result<Vec<ReactionExport>, GdprError> {
        let rows: Vec<ReactionRow> = sqlx::query_as(
            r#"
            SELECT emoji, user_id, created_at
            FROM message_reactions
            WHERE message_id = $1
            ORDER BY created_at ASC
            "#,
        )
        .bind(message_id)
        .fetch_all(self.pool.as_ref())
        .await?;

        Ok(rows
            .into_iter()
            .map(|r| ReactionExport {
                emoji: r.emoji,
                from_user_id: r.user_id,
                created_at: r.created_at,
            })
            .collect())
    }

    async fn get_message_attachment_ids(&self, message_id: Uuid) -> Result<Vec<Uuid>, GdprError> {
        let rows: Vec<(Uuid,)> = sqlx::query_as(
            r#"
            SELECT id FROM attachments WHERE message_id = $1
            "#,
        )
        .bind(message_id)
        .fetch_all(self.pool.as_ref())
        .await?;

        Ok(rows.into_iter().map(|(id,)| id).collect())
    }

    async fn get_user_attachments(
        &self,
        user_id: Uuid,
        _tenant_id: &str,
    ) -> Result<Vec<AttachmentExport>, GdprError> {
        let rows: Vec<AttachmentRow> = sqlx::query_as(
            r#"
            SELECT
                id, conversation_id, message_id, file_name,
                file_size, mime_type, storage_path, created_at
            FROM attachments
            WHERE uploader_id = $1
            ORDER BY created_at DESC
            "#,
        )
        .bind(user_id)
        .fetch_all(self.pool.as_ref())
        .await?;

        Ok(rows
            .into_iter()
            .map(|r| AttachmentExport {
                id: r.id,
                conversation_id: r.conversation_id,
                message_id: r.message_id,
                file_name: r.file_name,
                file_size: r.file_size,
                mime_type: r.mime_type,
                created_at: r.created_at,
                download_path: Some(format!("attachments/{}", r.storage_path)),
            })
            .collect())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_conversation_row_fields() {
        // Just verify the struct fields compile correctly
        let _row = ConversationRow {
            conversation_id: Uuid::new_v4(),
            conversation_type: "direct".to_string(),
            name: Some("Test".to_string()),
            joined_at: Utc::now(),
            left_at: None,
            role: "member".to_string(),
        };
    }

    #[test]
    fn test_message_row_fields() {
        let _row = MessageRow {
            id: Uuid::new_v4(),
            content: "Hello".to_string(),
            content_type: "text".to_string(),
            created_at: Utc::now(),
            updated_at: None,
            reply_to_id: None,
            mentions: vec![],
        };
    }

    #[test]
    fn test_export_metadata_format_version() {
        let metadata = ExportMetadata {
            user_id: Uuid::new_v4(),
            tenant_id: "test".to_string(),
            exported_at: Utc::now(),
            format_version: "1.0".to_string(),
            gdpr_request_id: Uuid::new_v4(),
        };
        assert_eq!(metadata.format_version, "1.0");
    }
}
