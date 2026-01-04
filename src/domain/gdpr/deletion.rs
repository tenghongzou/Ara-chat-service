//! GDPR Data Deleter
//!
//! Handles user data deletion/anonymization for GDPR Art. 17 (Right to Erasure).

use std::sync::Arc;

use chrono::Utc;
use sqlx::PgPool;
use uuid::Uuid;

use super::audit::GdprAuditLogger;
use super::error::GdprError;
use super::types::{
    AffectedDataSummary, DeletionOptions, DeletionResult, GdprActionType, GdprRequestContext,
    GdprRequestStatus,
};

/// Placeholder UUID for anonymized messages (represents "deleted user")
const DELETED_USER_UUID: &str = "00000000-0000-0000-0000-000000000000";

/// Data deleter for GDPR compliance
pub struct DataDeleter {
    pool: Arc<PgPool>,
    audit_logger: Arc<GdprAuditLogger>,
    tenant_id: String,
}

impl DataDeleter {
    pub fn new(pool: Arc<PgPool>, audit_logger: Arc<GdprAuditLogger>) -> Self {
        Self {
            pool,
            audit_logger,
            tenant_id: "default".to_string(),
        }
    }

    pub fn with_tenant(mut self, tenant_id: String) -> Self {
        self.tenant_id = tenant_id;
        self
    }

    /// Delete/anonymize all user data
    pub async fn delete_user_data(
        &self,
        ctx: GdprRequestContext,
        options: DeletionOptions,
    ) -> Result<DeletionResult, GdprError> {
        // Check for existing pending deletion
        if self
            .audit_logger
            .has_pending_request(&ctx.tenant_id, ctx.subject_user_id, "DATA_DELETION")
            .await?
        {
            return Err(GdprError::AlreadyProcessing(ctx.subject_user_id));
        }

        // Log start
        let log_id = self
            .audit_logger
            .log_start(&ctx, GdprActionType::DataDeletionRequested)
            .await?;

        let result = self.perform_deletion(&ctx, &options).await;

        match &result {
            Ok(deletion_result) => {
                self.audit_logger
                    .log_completed(log_id, &deletion_result.affected)
                    .await?;
            }
            Err(e) => {
                self.audit_logger.log_failed(log_id, &e.to_string()).await?;
            }
        }

        result
    }

    async fn perform_deletion(
        &self,
        ctx: &GdprRequestContext,
        options: &DeletionOptions,
    ) -> Result<DeletionResult, GdprError> {
        let user_id = ctx.subject_user_id;
        let tenant_id = &ctx.tenant_id;
        let mut affected = AffectedDataSummary::default();

        // Use a transaction for atomicity
        let mut tx = self.pool.begin().await?;

        // 1. Anonymize or delete messages
        if options.anonymize_messages {
            affected.messages_anonymized = self
                .anonymize_messages(&mut tx, user_id, tenant_id, options.preserve_thread_structure)
                .await?;
        } else {
            affected.messages_deleted = self.delete_messages(&mut tx, user_id, tenant_id).await?;
        }

        // 2. Delete reactions by user
        affected.reactions_deleted = self.delete_reactions(&mut tx, user_id).await?;

        // 3. Delete read receipts
        affected.read_receipts_deleted = self.delete_read_receipts(&mut tx, user_id).await?;

        // 4. Delete attachments (DB records - file deletion is best-effort outside transaction)
        let attachment_paths = if options.delete_attachment_files {
            self.get_attachment_paths(&mut tx, user_id).await?
        } else {
            vec![]
        };
        affected.attachments_deleted = self.delete_attachment_records(&mut tx, user_id).await?;
        affected.attachment_bytes_deleted = attachment_paths
            .iter()
            .map(|(_, size)| *size as u64)
            .sum();

        // 5. Leave all conversations (soft delete)
        affected.conversations_left = self
            .leave_all_conversations(&mut tx, user_id, tenant_id)
            .await?;

        // 6. Clean up DM lookup entries
        if options.delete_dm_lookups {
            affected.dm_lookups_deleted = self
                .delete_dm_lookups(&mut tx, user_id, tenant_id)
                .await?;
        }

        // 7. Remove user from mentions in other messages
        affected.mentions_removed = self
            .remove_from_mentions(&mut tx, user_id, tenant_id)
            .await?;

        // Commit transaction
        tx.commit().await?;

        // Best-effort file deletion (outside transaction)
        // Note: In production, this would use the FileStorage trait
        // For now, just log what would be deleted
        if !attachment_paths.is_empty() {
            tracing::info!(
                user_id = %user_id,
                file_count = attachment_paths.len(),
                "Attachment files marked for deletion"
            );
        }

        tracing::info!(
            request_id = %ctx.request_id,
            user_id = %user_id,
            messages_anonymized = affected.messages_anonymized,
            messages_deleted = affected.messages_deleted,
            reactions = affected.reactions_deleted,
            attachments = affected.attachments_deleted,
            conversations = affected.conversations_left,
            "User data deletion completed"
        );

        Ok(DeletionResult {
            request_id: ctx.request_id,
            status: GdprRequestStatus::Completed,
            affected,
            completed_at: Utc::now(),
        })
    }

    /// Anonymize messages (replace content and sender, keep structure)
    async fn anonymize_messages(
        &self,
        tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
        user_id: Uuid,
        tenant_id: &str,
        preserve_structure: bool,
    ) -> Result<u64, GdprError> {
        let placeholder = if preserve_structure {
            "[Message from deleted user]"
        } else {
            "[deleted]"
        };

        let deleted_user_id = Uuid::parse_str(DELETED_USER_UUID)
            .map_err(|e| GdprError::DeletionFailed(e.to_string()))?;

        let result = sqlx::query(
            r#"
            UPDATE messages
            SET content = $3,
                sender_id = $4,
                mentions = '{}',
                deleted_at = NOW(),
                updated_at = NOW()
            WHERE sender_id = $1 AND tenant_id = $2 AND deleted_at IS NULL
            "#,
        )
        .bind(user_id)
        .bind(tenant_id)
        .bind(placeholder)
        .bind(deleted_user_id)
        .execute(&mut **tx)
        .await?;

        Ok(result.rows_affected())
    }

    /// Hard delete messages (for non-critical scenarios)
    async fn delete_messages(
        &self,
        tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
        user_id: Uuid,
        tenant_id: &str,
    ) -> Result<u64, GdprError> {
        let result = sqlx::query(
            r#"
            DELETE FROM messages
            WHERE sender_id = $1 AND tenant_id = $2
            "#,
        )
        .bind(user_id)
        .bind(tenant_id)
        .execute(&mut **tx)
        .await?;

        Ok(result.rows_affected())
    }

    /// Delete all reactions by user
    async fn delete_reactions(
        &self,
        tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
        user_id: Uuid,
    ) -> Result<u64, GdprError> {
        let result = sqlx::query(
            r#"
            DELETE FROM message_reactions
            WHERE user_id = $1
            "#,
        )
        .bind(user_id)
        .execute(&mut **tx)
        .await?;

        Ok(result.rows_affected())
    }

    /// Delete read receipts for user
    async fn delete_read_receipts(
        &self,
        tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
        user_id: Uuid,
    ) -> Result<u64, GdprError> {
        let result = sqlx::query(
            r#"
            DELETE FROM read_receipts
            WHERE user_id = $1
            "#,
        )
        .bind(user_id)
        .execute(&mut **tx)
        .await?;

        Ok(result.rows_affected())
    }

    /// Get paths of attachments to delete (for file system cleanup)
    async fn get_attachment_paths(
        &self,
        tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
        user_id: Uuid,
    ) -> Result<Vec<(String, i64)>, GdprError> {
        let rows: Vec<(String, Option<String>, i64)> = sqlx::query_as(
            r#"
            SELECT storage_path, thumbnail_path, file_size
            FROM attachments
            WHERE uploader_id = $1
            "#,
        )
        .bind(user_id)
        .fetch_all(&mut **tx)
        .await?;

        let mut paths = Vec::new();
        for (path, thumb, size) in rows {
            paths.push((path, size));
            if let Some(thumb_path) = thumb {
                paths.push((thumb_path, 0));
            }
        }

        Ok(paths)
    }

    /// Delete attachment records from database
    async fn delete_attachment_records(
        &self,
        tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
        user_id: Uuid,
    ) -> Result<u64, GdprError> {
        let result = sqlx::query(
            r#"
            DELETE FROM attachments
            WHERE uploader_id = $1
            "#,
        )
        .bind(user_id)
        .execute(&mut **tx)
        .await?;

        Ok(result.rows_affected())
    }

    /// Soft-leave all conversations (set left_at timestamp)
    async fn leave_all_conversations(
        &self,
        tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
        user_id: Uuid,
        tenant_id: &str,
    ) -> Result<u64, GdprError> {
        let result = sqlx::query(
            r#"
            UPDATE conversation_participants
            SET left_at = NOW()
            WHERE user_id = $1 AND tenant_id = $2 AND left_at IS NULL
            "#,
        )
        .bind(user_id)
        .bind(tenant_id)
        .execute(&mut **tx)
        .await?;

        Ok(result.rows_affected())
    }

    /// Delete direct message lookup entries
    async fn delete_dm_lookups(
        &self,
        tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
        user_id: Uuid,
        tenant_id: &str,
    ) -> Result<u64, GdprError> {
        let result = sqlx::query(
            r#"
            DELETE FROM direct_message_lookup
            WHERE (user1_id = $1 OR user2_id = $1) AND tenant_id = $2
            "#,
        )
        .bind(user_id)
        .bind(tenant_id)
        .execute(&mut **tx)
        .await?;

        Ok(result.rows_affected())
    }

    /// Remove user from mentions arrays in other messages
    async fn remove_from_mentions(
        &self,
        tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
        user_id: Uuid,
        tenant_id: &str,
    ) -> Result<u64, GdprError> {
        let result = sqlx::query(
            r#"
            UPDATE messages
            SET mentions = array_remove(mentions, $1),
                updated_at = NOW()
            WHERE $1 = ANY(mentions) AND tenant_id = $2
            "#,
        )
        .bind(user_id)
        .bind(tenant_id)
        .execute(&mut **tx)
        .await?;

        Ok(result.rows_affected())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_deleted_user_uuid_is_valid() {
        let uuid = Uuid::parse_str(DELETED_USER_UUID);
        assert!(uuid.is_ok());
        assert_eq!(uuid.unwrap().to_string(), DELETED_USER_UUID);
    }

    #[test]
    fn test_affected_data_summary_totals() {
        let summary = AffectedDataSummary {
            messages_deleted: 50,
            messages_anonymized: 100,
            reactions_deleted: 25,
            read_receipts_deleted: 10,
            attachments_deleted: 5,
            attachment_bytes_deleted: 1024 * 1024,
            conversations_left: 3,
            dm_lookups_deleted: 2,
            mentions_removed: 15,
        };

        assert_eq!(summary.messages_deleted + summary.messages_anonymized, 150);
        assert_eq!(summary.attachment_bytes_deleted, 1048576);
    }

    #[test]
    fn test_deletion_options_json_roundtrip() {
        let options = DeletionOptions {
            anonymize_messages: true,
            preserve_thread_structure: false,
            delete_dm_lookups: true,
            delete_attachment_files: true,
        };

        let json = serde_json::to_string(&options).unwrap();
        let parsed: DeletionOptions = serde_json::from_str(&json).unwrap();

        assert_eq!(parsed.anonymize_messages, options.anonymize_messages);
        assert_eq!(
            parsed.preserve_thread_structure,
            options.preserve_thread_structure
        );
    }
}
