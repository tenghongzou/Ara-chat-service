//! GDPR Audit Logger
//!
//! Logs all GDPR-related actions to the database for compliance tracking.

use std::sync::Arc;

use chrono::{DateTime, Utc};
use sqlx::PgPool;
use uuid::Uuid;

use super::error::GdprError;
use super::types::{
    AffectedDataSummary, AuditLogEntry, GdprActionType, GdprRequestContext, GdprRequestStatus,
};

/// Database row for audit log queries
#[derive(Debug, sqlx::FromRow)]
struct AuditLogRow {
    id: Uuid,
    action_type: String,
    subject_user_id: Uuid,
    requester_user_id: Option<Uuid>,
    requester_type: String,
    request_id: Uuid,
    status: String,
    affected_data: Option<serde_json::Value>,
    error_message: Option<String>,
    created_at: DateTime<Utc>,
    completed_at: Option<DateTime<Utc>>,
}

impl From<AuditLogRow> for AuditLogEntry {
    fn from(row: AuditLogRow) -> Self {
        Self {
            id: row.id,
            action_type: row.action_type,
            subject_user_id: row.subject_user_id,
            requester_user_id: row.requester_user_id,
            requester_type: row.requester_type,
            request_id: row.request_id,
            status: row.status,
            affected_data: row.affected_data,
            error_message: row.error_message,
            created_at: row.created_at,
            completed_at: row.completed_at,
        }
    }
}

/// Audit logger for GDPR operations
pub struct GdprAuditLogger {
    pool: Arc<PgPool>,
}

impl GdprAuditLogger {
    pub fn new(pool: Arc<PgPool>) -> Self {
        Self { pool }
    }

    /// Log the start of a GDPR action
    pub async fn log_start(
        &self,
        ctx: &GdprRequestContext,
        action_type: GdprActionType,
    ) -> Result<Uuid, GdprError> {
        let log_id = Uuid::new_v4();

        sqlx::query(
            r#"
            INSERT INTO gdpr_audit_logs (
                id, tenant_id, action_type, subject_user_id,
                requester_user_id, requester_type, request_id,
                request_ip, request_user_agent, status, started_at
            )
            VALUES ($1, $2, $3, $4, $5, $6, $7, $8::inet, $9, 'processing', NOW())
            "#,
        )
        .bind(log_id)
        .bind(&ctx.tenant_id)
        .bind(action_type.as_str())
        .bind(ctx.subject_user_id)
        .bind(ctx.requester_user_id)
        .bind(ctx.requester_type.as_str())
        .bind(ctx.request_id)
        .bind(&ctx.request_ip)
        .bind(&ctx.request_user_agent)
        .execute(self.pool.as_ref())
        .await?;

        tracing::info!(
            log_id = %log_id,
            request_id = %ctx.request_id,
            action = %action_type.as_str(),
            subject_user_id = %ctx.subject_user_id,
            "GDPR action started"
        );

        Ok(log_id)
    }

    /// Log successful completion of a GDPR action
    pub async fn log_completed(
        &self,
        log_id: Uuid,
        affected: &AffectedDataSummary,
    ) -> Result<(), GdprError> {
        let affected_json = serde_json::to_value(affected)?;

        sqlx::query(
            r#"
            UPDATE gdpr_audit_logs
            SET status = 'completed',
                completed_at = NOW(),
                affected_data = $2
            WHERE id = $1
            "#,
        )
        .bind(log_id)
        .bind(affected_json)
        .execute(self.pool.as_ref())
        .await?;

        tracing::info!(
            log_id = %log_id,
            messages_affected = affected.messages_anonymized + affected.messages_deleted,
            attachments_deleted = affected.attachments_deleted,
            "GDPR action completed successfully"
        );

        Ok(())
    }

    /// Log failure of a GDPR action
    pub async fn log_failed(&self, log_id: Uuid, error_message: &str) -> Result<(), GdprError> {
        sqlx::query(
            r#"
            UPDATE gdpr_audit_logs
            SET status = 'failed',
                completed_at = NOW(),
                error_message = $2
            WHERE id = $1
            "#,
        )
        .bind(log_id)
        .bind(error_message)
        .execute(self.pool.as_ref())
        .await?;

        tracing::error!(
            log_id = %log_id,
            error = %error_message,
            "GDPR action failed"
        );

        Ok(())
    }

    /// Get audit logs for a specific user
    pub async fn get_user_audit_logs(
        &self,
        tenant_id: &str,
        user_id: Uuid,
        limit: i64,
    ) -> Result<Vec<AuditLogEntry>, GdprError> {
        let rows: Vec<AuditLogRow> = sqlx::query_as(
            r#"
            SELECT
                id, action_type, subject_user_id, requester_user_id,
                requester_type, request_id, status, affected_data,
                error_message, created_at, completed_at
            FROM gdpr_audit_logs
            WHERE tenant_id = $1 AND subject_user_id = $2
            ORDER BY created_at DESC
            LIMIT $3
            "#,
        )
        .bind(tenant_id)
        .bind(user_id)
        .bind(limit)
        .fetch_all(self.pool.as_ref())
        .await?;

        Ok(rows.into_iter().map(AuditLogEntry::from).collect())
    }

    /// Get a specific audit log by request ID
    pub async fn get_by_request_id(
        &self,
        request_id: Uuid,
    ) -> Result<Option<AuditLogEntry>, GdprError> {
        let row: Option<AuditLogRow> = sqlx::query_as(
            r#"
            SELECT
                id, action_type, subject_user_id, requester_user_id,
                requester_type, request_id, status, affected_data,
                error_message, created_at, completed_at
            FROM gdpr_audit_logs
            WHERE request_id = $1
            ORDER BY created_at DESC
            LIMIT 1
            "#,
        )
        .bind(request_id)
        .fetch_optional(self.pool.as_ref())
        .await?;

        Ok(row.map(AuditLogEntry::from))
    }

    /// Check if there's a pending or processing request for a user
    pub async fn has_pending_request(
        &self,
        tenant_id: &str,
        user_id: Uuid,
        action_prefix: &str,
    ) -> Result<bool, GdprError> {
        let count: (i64,) = sqlx::query_as(
            r#"
            SELECT COUNT(*)
            FROM gdpr_audit_logs
            WHERE tenant_id = $1
              AND subject_user_id = $2
              AND action_type LIKE $3
              AND status IN ('pending', 'processing')
            "#,
        )
        .bind(tenant_id)
        .bind(user_id)
        .bind(format!("{}%", action_prefix))
        .fetch_one(self.pool.as_ref())
        .await?;

        Ok(count.0 > 0)
    }

    /// Get request status
    pub async fn get_request_status(
        &self,
        request_id: Uuid,
    ) -> Result<Option<GdprRequestStatus>, GdprError> {
        let result: Option<(String,)> = sqlx::query_as(
            r#"
            SELECT status
            FROM gdpr_audit_logs
            WHERE request_id = $1
            ORDER BY created_at DESC
            LIMIT 1
            "#,
        )
        .bind(request_id)
        .fetch_optional(self.pool.as_ref())
        .await?;

        Ok(result.map(|(status,)| match status.as_str() {
            "pending" => GdprRequestStatus::Pending,
            "processing" => GdprRequestStatus::Processing,
            "completed" => GdprRequestStatus::Completed,
            "failed" => GdprRequestStatus::Failed,
            _ => GdprRequestStatus::Failed,
        }))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_audit_log_row_conversion() {
        let row = AuditLogRow {
            id: Uuid::new_v4(),
            action_type: "DATA_EXPORT_COMPLETED".to_string(),
            subject_user_id: Uuid::new_v4(),
            requester_user_id: Some(Uuid::new_v4()),
            requester_type: "user".to_string(),
            request_id: Uuid::new_v4(),
            status: "completed".to_string(),
            affected_data: Some(serde_json::json!({"messages_exported": 100})),
            error_message: None,
            created_at: Utc::now(),
            completed_at: Some(Utc::now()),
        };

        let entry: AuditLogEntry = row.into();
        assert_eq!(entry.action_type, "DATA_EXPORT_COMPLETED");
        assert_eq!(entry.status, "completed");
        assert!(entry.error_message.is_none());
    }
}
