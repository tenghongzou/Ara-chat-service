//! Read receipt tracker - manages read receipts and syncs with Redis

use std::sync::Arc;

use chrono::Utc;
use sqlx::PgPool;
use uuid::Uuid;

use crate::redis::{RedisPool, RedisCache};

/// Tracks read receipts for messages
pub struct ReadReceiptTracker {
    postgres: Arc<PgPool>,
    cache: Option<RedisCache>,
}

impl ReadReceiptTracker {
    pub fn new(postgres: Arc<PgPool>, redis: Option<Arc<RedisPool>>) -> Self {
        let cache = redis.map(|r| RedisCache::new(r));
        Self { postgres, cache }
    }

    /// Mark messages as read up to a specific message
    pub async fn mark_read(
        &self,
        conversation_id: Uuid,
        user_id: Uuid,
        message_id: Uuid,
    ) -> Result<ReadReceiptResult, ReadReceiptError> {
        let read_at = Utc::now();

        // Update PostgreSQL
        sqlx::query(
            r#"
            INSERT INTO read_receipts (conversation_id, user_id, last_read_message_id, last_read_at)
            VALUES ($1, $2, $3, $4)
            ON CONFLICT (conversation_id, user_id)
            DO UPDATE SET last_read_message_id = $3, last_read_at = $4
            WHERE read_receipts.last_read_at < $4
            "#,
        )
        .bind(conversation_id)
        .bind(user_id)
        .bind(message_id)
        .bind(read_at)
        .execute(self.postgres.as_ref())
        .await
        .map_err(|e| ReadReceiptError::Database(e.to_string()))?;

        // Also update participant's last_read in conversation_participants
        sqlx::query(
            r#"
            UPDATE conversation_participants
            SET last_read_message_id = $1, last_read_at = $2
            WHERE conversation_id = $3 AND user_id = $4
            "#,
        )
        .bind(message_id)
        .bind(read_at)
        .bind(conversation_id)
        .bind(user_id)
        .execute(self.postgres.as_ref())
        .await
        .map_err(|e| ReadReceiptError::Database(e.to_string()))?;

        // Reset unread count in Redis
        let reset_count = if let Some(ref cache) = self.cache {
            cache.reset_unread(user_id, conversation_id).await
                .unwrap_or(0)
        } else {
            0
        };

        Ok(ReadReceiptResult {
            conversation_id,
            user_id,
            message_id,
            read_at: read_at.timestamp_millis(),
            reset_unread_count: reset_count,
        })
    }

    /// Get last read message ID for a user in a conversation
    pub async fn get_last_read(
        &self,
        conversation_id: Uuid,
        user_id: Uuid,
    ) -> Result<Option<(Uuid, i64)>, ReadReceiptError> {
        let row: Option<(Uuid, chrono::DateTime<Utc>)> = sqlx::query_as(
            r#"
            SELECT last_read_message_id, last_read_at
            FROM read_receipts
            WHERE conversation_id = $1 AND user_id = $2
            "#,
        )
        .bind(conversation_id)
        .bind(user_id)
        .fetch_optional(self.postgres.as_ref())
        .await
        .map_err(|e| ReadReceiptError::Database(e.to_string()))?;

        Ok(row.map(|(id, at)| (id, at.timestamp_millis())))
    }

    /// Get read status for all participants in a conversation
    pub async fn get_conversation_read_status(
        &self,
        conversation_id: Uuid,
    ) -> Result<Vec<ParticipantReadStatus>, ReadReceiptError> {
        let rows: Vec<(Uuid, Option<Uuid>, Option<chrono::DateTime<Utc>>)> = sqlx::query_as(
            r#"
            SELECT user_id, last_read_message_id, last_read_at
            FROM conversation_participants
            WHERE conversation_id = $1 AND left_at IS NULL
            "#,
        )
        .bind(conversation_id)
        .fetch_all(self.postgres.as_ref())
        .await
        .map_err(|e| ReadReceiptError::Database(e.to_string()))?;

        Ok(rows.into_iter().map(|(user_id, msg_id, read_at)| {
            ParticipantReadStatus {
                user_id,
                last_read_message_id: msg_id,
                last_read_at: read_at.map(|t| t.timestamp_millis()),
            }
        }).collect())
    }

    /// Increment unread count when a message is sent
    pub async fn increment_unread(
        &self,
        user_id: Uuid,
        conversation_id: Uuid,
    ) -> Result<u64, ReadReceiptError> {
        if let Some(ref cache) = self.cache {
            cache.increment_unread(user_id, conversation_id).await
                .map_err(|e| ReadReceiptError::Redis(e.to_string()))
        } else {
            Ok(0)
        }
    }

    /// Get unread count for a user in a conversation
    pub async fn get_unread_count(
        &self,
        user_id: Uuid,
        conversation_id: Uuid,
    ) -> Result<u64, ReadReceiptError> {
        if let Some(ref cache) = self.cache {
            cache.get_unread(user_id, conversation_id).await
                .map_err(|e| ReadReceiptError::Redis(e.to_string()))
        } else {
            Ok(0)
        }
    }

    /// Get total unread count for a user
    pub async fn get_total_unread(&self, user_id: Uuid) -> Result<u64, ReadReceiptError> {
        if let Some(ref cache) = self.cache {
            cache.get_total_unread(user_id).await
                .map_err(|e| ReadReceiptError::Redis(e.to_string()))
        } else {
            Ok(0)
        }
    }
}

/// Result of marking messages as read
pub struct ReadReceiptResult {
    pub conversation_id: Uuid,
    pub user_id: Uuid,
    pub message_id: Uuid,
    pub read_at: i64,
    pub reset_unread_count: u64,
}

/// Read status for a participant
pub struct ParticipantReadStatus {
    pub user_id: Uuid,
    pub last_read_message_id: Option<Uuid>,
    pub last_read_at: Option<i64>,
}

#[derive(Debug, thiserror::Error)]
pub enum ReadReceiptError {
    #[error("Database error: {0}")]
    Database(String),

    #[error("Redis error: {0}")]
    Redis(String),
}
