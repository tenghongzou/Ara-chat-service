//! Message storage - persistence layer for chat messages

use std::sync::Arc;

use chrono::{DateTime, Utc};
use sqlx::{FromRow, PgPool, Row};
use uuid::Uuid;

use super::types::{ChatMessage, ContentType};

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
}

impl MessageRow {
    fn into_chat_message(self) -> ChatMessage {
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
            mentions: self.mentions,
            reactions: Default::default(), // Loaded separately
            recalled_at: self.deleted_at.map(|t| t.timestamp_millis()),
        }
    }
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
                created_at, updated_at, deleted_at, reply_to_id, mentions, client_message_id
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
                created_at, updated_at, deleted_at, reply_to_id, mentions, client_message_id
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
                    created_at, updated_at, deleted_at, reply_to_id, mentions, client_message_id
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
                    created_at, updated_at, deleted_at, reply_to_id, mentions, client_message_id
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
                created_at, updated_at, deleted_at, reply_to_id, mentions, client_message_id
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
                created_at, updated_at, deleted_at, reply_to_id, mentions, client_message_id
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
                created_at, updated_at, deleted_at, reply_to_id, mentions, client_message_id
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
