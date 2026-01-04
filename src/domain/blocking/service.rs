//! User blocking service

use std::sync::Arc;

use chrono::{DateTime, Utc};
use sqlx::PgPool;
use thiserror::Error;
use uuid::Uuid;

use super::types::BlockedUserInfo;

/// Errors that can occur in blocking operations
#[derive(Debug, Error)]
pub enum BlockingError {
    #[error("Database error: {0}")]
    Database(#[from] sqlx::Error),

    #[error("Cannot block yourself")]
    CannotBlockSelf,

    #[error("User not found")]
    UserNotFound,

    #[error("Block not found")]
    BlockNotFound,

    #[error("Already blocked")]
    AlreadyBlocked,
}

/// Service for managing user blocks
#[derive(Clone)]
pub struct BlockingService {
    pool: Arc<PgPool>,
    tenant_id: String,
}

impl BlockingService {
    /// Create a new blocking service
    pub fn new(pool: Arc<PgPool>, tenant_id: String) -> Self {
        Self { pool, tenant_id }
    }

    /// Block a user
    ///
    /// Returns the timestamp when the block was created.
    pub async fn block_user(
        &self,
        blocker_id: Uuid,
        blocked_user_id: Uuid,
        reason: Option<String>,
    ) -> Result<DateTime<Utc>, BlockingError> {
        if blocker_id == blocked_user_id {
            return Err(BlockingError::CannotBlockSelf);
        }

        let now = Utc::now();

        sqlx::query(
            r#"
            INSERT INTO user_blocks (blocker_id, blocked_user_id, tenant_id, blocked_at, reason)
            VALUES ($1, $2, $3, $4, $5)
            ON CONFLICT (blocker_id, blocked_user_id, tenant_id) DO UPDATE
            SET blocked_at = $4, reason = $5
            "#,
        )
        .bind(blocker_id)
        .bind(blocked_user_id)
        .bind(&self.tenant_id)
        .bind(now)
        .bind(reason)
        .execute(self.pool.as_ref())
        .await?;

        tracing::info!(
            blocker_id = %blocker_id,
            blocked_user_id = %blocked_user_id,
            "User blocked"
        );

        Ok(now)
    }

    /// Unblock a user
    pub async fn unblock_user(
        &self,
        blocker_id: Uuid,
        blocked_user_id: Uuid,
    ) -> Result<(), BlockingError> {
        let result = sqlx::query(
            r#"
            DELETE FROM user_blocks
            WHERE blocker_id = $1 AND blocked_user_id = $2 AND tenant_id = $3
            "#,
        )
        .bind(blocker_id)
        .bind(blocked_user_id)
        .bind(&self.tenant_id)
        .execute(self.pool.as_ref())
        .await?;

        if result.rows_affected() == 0 {
            return Err(BlockingError::BlockNotFound);
        }

        tracing::info!(
            blocker_id = %blocker_id,
            blocked_user_id = %blocked_user_id,
            "User unblocked"
        );

        Ok(())
    }

    /// Check if blocker has blocked blocked_user
    pub async fn is_blocked(
        &self,
        blocker_id: Uuid,
        blocked_user_id: Uuid,
    ) -> Result<bool, BlockingError> {
        let row: Option<(i64,)> = sqlx::query_as(
            r#"
            SELECT 1
            FROM user_blocks
            WHERE blocker_id = $1 AND blocked_user_id = $2 AND tenant_id = $3
            "#,
        )
        .bind(blocker_id)
        .bind(blocked_user_id)
        .bind(&self.tenant_id)
        .fetch_optional(self.pool.as_ref())
        .await?;

        Ok(row.is_some())
    }

    /// Check if either user has blocked the other (for DM restrictions)
    ///
    /// Returns true if:
    /// - user_a has blocked user_b, OR
    /// - user_b has blocked user_a
    pub async fn is_mutually_blocked(
        &self,
        user_a: Uuid,
        user_b: Uuid,
    ) -> Result<bool, BlockingError> {
        let row: Option<(i64,)> = sqlx::query_as(
            r#"
            SELECT 1
            FROM user_blocks
            WHERE tenant_id = $1
              AND (
                  (blocker_id = $2 AND blocked_user_id = $3)
                  OR (blocker_id = $3 AND blocked_user_id = $2)
              )
            LIMIT 1
            "#,
        )
        .bind(&self.tenant_id)
        .bind(user_a)
        .bind(user_b)
        .fetch_optional(self.pool.as_ref())
        .await?;

        Ok(row.is_some())
    }

    /// Get list of users blocked by a user
    pub async fn get_blocked_users(
        &self,
        user_id: Uuid,
    ) -> Result<Vec<BlockedUserInfo>, BlockingError> {
        let rows: Vec<(Uuid, DateTime<Utc>)> = sqlx::query_as(
            r#"
            SELECT blocked_user_id, blocked_at
            FROM user_blocks
            WHERE blocker_id = $1 AND tenant_id = $2
            ORDER BY blocked_at DESC
            "#,
        )
        .bind(user_id)
        .bind(&self.tenant_id)
        .fetch_all(self.pool.as_ref())
        .await?;

        Ok(rows
            .into_iter()
            .map(|(user_id, blocked_at)| BlockedUserInfo {
                user_id,
                blocked_at: blocked_at.timestamp_millis(),
            })
            .collect())
    }

    /// Get list of users who have blocked this user
    pub async fn get_blocked_by(&self, user_id: Uuid) -> Result<Vec<Uuid>, BlockingError> {
        let rows: Vec<(Uuid,)> = sqlx::query_as(
            r#"
            SELECT blocker_id
            FROM user_blocks
            WHERE blocked_user_id = $1 AND tenant_id = $2
            "#,
        )
        .bind(user_id)
        .bind(&self.tenant_id)
        .fetch_all(self.pool.as_ref())
        .await?;

        Ok(rows.into_iter().map(|(id,)| id).collect())
    }

    /// Get all blocked user IDs for filtering (both directions)
    ///
    /// Returns users that:
    /// - The user has blocked
    /// - Have blocked the user
    ///
    /// This is used for filtering message delivery and presence updates.
    pub async fn get_all_blocked_user_ids(
        &self,
        user_id: Uuid,
    ) -> Result<Vec<Uuid>, BlockingError> {
        let rows: Vec<(Uuid,)> = sqlx::query_as(
            r#"
            SELECT DISTINCT
                CASE
                    WHEN blocker_id = $1 THEN blocked_user_id
                    ELSE blocker_id
                END as other_user_id
            FROM user_blocks
            WHERE tenant_id = $2
              AND (blocker_id = $1 OR blocked_user_id = $1)
            "#,
        )
        .bind(user_id)
        .bind(&self.tenant_id)
        .fetch_all(self.pool.as_ref())
        .await?;

        Ok(rows.into_iter().map(|(id,)| id).collect())
    }

    /// Get block count for a user
    pub async fn get_block_count(&self, user_id: Uuid) -> Result<usize, BlockingError> {
        let row: (i64,) = sqlx::query_as(
            r#"
            SELECT COUNT(*)
            FROM user_blocks
            WHERE blocker_id = $1 AND tenant_id = $2
            "#,
        )
        .bind(user_id)
        .bind(&self.tenant_id)
        .fetch_one(self.pool.as_ref())
        .await?;

        Ok(row.0 as usize)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_blocking_error_display() {
        assert_eq!(
            BlockingError::CannotBlockSelf.to_string(),
            "Cannot block yourself"
        );
        assert_eq!(BlockingError::BlockNotFound.to_string(), "Block not found");
        assert_eq!(BlockingError::AlreadyBlocked.to_string(), "Already blocked");
    }

    #[test]
    fn test_blocking_error_debug() {
        let err = BlockingError::CannotBlockSelf;
        assert!(format!("{:?}", err).contains("CannotBlockSelf"));
    }
}
