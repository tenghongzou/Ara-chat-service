//! Reaction service - manages emoji reactions

use std::sync::Arc;
use std::collections::HashMap;

use chrono::{NaiveDate, Utc};
use sqlx::PgPool;
use uuid::Uuid;

use crate::message::ReactionAction;

/// Service for managing emoji reactions
pub struct ReactionService {
    pool: Arc<PgPool>,
}

impl ReactionService {
    pub fn new(pool: Arc<PgPool>) -> Self {
        Self { pool }
    }

    /// Get message partition key from message_id
    async fn get_message_partition_key(
        &self,
        message_id: Uuid,
    ) -> Result<Option<NaiveDate>, ReactionError> {
        let row: Option<(NaiveDate,)> = sqlx::query_as(
            r#"
            SELECT partition_key
            FROM messages
            WHERE id = $1
            LIMIT 1
            "#,
        )
        .bind(message_id)
        .fetch_optional(self.pool.as_ref())
        .await
        .map_err(|e| ReactionError::Database(e.to_string()))?;

        Ok(row.map(|(pk,)| pk))
    }

    /// Toggle a reaction on a message
    pub async fn toggle_reaction(
        &self,
        message_id: Uuid,
        user_id: Uuid,
        emoji: &str,
    ) -> Result<ReactionAction, ReactionError> {
        // Validate emoji (basic check for length)
        if emoji.is_empty() || emoji.len() > 32 {
            return Err(ReactionError::InvalidEmoji);
        }

        // Get message partition key
        let partition_key = self.get_message_partition_key(message_id).await?
            .ok_or(ReactionError::MessageNotFound)?;

        // Check if reaction already exists
        let exists = self.has_reaction_with_partition(message_id, user_id, emoji, partition_key).await?;

        if exists {
            self.remove_reaction_with_partition(message_id, user_id, emoji, partition_key).await?;
            Ok(ReactionAction::Remove)
        } else {
            self.add_reaction_with_partition(message_id, user_id, emoji, partition_key).await?;
            Ok(ReactionAction::Add)
        }
    }

    /// Check if a user has reacted with a specific emoji (using partition key)
    async fn has_reaction_with_partition(
        &self,
        message_id: Uuid,
        user_id: Uuid,
        emoji: &str,
        partition_key: NaiveDate,
    ) -> Result<bool, ReactionError> {
        let exists: Option<(i64,)> = sqlx::query_as(
            r#"
            SELECT 1
            FROM message_reactions
            WHERE message_id = $1
              AND user_id = $2
              AND emoji = $3
              AND message_partition_key = $4
            "#,
        )
        .bind(message_id)
        .bind(user_id)
        .bind(emoji)
        .bind(partition_key)
        .fetch_optional(self.pool.as_ref())
        .await
        .map_err(|e| ReactionError::Database(e.to_string()))?;

        Ok(exists.is_some())
    }

    /// Check if a user has reacted with a specific emoji (scans all partitions)
    pub async fn has_reaction(
        &self,
        message_id: Uuid,
        user_id: Uuid,
        emoji: &str,
    ) -> Result<bool, ReactionError> {
        let exists: Option<(i64,)> = sqlx::query_as(
            r#"
            SELECT 1
            FROM message_reactions
            WHERE message_id = $1
              AND user_id = $2
              AND emoji = $3
            LIMIT 1
            "#,
        )
        .bind(message_id)
        .bind(user_id)
        .bind(emoji)
        .fetch_optional(self.pool.as_ref())
        .await
        .map_err(|e| ReactionError::Database(e.to_string()))?;

        Ok(exists.is_some())
    }

    /// Add a reaction with known partition key
    async fn add_reaction_with_partition(
        &self,
        message_id: Uuid,
        user_id: Uuid,
        emoji: &str,
        partition_key: NaiveDate,
    ) -> Result<(), ReactionError> {
        sqlx::query(
            r#"
            INSERT INTO message_reactions (message_id, message_partition_key, user_id, emoji)
            VALUES ($1, $2, $3, $4)
            ON CONFLICT (message_id, message_partition_key, user_id, emoji) DO NOTHING
            "#,
        )
        .bind(message_id)
        .bind(partition_key)
        .bind(user_id)
        .bind(emoji)
        .execute(self.pool.as_ref())
        .await
        .map_err(|e| ReactionError::Database(e.to_string()))?;

        tracing::debug!(
            message_id = %message_id,
            user_id = %user_id,
            emoji = %emoji,
            "Reaction added"
        );

        Ok(())
    }

    /// Remove a reaction with known partition key
    async fn remove_reaction_with_partition(
        &self,
        message_id: Uuid,
        user_id: Uuid,
        emoji: &str,
        partition_key: NaiveDate,
    ) -> Result<(), ReactionError> {
        sqlx::query(
            r#"
            DELETE FROM message_reactions
            WHERE message_id = $1
              AND message_partition_key = $2
              AND user_id = $3
              AND emoji = $4
            "#,
        )
        .bind(message_id)
        .bind(partition_key)
        .bind(user_id)
        .bind(emoji)
        .execute(self.pool.as_ref())
        .await
        .map_err(|e| ReactionError::Database(e.to_string()))?;

        tracing::debug!(
            message_id = %message_id,
            user_id = %user_id,
            emoji = %emoji,
            "Reaction removed"
        );

        Ok(())
    }

    /// Get all reactions for a message
    pub async fn get_reactions(
        &self,
        message_id: Uuid,
    ) -> Result<Vec<(String, Vec<Uuid>)>, ReactionError> {
        // Query all reactions for this message (across partitions for flexibility)
        let rows: Vec<(String, Uuid)> = sqlx::query_as(
            r#"
            SELECT emoji, user_id
            FROM message_reactions
            WHERE message_id = $1
            ORDER BY emoji, created_at
            "#,
        )
        .bind(message_id)
        .fetch_all(self.pool.as_ref())
        .await
        .map_err(|e| ReactionError::Database(e.to_string()))?;

        // Group by emoji
        let mut grouped: HashMap<String, Vec<Uuid>> = HashMap::new();
        for (emoji, user_id) in rows {
            grouped.entry(emoji).or_default().push(user_id);
        }

        Ok(grouped.into_iter().collect())
    }

    /// Get reactions summary for a message (emoji -> count)
    pub async fn get_reaction_counts(
        &self,
        message_id: Uuid,
    ) -> Result<HashMap<String, i64>, ReactionError> {
        let rows: Vec<(String, i64)> = sqlx::query_as(
            r#"
            SELECT emoji, COUNT(*) as count
            FROM message_reactions
            WHERE message_id = $1
            GROUP BY emoji
            "#,
        )
        .bind(message_id)
        .fetch_all(self.pool.as_ref())
        .await
        .map_err(|e| ReactionError::Database(e.to_string()))?;

        Ok(rows.into_iter().collect())
    }

    /// Get user's reactions on a message
    pub async fn get_user_reactions(
        &self,
        message_id: Uuid,
        user_id: Uuid,
    ) -> Result<Vec<String>, ReactionError> {
        let rows: Vec<(String,)> = sqlx::query_as(
            r#"
            SELECT emoji
            FROM message_reactions
            WHERE message_id = $1 AND user_id = $2
            ORDER BY created_at
            "#,
        )
        .bind(message_id)
        .bind(user_id)
        .fetch_all(self.pool.as_ref())
        .await
        .map_err(|e| ReactionError::Database(e.to_string()))?;

        Ok(rows.into_iter().map(|(e,)| e).collect())
    }

    /// Get reaction summary with user's own reactions marked
    pub async fn get_reactions_with_user(
        &self,
        message_id: Uuid,
        user_id: Uuid,
    ) -> Result<Vec<ReactionSummary>, ReactionError> {
        let rows: Vec<(String, Uuid)> = sqlx::query_as(
            r#"
            SELECT emoji, user_id
            FROM message_reactions
            WHERE message_id = $1
            ORDER BY emoji, created_at
            "#,
        )
        .bind(message_id)
        .fetch_all(self.pool.as_ref())
        .await
        .map_err(|e| ReactionError::Database(e.to_string()))?;

        // Group by emoji and track user's reactions
        let mut grouped: HashMap<String, (Vec<Uuid>, bool)> = HashMap::new();
        for (emoji, uid) in rows {
            let entry = grouped.entry(emoji).or_insert_with(|| (Vec::new(), false));
            if uid == user_id {
                entry.1 = true;
            }
            entry.0.push(uid);
        }

        Ok(grouped.into_iter().map(|(emoji, (users, reacted))| {
            ReactionSummary {
                emoji,
                count: users.len() as u32,
                user_reacted: reacted,
                users,
            }
        }).collect())
    }

    /// Get reactions for multiple messages at once (batch fetch)
    pub async fn get_reactions_batch(
        &self,
        message_ids: &[Uuid],
    ) -> Result<HashMap<Uuid, Vec<(String, Vec<Uuid>)>>, ReactionError> {
        if message_ids.is_empty() {
            return Ok(HashMap::new());
        }

        let rows: Vec<(Uuid, String, Uuid)> = sqlx::query_as(
            r#"
            SELECT message_id, emoji, user_id
            FROM message_reactions
            WHERE message_id = ANY($1)
            ORDER BY message_id, emoji, created_at
            "#,
        )
        .bind(message_ids)
        .fetch_all(self.pool.as_ref())
        .await
        .map_err(|e| ReactionError::Database(e.to_string()))?;

        // Group by message_id, then by emoji
        let mut result: HashMap<Uuid, HashMap<String, Vec<Uuid>>> = HashMap::new();
        for (msg_id, emoji, user_id) in rows {
            result
                .entry(msg_id)
                .or_default()
                .entry(emoji)
                .or_default()
                .push(user_id);
        }

        Ok(result.into_iter().map(|(msg_id, reactions)| {
            (msg_id, reactions.into_iter().collect())
        }).collect())
    }

    /// Get reaction counts for multiple messages at once
    pub async fn get_reaction_counts_batch(
        &self,
        message_ids: &[Uuid],
    ) -> Result<HashMap<Uuid, HashMap<String, i64>>, ReactionError> {
        if message_ids.is_empty() {
            return Ok(HashMap::new());
        }

        let rows: Vec<(Uuid, String, i64)> = sqlx::query_as(
            r#"
            SELECT message_id, emoji, COUNT(*) as count
            FROM message_reactions
            WHERE message_id = ANY($1)
            GROUP BY message_id, emoji
            "#,
        )
        .bind(message_ids)
        .fetch_all(self.pool.as_ref())
        .await
        .map_err(|e| ReactionError::Database(e.to_string()))?;

        let mut result: HashMap<Uuid, HashMap<String, i64>> = HashMap::new();
        for (msg_id, emoji, count) in rows {
            result.entry(msg_id).or_default().insert(emoji, count);
        }

        Ok(result)
    }
}

/// Summary of reactions on a message
#[derive(Debug, Clone)]
pub struct ReactionSummary {
    pub emoji: String,
    pub count: u32,
    pub user_reacted: bool,
    pub users: Vec<Uuid>,
}

#[derive(Debug, thiserror::Error)]
pub enum ReactionError {
    #[error("Database error: {0}")]
    Database(String),

    #[error("Invalid emoji")]
    InvalidEmoji,

    #[error("Message not found")]
    MessageNotFound,
}
