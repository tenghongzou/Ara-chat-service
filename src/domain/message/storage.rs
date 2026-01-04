//! Message storage - persistence layer for chat messages

use std::sync::Arc;

use chrono::{DateTime, Utc};
use sqlx::{FromRow, PgPool, Row};
use uuid::Uuid;

use super::types::{ChatMessage, ContentType, ForwardedFrom, ReplyContext, ThreadInfo};

/// Message row from database
#[derive(Debug, FromRow)]
struct MessageRow {
    id: Uuid,
    conversation_id: Uuid,
    sender_id: Uuid,
    content: String,
    content_type: String,
    created_at: DateTime<Utc>,
    updated_at: Option<DateTime<Utc>>,
    deleted_at: Option<DateTime<Utc>>,
    reply_to_id: Option<Uuid>,
    mentions: Vec<Uuid>,
    client_message_id: Option<String>,
    // Forwarding metadata
    forwarded_from_message_id: Option<Uuid>,
    forwarded_from_sender_id: Option<Uuid>,
    forwarded_from_conversation_id: Option<Uuid>,
}

impl MessageRow {
    fn into_chat_message(self) -> ChatMessage {
        // Build forwarded_from if all three fields are present
        let forwarded_from = match (
            self.forwarded_from_message_id,
            self.forwarded_from_sender_id,
            self.forwarded_from_conversation_id,
        ) {
            (Some(message_id), Some(sender_id), Some(conversation_id)) => Some(ForwardedFrom {
                message_id,
                sender_id,
                conversation_id,
            }),
            _ => None,
        };

        ChatMessage {
            id: self.id,
            conversation_id: self.conversation_id,
            sender_id: self.sender_id,
            content: self.content,
            content_type: match self.content_type.as_str() {
                "image" => ContentType::Image,
                "file" => ContentType::File,
                "system" => ContentType::System,
                _ => ContentType::Text,
            },
            created_at: self.created_at.timestamp_millis(),
            updated_at: self.updated_at.map(|t| t.timestamp_millis()),
            reply_to_id: self.reply_to_id,
            reply_context: None,  // Loaded separately when needed
            thread_info: None,    // Loaded separately when needed
            mentions: self.mentions,
            reactions: Default::default(), // Loaded separately
            recalled_at: self.deleted_at.map(|t| t.timestamp_millis()),
            pinned_at: None,      // Loaded separately when needed
            pinned_by: None,      // Loaded separately when needed
            forwarded_from,
        }
    }
}

/// Pin info row from database
#[derive(Debug, FromRow)]
struct PinInfoRow {
    pinned_at: DateTime<Utc>,
    pinned_by: Uuid,
}

/// Message storage backend
pub struct MessageStorage {
    pool: Arc<PgPool>,
    tenant_id: String,
}

impl MessageStorage {
    pub fn new(pool: Arc<PgPool>) -> Self {
        Self {
            pool,
            tenant_id: "default".to_string(),
        }
    }

    pub fn with_tenant(pool: Arc<PgPool>, tenant_id: String) -> Self {
        Self { pool, tenant_id }
    }

    /// Create a new message
    pub async fn create_message(
        &self,
        conversation_id: Uuid,
        sender_id: Uuid,
        content: String,
        content_type: ContentType,
        reply_to: Option<Uuid>,
        client_message_id: Option<String>,
        mentions: Vec<Uuid>,
    ) -> Result<ChatMessage, StorageError> {
        let id = Uuid::new_v4();
        let content_type_str = match content_type {
            ContentType::Text => "text",
            ContentType::Image => "image",
            ContentType::File => "file",
            ContentType::System => "system",
        };

        let row = sqlx::query_as::<_, MessageRow>(
            r#"
            INSERT INTO messages (
                id, conversation_id, sender_id, tenant_id,
                content, content_type, reply_to_id, mentions, client_message_id
            )
            VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9)
            RETURNING
                id, conversation_id, sender_id, content, content_type,
                created_at, updated_at, deleted_at, reply_to_id, mentions, client_message_id,
                forwarded_from_message_id, forwarded_from_sender_id, forwarded_from_conversation_id
            "#,
        )
        .bind(id)
        .bind(conversation_id)
        .bind(sender_id)
        .bind(&self.tenant_id)
        .bind(&content)
        .bind(content_type_str)
        .bind(reply_to)
        .bind(&mentions)
        .bind(&client_message_id)
        .fetch_one(self.pool.as_ref())
        .await
        .map_err(|e| StorageError::Database(e.to_string()))?;

        // Update conversation's last_message
        sqlx::query(
            r#"
            UPDATE conversations
            SET last_message_id = $1, last_message_at = NOW(), updated_at = NOW()
            WHERE id = $2
            "#,
        )
        .bind(id)
        .bind(conversation_id)
        .execute(self.pool.as_ref())
        .await
        .map_err(|e| StorageError::Database(e.to_string()))?;

        Ok(row.into_chat_message())
    }

    /// Get a message by ID
    pub async fn get_message(&self, message_id: Uuid) -> Result<Option<ChatMessage>, StorageError> {
        let row = sqlx::query_as::<_, MessageRow>(
            r#"
            SELECT
                id, conversation_id, sender_id, content, content_type,
                created_at, updated_at, deleted_at, reply_to_id, mentions, client_message_id,
                forwarded_from_message_id, forwarded_from_sender_id, forwarded_from_conversation_id
            FROM messages
            WHERE id = $1 AND tenant_id = $2
            "#,
        )
        .bind(message_id)
        .bind(&self.tenant_id)
        .fetch_optional(self.pool.as_ref())
        .await
        .map_err(|e| StorageError::Database(e.to_string()))?;

        Ok(row.map(|r| r.into_chat_message()))
    }

    /// Mark a message as recalled (soft delete)
    pub async fn recall_message(&self, message_id: Uuid) -> Result<(), StorageError> {
        let result = sqlx::query(
            r#"
            UPDATE messages
            SET deleted_at = NOW(), content = '[Message recalled]', updated_at = NOW()
            WHERE id = $1 AND tenant_id = $2 AND deleted_at IS NULL
            "#,
        )
        .bind(message_id)
        .bind(&self.tenant_id)
        .execute(self.pool.as_ref())
        .await
        .map_err(|e| StorageError::Database(e.to_string()))?;

        if result.rows_affected() == 0 {
            return Err(StorageError::NotFound);
        }

        Ok(())
    }

    /// Get message history for a conversation with cursor-based pagination
    pub async fn get_history(
        &self,
        conversation_id: Uuid,
        before: Option<Uuid>,
        limit: u32,
    ) -> Result<(Vec<ChatMessage>, bool), StorageError> {
        let limit = limit.min(100) as i64;

        let messages = if let Some(before_id) = before {
            // Get the created_at of the cursor message
            let cursor_time: Option<DateTime<Utc>> = sqlx::query_scalar(
                "SELECT created_at FROM messages WHERE id = $1"
            )
            .bind(before_id)
            .fetch_optional(self.pool.as_ref())
            .await
            .map_err(|e| StorageError::Database(e.to_string()))?;

            let cursor_time = cursor_time.ok_or(StorageError::InvalidCursor)?;

            sqlx::query_as::<_, MessageRow>(
                r#"
                SELECT
                    id, conversation_id, sender_id, content, content_type,
                    created_at, updated_at, deleted_at, reply_to_id, mentions, client_message_id,
                    forwarded_from_message_id, forwarded_from_sender_id, forwarded_from_conversation_id
                FROM messages
                WHERE conversation_id = $1
                    AND tenant_id = $2
                    AND created_at < $3
                ORDER BY created_at DESC
                LIMIT $4
                "#,
            )
            .bind(conversation_id)
            .bind(&self.tenant_id)
            .bind(cursor_time)
            .bind(limit + 1) // Fetch one extra to check has_more
            .fetch_all(self.pool.as_ref())
            .await
            .map_err(|e| StorageError::Database(e.to_string()))?
        } else {
            sqlx::query_as::<_, MessageRow>(
                r#"
                SELECT
                    id, conversation_id, sender_id, content, content_type,
                    created_at, updated_at, deleted_at, reply_to_id, mentions, client_message_id,
                    forwarded_from_message_id, forwarded_from_sender_id, forwarded_from_conversation_id
                FROM messages
                WHERE conversation_id = $1 AND tenant_id = $2
                ORDER BY created_at DESC
                LIMIT $3
                "#,
            )
            .bind(conversation_id)
            .bind(&self.tenant_id)
            .bind(limit + 1)
            .fetch_all(self.pool.as_ref())
            .await
            .map_err(|e| StorageError::Database(e.to_string()))?
        };

        let has_more = messages.len() > limit as usize;
        let messages: Vec<ChatMessage> = messages
            .into_iter()
            .take(limit as usize)
            .map(|r| r.into_chat_message())
            .collect();

        Ok((messages, has_more))
    }

    /// Get messages by IDs (for batch loading)
    pub async fn get_messages_by_ids(&self, message_ids: &[Uuid]) -> Result<Vec<ChatMessage>, StorageError> {
        if message_ids.is_empty() {
            return Ok(vec![]);
        }

        let rows = sqlx::query_as::<_, MessageRow>(
            r#"
            SELECT
                id, conversation_id, sender_id, content, content_type,
                created_at, updated_at, deleted_at, reply_to_id, mentions, client_message_id,
                forwarded_from_message_id, forwarded_from_sender_id, forwarded_from_conversation_id
            FROM messages
            WHERE id = ANY($1) AND tenant_id = $2
            "#,
        )
        .bind(message_ids)
        .bind(&self.tenant_id)
        .fetch_all(self.pool.as_ref())
        .await
        .map_err(|e| StorageError::Database(e.to_string()))?;

        Ok(rows.into_iter().map(|r| r.into_chat_message()).collect())
    }

    /// Check for duplicate message by client_message_id
    pub async fn find_by_client_id(
        &self,
        sender_id: Uuid,
        client_message_id: &str,
    ) -> Result<Option<ChatMessage>, StorageError> {
        let row = sqlx::query_as::<_, MessageRow>(
            r#"
            SELECT
                id, conversation_id, sender_id, content, content_type,
                created_at, updated_at, deleted_at, reply_to_id, mentions, client_message_id,
                forwarded_from_message_id, forwarded_from_sender_id, forwarded_from_conversation_id
            FROM messages
            WHERE sender_id = $1 AND client_message_id = $2 AND tenant_id = $3
            ORDER BY created_at DESC
            LIMIT 1
            "#,
        )
        .bind(sender_id)
        .bind(client_message_id)
        .bind(&self.tenant_id)
        .fetch_optional(self.pool.as_ref())
        .await
        .map_err(|e| StorageError::Database(e.to_string()))?;

        Ok(row.map(|r| r.into_chat_message()))
    }

    /// Edit a message's content and mentions
    pub async fn edit_message(
        &self,
        message_id: Uuid,
        new_content: String,
        new_mentions: Vec<Uuid>,
    ) -> Result<ChatMessage, StorageError> {
        let now = Utc::now();

        let row = sqlx::query_as::<_, MessageRow>(
            r#"
            UPDATE messages
            SET content = $1, mentions = $2, updated_at = $3
            WHERE id = $4 AND tenant_id = $5 AND deleted_at IS NULL
            RETURNING
                id, conversation_id, sender_id, content, content_type,
                created_at, updated_at, deleted_at, reply_to_id, mentions, client_message_id,
                forwarded_from_message_id, forwarded_from_sender_id, forwarded_from_conversation_id
            "#,
        )
        .bind(&new_content)
        .bind(&new_mentions)
        .bind(now)
        .bind(message_id)
        .bind(&self.tenant_id)
        .fetch_optional(self.pool.as_ref())
        .await
        .map_err(|e| StorageError::Database(e.to_string()))?;

        row.map(|r| r.into_chat_message())
            .ok_or(StorageError::NotFound)
    }

    /// Search messages using PostgreSQL full-text search
    /// Returns messages only from conversations the user participates in
    pub async fn search_messages(
        &self,
        user_id: Uuid,
        search_term: &str,
        conversation_id: Option<Uuid>,
        limit: u32,
    ) -> Result<(Vec<MessageSearchResult>, u64), StorageError> {
        let limit = limit.min(50) as i64;

        // Escape special characters for tsquery
        // PostgreSQL tsquery special characters: & | ! : * ( ) \ '
        let search_query = search_term
            .split_whitespace()
            .filter(|w| !w.is_empty())
            .map(|w| {
                // Remove all tsquery special characters and escape single quotes
                let cleaned: String = w
                    .chars()
                    .filter(|c| !matches!(c, '&' | '|' | '!' | ':' | '*' | '(' | ')' | '\\'))
                    .collect();
                // Escape single quotes for SQL
                let escaped = cleaned.replace('\'', "''");
                // Only add prefix search if we have valid characters
                if escaped.is_empty() {
                    String::new()
                } else {
                    format!("{}:*", escaped)
                }
            })
            .filter(|s| !s.is_empty())
            .collect::<Vec<_>>()
            .join(" & ");

        // If no valid search terms after escaping, return empty results
        if search_query.is_empty() {
            return Ok((vec![], 0));
        }

        let (messages, total) = if let Some(conv_id) = conversation_id {
            // Search within a specific conversation
            let rows = sqlx::query_as::<_, MessageSearchRow>(
                r#"
                SELECT
                    m.id,
                    m.conversation_id,
                    m.sender_id,
                    m.content,
                    m.created_at,
                    ts_headline('english', m.content, to_tsquery('english', $1),
                        'StartSel=<mark>, StopSel=</mark>, MaxWords=35, MinWords=15'
                    ) as highlight
                FROM messages m
                INNER JOIN conversation_participants cp
                    ON m.conversation_id = cp.conversation_id
                WHERE cp.user_id = $2
                    AND cp.left_at IS NULL
                    AND m.conversation_id = $3
                    AND m.tenant_id = $4
                    AND m.deleted_at IS NULL
                    AND to_tsvector('english', m.content) @@ to_tsquery('english', $1)
                ORDER BY m.created_at DESC
                LIMIT $5
                "#,
            )
            .bind(&search_query)
            .bind(user_id)
            .bind(conv_id)
            .bind(&self.tenant_id)
            .bind(limit)
            .fetch_all(self.pool.as_ref())
            .await
            .map_err(|e| StorageError::Database(e.to_string()))?;

            // Get total count
            let count: (i64,) = sqlx::query_as(
                r#"
                SELECT COUNT(*)
                FROM messages m
                INNER JOIN conversation_participants cp
                    ON m.conversation_id = cp.conversation_id
                WHERE cp.user_id = $1
                    AND cp.left_at IS NULL
                    AND m.conversation_id = $2
                    AND m.tenant_id = $3
                    AND m.deleted_at IS NULL
                    AND to_tsvector('english', m.content) @@ to_tsquery('english', $4)
                "#,
            )
            .bind(user_id)
            .bind(conv_id)
            .bind(&self.tenant_id)
            .bind(&search_query)
            .fetch_one(self.pool.as_ref())
            .await
            .map_err(|e| StorageError::Database(e.to_string()))?;

            (rows, count.0 as u64)
        } else {
            // Search across all user's conversations
            let rows = sqlx::query_as::<_, MessageSearchRow>(
                r#"
                SELECT DISTINCT ON (m.id)
                    m.id,
                    m.conversation_id,
                    m.sender_id,
                    m.content,
                    m.created_at,
                    ts_headline('english', m.content, to_tsquery('english', $1),
                        'StartSel=<mark>, StopSel=</mark>, MaxWords=35, MinWords=15'
                    ) as highlight
                FROM messages m
                INNER JOIN conversation_participants cp
                    ON m.conversation_id = cp.conversation_id
                WHERE cp.user_id = $2
                    AND cp.left_at IS NULL
                    AND m.tenant_id = $3
                    AND m.deleted_at IS NULL
                    AND to_tsvector('english', m.content) @@ to_tsquery('english', $1)
                ORDER BY m.id, m.created_at DESC
                LIMIT $4
                "#,
            )
            .bind(&search_query)
            .bind(user_id)
            .bind(&self.tenant_id)
            .bind(limit)
            .fetch_all(self.pool.as_ref())
            .await
            .map_err(|e| StorageError::Database(e.to_string()))?;

            // Get total count
            let count: (i64,) = sqlx::query_as(
                r#"
                SELECT COUNT(DISTINCT m.id)
                FROM messages m
                INNER JOIN conversation_participants cp
                    ON m.conversation_id = cp.conversation_id
                WHERE cp.user_id = $1
                    AND cp.left_at IS NULL
                    AND m.tenant_id = $2
                    AND m.deleted_at IS NULL
                    AND to_tsvector('english', m.content) @@ to_tsquery('english', $3)
                "#,
            )
            .bind(user_id)
            .bind(&self.tenant_id)
            .bind(&search_query)
            .fetch_one(self.pool.as_ref())
            .await
            .map_err(|e| StorageError::Database(e.to_string()))?;

            (rows, count.0 as u64)
        };

        let results = messages
            .into_iter()
            .map(|r| MessageSearchResult {
                id: r.id,
                conversation_id: r.conversation_id,
                sender_id: r.sender_id,
                content: r.content,
                created_at: r.created_at.timestamp_millis(),
                highlight: r.highlight,
            })
            .collect();

        Ok((results, total))
    }

    // ==================== Thread-related Methods ====================

    /// Validate that a reply target exists and is in the same conversation
    /// Returns true if valid, false if not found or in different conversation
    pub async fn validate_reply_target(
        &self,
        reply_to_id: Uuid,
        conversation_id: Uuid,
    ) -> Result<bool, StorageError> {
        let result: Option<(Uuid, Option<DateTime<Utc>>)> = sqlx::query_as(
            r#"
            SELECT conversation_id, deleted_at
            FROM messages
            WHERE id = $1 AND tenant_id = $2
            "#,
        )
        .bind(reply_to_id)
        .bind(&self.tenant_id)
        .fetch_optional(self.pool.as_ref())
        .await
        .map_err(|e| StorageError::Database(e.to_string()))?;

        match result {
            Some((msg_conv_id, deleted_at)) => {
                // Check if message is in the same conversation and not deleted
                Ok(msg_conv_id == conversation_id && deleted_at.is_none())
            }
            None => Ok(false),
        }
    }

    /// Get reply context (preview of original message) for a reply
    pub async fn get_reply_context(
        &self,
        reply_to_id: Uuid,
    ) -> Result<Option<ReplyContext>, StorageError> {
        let row: Option<(Uuid, Uuid, String, String)> = sqlx::query_as(
            r#"
            SELECT id, sender_id, content, content_type
            FROM messages
            WHERE id = $1 AND tenant_id = $2 AND deleted_at IS NULL
            "#,
        )
        .bind(reply_to_id)
        .bind(&self.tenant_id)
        .fetch_optional(self.pool.as_ref())
        .await
        .map_err(|e| StorageError::Database(e.to_string()))?;

        Ok(row.map(|(id, sender_id, content, content_type)| {
            let ct = match content_type.as_str() {
                "image" => ContentType::Image,
                "file" => ContentType::File,
                "system" => ContentType::System,
                _ => ContentType::Text,
            };
            ReplyContext::new(id, sender_id, &content, ct)
        }))
    }

    /// Get the number of replies to a message
    pub async fn get_reply_count(&self, message_id: Uuid) -> Result<i32, StorageError> {
        let count: (i64,) = sqlx::query_as(
            r#"
            SELECT COUNT(*)
            FROM messages
            WHERE reply_to_id = $1 AND tenant_id = $2 AND deleted_at IS NULL
            "#,
        )
        .bind(message_id)
        .bind(&self.tenant_id)
        .fetch_one(self.pool.as_ref())
        .await
        .map_err(|e| StorageError::Database(e.to_string()))?;

        Ok(count.0 as i32)
    }

    /// Get thread info for a message (reply count, last reply info)
    pub async fn get_thread_info(&self, message_id: Uuid) -> Result<Option<ThreadInfo>, StorageError> {
        // Get reply count and last reply info in a single query
        let row: Option<(i64, Option<DateTime<Utc>>, Option<Uuid>)> = sqlx::query_as(
            r#"
            SELECT
                COUNT(*) as reply_count,
                MAX(created_at) as last_reply_at,
                (SELECT sender_id FROM messages
                 WHERE reply_to_id = $1 AND tenant_id = $2 AND deleted_at IS NULL
                 ORDER BY created_at DESC LIMIT 1) as last_reply_sender_id
            FROM messages
            WHERE reply_to_id = $1 AND tenant_id = $2 AND deleted_at IS NULL
            "#,
        )
        .bind(message_id)
        .bind(&self.tenant_id)
        .fetch_optional(self.pool.as_ref())
        .await
        .map_err(|e| StorageError::Database(e.to_string()))?;

        Ok(row.and_then(|(count, last_at, last_sender)| {
            if count > 0 {
                Some(ThreadInfo {
                    reply_count: count as i32,
                    last_reply_at: last_at.map(|t| t.timestamp_millis()),
                    last_reply_sender_id: last_sender,
                })
            } else {
                None
            }
        }))
    }

    /// Get replies to a specific message with pagination
    pub async fn get_thread_replies(
        &self,
        message_id: Uuid,
        before: Option<DateTime<Utc>>,
        limit: u32,
    ) -> Result<(Vec<ChatMessage>, bool), StorageError> {
        let limit = limit.min(100) as i64;

        let messages = if let Some(before_time) = before {
            sqlx::query_as::<_, MessageRow>(
                r#"
                SELECT
                    id, conversation_id, sender_id, content, content_type,
                    created_at, updated_at, deleted_at, reply_to_id, mentions, client_message_id,
                    forwarded_from_message_id, forwarded_from_sender_id, forwarded_from_conversation_id
                FROM messages
                WHERE reply_to_id = $1
                    AND tenant_id = $2
                    AND created_at < $3
                ORDER BY created_at ASC
                LIMIT $4
                "#,
            )
            .bind(message_id)
            .bind(&self.tenant_id)
            .bind(before_time)
            .bind(limit + 1)
            .fetch_all(self.pool.as_ref())
            .await
            .map_err(|e| StorageError::Database(e.to_string()))?
        } else {
            sqlx::query_as::<_, MessageRow>(
                r#"
                SELECT
                    id, conversation_id, sender_id, content, content_type,
                    created_at, updated_at, deleted_at, reply_to_id, mentions, client_message_id,
                    forwarded_from_message_id, forwarded_from_sender_id, forwarded_from_conversation_id
                FROM messages
                WHERE reply_to_id = $1 AND tenant_id = $2
                ORDER BY created_at ASC
                LIMIT $3
                "#,
            )
            .bind(message_id)
            .bind(&self.tenant_id)
            .bind(limit + 1)
            .fetch_all(self.pool.as_ref())
            .await
            .map_err(|e| StorageError::Database(e.to_string()))?
        };

        let has_more = messages.len() > limit as usize;
        let messages: Vec<ChatMessage> = messages
            .into_iter()
            .take(limit as usize)
            .map(|r| r.into_chat_message())
            .collect();

        Ok((messages, has_more))
    }

    /// Batch get thread info for multiple messages
    pub async fn get_thread_info_batch(
        &self,
        message_ids: &[Uuid],
    ) -> Result<std::collections::HashMap<Uuid, ThreadInfo>, StorageError> {
        if message_ids.is_empty() {
            return Ok(std::collections::HashMap::new());
        }

        let rows: Vec<(Uuid, i64, Option<DateTime<Utc>>, Option<Uuid>)> = sqlx::query_as(
            r#"
            SELECT
                reply_to_id,
                COUNT(*) as reply_count,
                MAX(created_at) as last_reply_at,
                (SELECT sender_id FROM messages m2
                 WHERE m2.reply_to_id = messages.reply_to_id
                   AND m2.tenant_id = $2
                   AND m2.deleted_at IS NULL
                 ORDER BY m2.created_at DESC LIMIT 1) as last_reply_sender_id
            FROM messages
            WHERE reply_to_id = ANY($1) AND tenant_id = $2 AND deleted_at IS NULL
            GROUP BY reply_to_id
            "#,
        )
        .bind(message_ids)
        .bind(&self.tenant_id)
        .fetch_all(self.pool.as_ref())
        .await
        .map_err(|e| StorageError::Database(e.to_string()))?;

        let result = rows
            .into_iter()
            .map(|(msg_id, count, last_at, last_sender)| {
                (
                    msg_id,
                    ThreadInfo {
                        reply_count: count as i32,
                        last_reply_at: last_at.map(|t| t.timestamp_millis()),
                        last_reply_sender_id: last_sender,
                    },
                )
            })
            .collect();

        Ok(result)
    }

    // ==================== Pin-related Methods ====================

    /// Pin a message in a conversation
    /// Returns the timestamp when the message was pinned
    pub async fn pin_message(
        &self,
        message_id: Uuid,
        conversation_id: Uuid,
        pinned_by: Uuid,
    ) -> Result<i64, StorageError> {
        // First verify the message exists and belongs to this conversation
        let msg_check: Option<(Uuid,)> = sqlx::query_as(
            r#"
            SELECT conversation_id
            FROM messages
            WHERE id = $1 AND tenant_id = $2 AND deleted_at IS NULL
            "#,
        )
        .bind(message_id)
        .bind(&self.tenant_id)
        .fetch_optional(self.pool.as_ref())
        .await
        .map_err(|e| StorageError::Database(e.to_string()))?;

        match msg_check {
            Some((msg_conv_id,)) if msg_conv_id == conversation_id => {
                // Message exists and is in the correct conversation
            }
            Some(_) => {
                return Err(StorageError::MessageNotInConversation);
            }
            None => {
                return Err(StorageError::NotFound);
            }
        }

        // Insert the pin (ON CONFLICT DO UPDATE to handle re-pinning)
        let row: (DateTime<Utc>,) = sqlx::query_as(
            r#"
            INSERT INTO message_pins (message_id, conversation_id, pinned_by, pinned_at)
            VALUES ($1, $2, $3, NOW())
            ON CONFLICT (message_id, conversation_id) DO UPDATE
                SET pinned_by = $3, pinned_at = NOW()
            RETURNING pinned_at
            "#,
        )
        .bind(message_id)
        .bind(conversation_id)
        .bind(pinned_by)
        .fetch_one(self.pool.as_ref())
        .await
        .map_err(|e| StorageError::Database(e.to_string()))?;

        Ok(row.0.timestamp_millis())
    }

    /// Unpin a message from a conversation
    /// Returns true if the message was unpinned, false if it wasn't pinned
    pub async fn unpin_message(
        &self,
        message_id: Uuid,
        conversation_id: Uuid,
    ) -> Result<bool, StorageError> {
        let result = sqlx::query(
            r#"
            DELETE FROM message_pins
            WHERE message_id = $1 AND conversation_id = $2
            "#,
        )
        .bind(message_id)
        .bind(conversation_id)
        .execute(self.pool.as_ref())
        .await
        .map_err(|e| StorageError::Database(e.to_string()))?;

        Ok(result.rows_affected() > 0)
    }

    /// Check if a message is pinned
    pub async fn is_message_pinned(
        &self,
        message_id: Uuid,
        conversation_id: Uuid,
    ) -> Result<bool, StorageError> {
        let exists: Option<(i64,)> = sqlx::query_as(
            r#"
            SELECT 1
            FROM message_pins
            WHERE message_id = $1 AND conversation_id = $2
            "#,
        )
        .bind(message_id)
        .bind(conversation_id)
        .fetch_optional(self.pool.as_ref())
        .await
        .map_err(|e| StorageError::Database(e.to_string()))?;

        Ok(exists.is_some())
    }

    /// Get pin info for a message (pinned_at, pinned_by)
    pub async fn get_pin_info(
        &self,
        message_id: Uuid,
        conversation_id: Uuid,
    ) -> Result<Option<(i64, Uuid)>, StorageError> {
        let row: Option<PinInfoRow> = sqlx::query_as(
            r#"
            SELECT pinned_at, pinned_by
            FROM message_pins
            WHERE message_id = $1 AND conversation_id = $2
            "#,
        )
        .bind(message_id)
        .bind(conversation_id)
        .fetch_optional(self.pool.as_ref())
        .await
        .map_err(|e| StorageError::Database(e.to_string()))?;

        Ok(row.map(|r| (r.pinned_at.timestamp_millis(), r.pinned_by)))
    }

    /// Get all pinned messages in a conversation
    pub async fn get_pinned_messages(
        &self,
        conversation_id: Uuid,
        limit: i64,
    ) -> Result<Vec<ChatMessage>, StorageError> {
        let limit = limit.min(100);

        // Get pinned message IDs with their pin info
        let pin_rows: Vec<(Uuid, DateTime<Utc>, Uuid)> = sqlx::query_as(
            r#"
            SELECT message_id, pinned_at, pinned_by
            FROM message_pins
            WHERE conversation_id = $1
            ORDER BY pinned_at DESC
            LIMIT $2
            "#,
        )
        .bind(conversation_id)
        .bind(limit)
        .fetch_all(self.pool.as_ref())
        .await
        .map_err(|e| StorageError::Database(e.to_string()))?;

        if pin_rows.is_empty() {
            return Ok(vec![]);
        }

        // Collect message IDs and create a map of pin info
        let message_ids: Vec<Uuid> = pin_rows.iter().map(|(id, _, _)| *id).collect();
        let pin_info: std::collections::HashMap<Uuid, (i64, Uuid)> = pin_rows
            .into_iter()
            .map(|(id, at, by)| (id, (at.timestamp_millis(), by)))
            .collect();

        // Fetch the full messages
        let rows = sqlx::query_as::<_, MessageRow>(
            r#"
            SELECT
                id, conversation_id, sender_id, content, content_type,
                created_at, updated_at, deleted_at, reply_to_id, mentions, client_message_id,
                forwarded_from_message_id, forwarded_from_sender_id, forwarded_from_conversation_id
            FROM messages
            WHERE id = ANY($1) AND tenant_id = $2
            "#,
        )
        .bind(&message_ids)
        .bind(&self.tenant_id)
        .fetch_all(self.pool.as_ref())
        .await
        .map_err(|e| StorageError::Database(e.to_string()))?;

        // Convert to ChatMessage and add pin info
        let mut messages: Vec<ChatMessage> = rows
            .into_iter()
            .map(|r| {
                let mut msg = r.into_chat_message();
                if let Some((pinned_at, pinned_by)) = pin_info.get(&msg.id) {
                    msg.pinned_at = Some(*pinned_at);
                    msg.pinned_by = Some(*pinned_by);
                }
                msg
            })
            .collect();

        // Sort by pinned_at DESC (most recently pinned first)
        messages.sort_by(|a, b| b.pinned_at.cmp(&a.pinned_at));

        Ok(messages)
    }

    /// Get the count of pinned messages in a conversation
    pub async fn get_pinned_count(&self, conversation_id: Uuid) -> Result<i64, StorageError> {
        let count: (i64,) = sqlx::query_as(
            r#"
            SELECT COUNT(*)
            FROM message_pins
            WHERE conversation_id = $1
            "#,
        )
        .bind(conversation_id)
        .fetch_one(self.pool.as_ref())
        .await
        .map_err(|e| StorageError::Database(e.to_string()))?;

        Ok(count.0)
    }

    // ==================== Forwarding-related Methods ====================

    /// Create a forwarded message
    /// This creates a new message in the target conversation with forwarding metadata
    pub async fn create_forwarded_message(
        &self,
        conversation_id: Uuid,
        sender_id: Uuid,
        content: String,
        content_type: ContentType,
        forwarded_from: ForwardedFrom,
    ) -> Result<ChatMessage, StorageError> {
        let id = Uuid::new_v4();
        let content_type_str = match content_type {
            ContentType::Text => "text",
            ContentType::Image => "image",
            ContentType::File => "file",
            ContentType::System => "system",
        };

        let row = sqlx::query_as::<_, MessageRow>(
            r#"
            INSERT INTO messages (
                id, conversation_id, sender_id, tenant_id,
                content, content_type, mentions,
                forwarded_from_message_id, forwarded_from_sender_id, forwarded_from_conversation_id
            )
            VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10)
            RETURNING
                id, conversation_id, sender_id, content, content_type,
                created_at, updated_at, deleted_at, reply_to_id, mentions, client_message_id,
                forwarded_from_message_id, forwarded_from_sender_id, forwarded_from_conversation_id
            "#,
        )
        .bind(id)
        .bind(conversation_id)
        .bind(sender_id)
        .bind(&self.tenant_id)
        .bind(&content)
        .bind(content_type_str)
        .bind(&Vec::<Uuid>::new()) // Empty mentions for forwarded messages
        .bind(forwarded_from.message_id)
        .bind(forwarded_from.sender_id)
        .bind(forwarded_from.conversation_id)
        .fetch_one(self.pool.as_ref())
        .await
        .map_err(|e| StorageError::Database(e.to_string()))?;

        // Update conversation's last_message
        sqlx::query(
            r#"
            UPDATE conversations
            SET last_message_id = $1, last_message_at = NOW(), updated_at = NOW()
            WHERE id = $2
            "#,
        )
        .bind(id)
        .bind(conversation_id)
        .execute(self.pool.as_ref())
        .await
        .map_err(|e| StorageError::Database(e.to_string()))?;

        Ok(row.into_chat_message())
    }

    /// Get a message that can be forwarded (exists and not recalled)
    /// Returns None if message doesn't exist or has been recalled
    pub async fn get_forwardable_message(
        &self,
        message_id: Uuid,
    ) -> Result<Option<ChatMessage>, StorageError> {
        let row = sqlx::query_as::<_, MessageRow>(
            r#"
            SELECT
                id, conversation_id, sender_id, content, content_type,
                created_at, updated_at, deleted_at, reply_to_id, mentions, client_message_id,
                forwarded_from_message_id, forwarded_from_sender_id, forwarded_from_conversation_id
            FROM messages
            WHERE id = $1 AND tenant_id = $2 AND deleted_at IS NULL
            "#,
        )
        .bind(message_id)
        .bind(&self.tenant_id)
        .fetch_optional(self.pool.as_ref())
        .await
        .map_err(|e| StorageError::Database(e.to_string()))?;

        Ok(row.map(|r| r.into_chat_message()))
    }
}

/// Search result row from database
#[derive(Debug, FromRow)]
struct MessageSearchRow {
    id: Uuid,
    conversation_id: Uuid,
    sender_id: Uuid,
    content: String,
    created_at: DateTime<Utc>,
    highlight: Option<String>,
}

/// Message search result
#[derive(Debug)]
pub struct MessageSearchResult {
    pub id: Uuid,
    pub conversation_id: Uuid,
    pub sender_id: Uuid,
    pub content: String,
    pub created_at: i64,
    pub highlight: Option<String>,
}

/// Storage errors
#[derive(Debug, thiserror::Error)]
pub enum StorageError {
    #[error("Database error: {0}")]
    Database(String),

    #[error("Message not found")]
    NotFound,

    #[error("Invalid cursor")]
    InvalidCursor,

    #[error("Serialization error: {0}")]
    Serialization(String),

    #[error("Invalid reply target: message not found or deleted")]
    InvalidReplyTarget,

    #[error("Message does not belong to this conversation")]
    MessageNotInConversation,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_storage_error_database_display() {
        let err = StorageError::Database("connection failed".to_string());
        assert_eq!(err.to_string(), "Database error: connection failed");
    }

    #[test]
    fn test_storage_error_not_found_display() {
        let err = StorageError::NotFound;
        assert_eq!(err.to_string(), "Message not found");
    }

    #[test]
    fn test_storage_error_invalid_cursor_display() {
        let err = StorageError::InvalidCursor;
        assert_eq!(err.to_string(), "Invalid cursor");
    }

    #[test]
    fn test_storage_error_serialization_display() {
        let err = StorageError::Serialization("invalid format".to_string());
        assert_eq!(err.to_string(), "Serialization error: invalid format");
    }

    #[test]
    fn test_storage_error_invalid_reply_target_display() {
        let err = StorageError::InvalidReplyTarget;
        assert_eq!(
            err.to_string(),
            "Invalid reply target: message not found or deleted"
        );
    }

    #[test]
    fn test_storage_error_debug() {
        let err = StorageError::NotFound;
        let debug = format!("{:?}", err);
        assert!(debug.contains("NotFound"));
    }

    #[test]
    fn test_content_type_from_string() {
        // Test the content type mapping in MessageRow::into_chat_message
        assert_eq!(ContentType::Text, ContentType::default());
    }

    // ==================== Search Query Escaping Tests ====================

    /// Helper to simulate the search query escaping logic
    fn escape_search_query(search_term: &str) -> String {
        search_term
            .split_whitespace()
            .filter(|w| !w.is_empty())
            .map(|w| {
                let cleaned: String = w
                    .chars()
                    .filter(|c| !matches!(c, '&' | '|' | '!' | ':' | '*' | '(' | ')' | '\\'))
                    .collect();
                let escaped = cleaned.replace('\'', "''");
                if escaped.is_empty() {
                    String::new()
                } else {
                    format!("{}:*", escaped)
                }
            })
            .filter(|s| !s.is_empty())
            .collect::<Vec<_>>()
            .join(" & ")
    }

    #[test]
    fn test_search_escape_normal_text() {
        let result = escape_search_query("hello world");
        assert_eq!(result, "hello:* & world:*");
    }

    #[test]
    fn test_search_escape_single_quotes() {
        let result = escape_search_query("it's a test");
        assert_eq!(result, "it''s:* & a:* & test:*");
    }

    #[test]
    fn test_search_escape_operators() {
        // Tsquery operators should be stripped
        let result = escape_search_query("hello & world | test");
        assert_eq!(result, "hello:* & world:* & test:*");
    }

    #[test]
    fn test_search_escape_special_chars() {
        let result = escape_search_query("test:* (foo) !bar");
        assert_eq!(result, "test:* & foo:* & bar:*");
    }

    #[test]
    fn test_search_escape_backslash() {
        let result = escape_search_query("path\\to\\file");
        assert_eq!(result, "pathtofile:*");
    }

    #[test]
    fn test_search_escape_empty_after_cleaning() {
        let result = escape_search_query("& | !");
        assert_eq!(result, "");
    }

    #[test]
    fn test_search_escape_mixed_content() {
        let result = escape_search_query("user's input & other:stuff");
        assert_eq!(result, "user''s:* & input:* & otherstuff:*");
    }

    #[test]
    fn test_search_escape_unicode() {
        let result = escape_search_query("你好 世界");
        assert_eq!(result, "你好:* & 世界:*");
    }

    #[test]
    fn test_search_escape_multiple_spaces() {
        let result = escape_search_query("hello    world");
        assert_eq!(result, "hello:* & world:*");
    }
}
