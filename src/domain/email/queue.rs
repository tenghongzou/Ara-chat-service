//! Email queue management

use std::sync::Arc;

use chrono::{Duration, Utc};
use sqlx::PgPool;
use uuid::Uuid;

use super::error::EmailError;
use super::types::{EmailPriority, EmailStatus, EmailType, QueuedEmail, QueuedEmailRow};

/// Email queue for delayed and batched sending
pub struct EmailQueue {
    pool: Arc<PgPool>,
    tenant_id: String,
    delay_seconds: i64,
    batch_window_seconds: i64,
    max_batch_size: usize,
    max_emails_per_hour: u32,
}

impl EmailQueue {
    /// Create a new email queue
    pub fn new(
        pool: Arc<PgPool>,
        delay_seconds: u32,
        batch_window_seconds: u32,
        max_batch_size: usize,
        max_emails_per_hour: u32,
    ) -> Self {
        Self {
            pool,
            tenant_id: "default".to_string(),
            delay_seconds: delay_seconds as i64,
            batch_window_seconds: batch_window_seconds as i64,
            max_batch_size,
            max_emails_per_hour,
        }
    }

    /// Set tenant ID for multi-tenant isolation
    pub fn with_tenant(mut self, tenant_id: String) -> Self {
        self.tenant_id = tenant_id;
        self
    }

    /// Enqueue a notification (may batch with existing)
    pub async fn enqueue(
        &self,
        user_id: Uuid,
        email_type: EmailType,
        conversation_id: Uuid,
        message_id: Uuid,
        sender_id: Uuid,
        content_preview: Option<String>,
        priority: EmailPriority,
    ) -> Result<Uuid, EmailError> {
        let send_after = Utc::now() + Duration::seconds(self.delay_seconds);

        // Try to batch with existing pending email for same user/conversation
        let existing: Option<(Uuid, Vec<Uuid>, Vec<Uuid>, Option<Vec<String>>)> = sqlx::query_as(
            r#"
            SELECT id, message_ids, sender_ids, content_previews
            FROM email_queue
            WHERE user_id = $1 AND tenant_id = $2 AND conversation_id = $3
              AND status = 'pending'
              AND created_at > NOW() - INTERVAL '1 second' * $4
            ORDER BY created_at DESC
            LIMIT 1
            "#,
        )
        .bind(user_id)
        .bind(&self.tenant_id)
        .bind(conversation_id)
        .bind(self.batch_window_seconds)
        .fetch_optional(self.pool.as_ref())
        .await?;

        if let Some((id, mut message_ids, mut sender_ids, previews)) = existing {
            // Batch with existing if under limit
            if message_ids.len() < self.max_batch_size {
                message_ids.push(message_id);
                sender_ids.push(sender_id);

                let mut new_previews = previews.unwrap_or_default();
                if let Some(preview) = content_preview {
                    new_previews.push(preview);
                }

                sqlx::query(
                    r#"
                    UPDATE email_queue
                    SET message_ids = $2, sender_ids = $3, content_previews = $4,
                        send_after = $5
                    WHERE id = $1
                    "#,
                )
                .bind(id)
                .bind(&message_ids)
                .bind(&sender_ids)
                .bind(&new_previews)
                .bind(send_after)
                .execute(self.pool.as_ref())
                .await?;

                tracing::debug!(
                    email_id = %id,
                    user_id = %user_id,
                    batch_size = message_ids.len(),
                    "Batched email notification"
                );

                return Ok(id);
            }
        }

        // Create new queue entry
        let id = Uuid::new_v4();
        let previews: Vec<String> = content_preview.into_iter().collect();

        sqlx::query(
            r#"
            INSERT INTO email_queue
            (id, user_id, tenant_id, email_type, conversation_id, message_ids,
             sender_ids, content_previews, priority, status, send_after)
            VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, 'pending', $10)
            "#,
        )
        .bind(id)
        .bind(user_id)
        .bind(&self.tenant_id)
        .bind(email_type.as_str())
        .bind(conversation_id)
        .bind(&[message_id])
        .bind(&[sender_id])
        .bind(&previews)
        .bind(priority.as_str())
        .bind(send_after)
        .execute(self.pool.as_ref())
        .await?;

        tracing::debug!(
            email_id = %id,
            user_id = %user_id,
            email_type = ?email_type,
            "Enqueued email notification"
        );

        Ok(id)
    }

    /// Cancel pending emails for a user (e.g., when they reconnect)
    pub async fn cancel_pending(&self, user_id: Uuid) -> Result<usize, EmailError> {
        let result = sqlx::query(
            r#"
            UPDATE email_queue
            SET status = 'cancelled'
            WHERE user_id = $1 AND tenant_id = $2 AND status = 'pending'
            "#,
        )
        .bind(user_id)
        .bind(&self.tenant_id)
        .execute(self.pool.as_ref())
        .await?;

        let count = result.rows_affected() as usize;
        if count > 0 {
            tracing::debug!(
                user_id = %user_id,
                cancelled = count,
                "Cancelled pending email notifications"
            );
        }

        Ok(count)
    }

    /// Get emails ready to send
    pub async fn get_ready_emails(&self, limit: i32) -> Result<Vec<QueuedEmail>, EmailError> {
        let rows: Vec<QueuedEmailRow> = sqlx::query_as(
            r#"
            SELECT id, user_id, email_type, conversation_id, message_ids,
                   sender_ids, content_previews, priority, status,
                   scheduled_at, send_after, retry_count
            FROM email_queue
            WHERE tenant_id = $1 AND status = 'pending' AND send_after <= NOW()
            ORDER BY
                CASE priority
                    WHEN 'high' THEN 0
                    WHEN 'normal' THEN 1
                    WHEN 'low' THEN 2
                END,
                send_after ASC
            LIMIT $2
            FOR UPDATE SKIP LOCKED
            "#,
        )
        .bind(&self.tenant_id)
        .bind(limit)
        .fetch_all(self.pool.as_ref())
        .await?;

        Ok(rows.into_iter().map(|r| r.into()).collect())
    }

    /// Check rate limit for user
    pub async fn check_rate_limit(&self, user_id: Uuid) -> Result<bool, EmailError> {
        let hour_bucket = Utc::now()
            .format("%Y-%m-%d %H:00:00")
            .to_string();

        let count: Option<(i16,)> = sqlx::query_as(
            r#"
            SELECT email_count FROM email_rate_limits
            WHERE user_id = $1 AND tenant_id = $2 AND hour_bucket = $3::timestamptz
            "#,
        )
        .bind(user_id)
        .bind(&self.tenant_id)
        .bind(&hour_bucket)
        .fetch_optional(self.pool.as_ref())
        .await?;

        Ok(count.map(|(c,)| c).unwrap_or(0) < self.max_emails_per_hour as i16)
    }

    /// Increment rate limit counter
    pub async fn increment_rate_limit(&self, user_id: Uuid) -> Result<(), EmailError> {
        let hour_bucket = Utc::now()
            .format("%Y-%m-%d %H:00:00")
            .to_string();

        sqlx::query(
            r#"
            INSERT INTO email_rate_limits (user_id, tenant_id, hour_bucket, email_count)
            VALUES ($1, $2, $3::timestamptz, 1)
            ON CONFLICT (user_id, tenant_id, hour_bucket)
            DO UPDATE SET email_count = email_rate_limits.email_count + 1
            "#,
        )
        .bind(user_id)
        .bind(&self.tenant_id)
        .bind(&hour_bucket)
        .execute(self.pool.as_ref())
        .await?;

        Ok(())
    }

    /// Mark email as processing
    pub async fn mark_processing(&self, id: Uuid) -> Result<(), EmailError> {
        sqlx::query(r#"UPDATE email_queue SET status = 'processing' WHERE id = $1"#)
            .bind(id)
            .execute(self.pool.as_ref())
            .await?;
        Ok(())
    }

    /// Mark email as sent
    pub async fn mark_sent(&self, id: Uuid) -> Result<(), EmailError> {
        sqlx::query(r#"UPDATE email_queue SET status = 'sent', sent_at = NOW() WHERE id = $1"#)
            .bind(id)
            .execute(self.pool.as_ref())
            .await?;
        Ok(())
    }

    /// Mark email as failed
    pub async fn mark_failed(&self, id: Uuid, error: &str) -> Result<(), EmailError> {
        sqlx::query(
            r#"
            UPDATE email_queue
            SET status = 'failed', error = $2, retry_count = retry_count + 1
            WHERE id = $1
            "#,
        )
        .bind(id)
        .bind(error)
        .execute(self.pool.as_ref())
        .await?;
        Ok(())
    }

    /// Requeue failed emails for retry (up to max retries)
    pub async fn requeue_failed(&self, max_retries: i16) -> Result<usize, EmailError> {
        let result = sqlx::query(
            r#"
            UPDATE email_queue
            SET status = 'pending', send_after = NOW() + INTERVAL '5 minutes'
            WHERE tenant_id = $1 AND status = 'failed' AND retry_count < $2
            "#,
        )
        .bind(&self.tenant_id)
        .bind(max_retries)
        .execute(self.pool.as_ref())
        .await?;

        Ok(result.rows_affected() as usize)
    }

    /// Clean up old completed/cancelled emails
    pub async fn cleanup_old(&self, days: i32) -> Result<usize, EmailError> {
        let result = sqlx::query(
            r#"
            DELETE FROM email_queue
            WHERE tenant_id = $1
              AND status IN ('sent', 'cancelled', 'failed')
              AND created_at < NOW() - INTERVAL '1 day' * $2
            "#,
        )
        .bind(&self.tenant_id)
        .bind(days)
        .execute(self.pool.as_ref())
        .await?;

        Ok(result.rows_affected() as usize)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_queue_creation() {
        // Note: This test doesn't actually test DB operations
        // as it requires a database connection
        let _queue = EmailQueue::new(
            Arc::new(PgPool::connect_lazy("postgres://fake").unwrap()),
            120,
            300,
            10,
            5,
        );
    }
}
