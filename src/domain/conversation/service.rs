//! Conversation service - CRUD operations for conversations

use std::sync::Arc;

use chrono::{DateTime, Utc};
use serde::Deserialize;
use sqlx::{FromRow, PgPool};
use sqlx::types::Json;
use uuid::Uuid;

use crate::message::{ConversationType, ConversationSummary, LastMessagePreview, ParticipantInfo, ParticipantRole, ContentType};
use super::types::{Conversation, ConversationParticipant};
use super::direct_lookup::DirectMessageLookup;

/// Conversation row from database
#[derive(Debug, FromRow)]
struct ConversationRow {
    id: Uuid,
    tenant_id: String,
    #[sqlx(rename = "type")]
    conversation_type: String,
    name: Option<String>,
    avatar_url: Option<String>,
    created_by: Uuid,
    created_at: DateTime<Utc>,
    updated_at: DateTime<Utc>,
    participant_count: i32,
    last_message_id: Option<Uuid>,
    last_message_at: Option<DateTime<Utc>>,
}

impl ConversationRow {
    fn into_conversation(self) -> Conversation {
        Conversation {
            id: self.id,
            tenant_id: self.tenant_id,
            conversation_type: match self.conversation_type.as_str() {
                "group" => ConversationType::Group,
                _ => ConversationType::Direct,
            },
            name: self.name,
            avatar_url: self.avatar_url,
            created_by: self.created_by,
            created_at: self.created_at,
            updated_at: self.updated_at,
            participant_count: self.participant_count as u32,
            last_message_id: self.last_message_id,
            last_message_at: self.last_message_at,
        }
    }
}

/// Conversation row with participant preview (optimized for lazy loading)
#[derive(Debug, FromRow)]
struct ConversationWithPreviewRow {
    id: Uuid,
    tenant_id: String,
    #[sqlx(rename = "type")]
    conversation_type: String,
    name: Option<String>,
    avatar_url: Option<String>,
    created_by: Uuid,
    created_at: DateTime<Utc>,
    updated_at: DateTime<Utc>,
    participant_count: i32,
    last_message_id: Option<Uuid>,
    last_message_at: Option<DateTime<Utc>>,
    /// JSON array of participant preview: [{user_id, role}, ...]
    participant_preview: Option<Json<Vec<ParticipantPreviewJson>>>,
    /// Current user's muted status
    is_muted: bool,
}

/// Participant preview from JSON subquery
#[derive(Debug, Deserialize)]
struct ParticipantPreviewJson {
    user_id: Uuid,
    role: String,
}

/// Participant row from database
#[derive(Debug, FromRow)]
struct ParticipantRow {
    conversation_id: Uuid,
    user_id: Uuid,
    tenant_id: String,
    role: String,
    joined_at: DateTime<Utc>,
    left_at: Option<DateTime<Utc>>,
    last_read_message_id: Option<Uuid>,
    last_read_at: Option<DateTime<Utc>>,
    is_muted: bool,
    muted_at: Option<DateTime<Utc>>,
}

/// Service for managing conversations
pub struct ConversationService {
    pool: Arc<PgPool>,
    direct_lookup: DirectMessageLookup,
    tenant_id: String,
}

impl ConversationService {
    pub fn new(pool: Arc<PgPool>) -> Self {
        Self {
            direct_lookup: DirectMessageLookup::new(pool.clone()),
            pool,
            tenant_id: "default".to_string(),
        }
    }

    pub fn with_tenant(pool: Arc<PgPool>, tenant_id: String) -> Self {
        Self {
            direct_lookup: DirectMessageLookup::new(pool.clone()),
            pool,
            tenant_id,
        }
    }

    /// Check if a user is a participant in a conversation
    pub async fn is_participant(
        &self,
        conversation_id: Uuid,
        user_id: Uuid,
    ) -> Result<bool, ConversationError> {
        let result: Option<(Uuid,)> = sqlx::query_as(
            r#"
            SELECT user_id
            FROM conversation_participants
            WHERE conversation_id = $1 AND user_id = $2 AND tenant_id = $3 AND left_at IS NULL
            "#,
        )
        .bind(conversation_id)
        .bind(user_id)
        .bind(&self.tenant_id)
        .fetch_optional(self.pool.as_ref())
        .await
        .map_err(|e| ConversationError::Database(e.to_string()))?;

        Ok(result.is_some())
    }

    /// Get all participant user IDs for a conversation
    pub async fn get_participant_ids(
        &self,
        conversation_id: Uuid,
    ) -> Result<Vec<Uuid>, ConversationError> {
        let rows: Vec<(Uuid,)> = sqlx::query_as(
            r#"
            SELECT user_id
            FROM conversation_participants
            WHERE conversation_id = $1 AND tenant_id = $2 AND left_at IS NULL
            "#,
        )
        .bind(conversation_id)
        .bind(&self.tenant_id)
        .fetch_all(self.pool.as_ref())
        .await
        .map_err(|e| ConversationError::Database(e.to_string()))?;

        Ok(rows.into_iter().map(|(id,)| id).collect())
    }

    /// Get participants with details
    pub async fn get_participants(
        &self,
        conversation_id: Uuid,
    ) -> Result<Vec<ConversationParticipant>, ConversationError> {
        let rows = sqlx::query_as::<_, ParticipantRow>(
            r#"
            SELECT conversation_id, user_id, tenant_id, role, joined_at, left_at,
                   last_read_message_id, last_read_at, is_muted, muted_at
            FROM conversation_participants
            WHERE conversation_id = $1 AND tenant_id = $2 AND left_at IS NULL
            "#,
        )
        .bind(conversation_id)
        .bind(&self.tenant_id)
        .fetch_all(self.pool.as_ref())
        .await
        .map_err(|e| ConversationError::Database(e.to_string()))?;

        Ok(rows.into_iter().map(|r| ConversationParticipant {
            conversation_id: r.conversation_id,
            user_id: r.user_id,
            tenant_id: r.tenant_id,
            role: match r.role.as_str() {
                "owner" => ParticipantRole::Owner,
                "admin" => ParticipantRole::Admin,
                _ => ParticipantRole::Member,
            },
            joined_at: r.joined_at,
            left_at: r.left_at,
            last_read_message_id: r.last_read_message_id,
            last_read_at: r.last_read_at,
            is_muted: r.is_muted,
            muted_at: r.muted_at,
        }).collect())
    }

    /// Get participants with pagination for lazy loading
    /// Returns (participants, total_count, has_more)
    pub async fn get_participants_paginated(
        &self,
        conversation_id: Uuid,
        offset: u32,
        limit: u32,
    ) -> Result<(Vec<ParticipantInfo>, u32, bool), ConversationError> {
        // Get total count
        let count_row: (i64,) = sqlx::query_as(
            r#"
            SELECT COUNT(*)
            FROM conversation_participants
            WHERE conversation_id = $1 AND tenant_id = $2 AND left_at IS NULL
            "#,
        )
        .bind(conversation_id)
        .bind(&self.tenant_id)
        .fetch_one(self.pool.as_ref())
        .await
        .map_err(|e| ConversationError::Database(e.to_string()))?;

        let total_count = count_row.0 as u32;

        // Fetch paginated participants, ordered by role (owner first, then admin, then member) and join time
        let rows = sqlx::query_as::<_, ParticipantRow>(
            r#"
            SELECT conversation_id, user_id, tenant_id, role, joined_at, left_at,
                   last_read_message_id, last_read_at, is_muted, muted_at
            FROM conversation_participants
            WHERE conversation_id = $1 AND tenant_id = $2 AND left_at IS NULL
            ORDER BY CASE role
                WHEN 'owner' THEN 0
                WHEN 'admin' THEN 1
                ELSE 2
            END, joined_at
            LIMIT $3 OFFSET $4
            "#,
        )
        .bind(conversation_id)
        .bind(&self.tenant_id)
        .bind(limit as i64)
        .bind(offset as i64)
        .fetch_all(self.pool.as_ref())
        .await
        .map_err(|e| ConversationError::Database(e.to_string()))?;

        let participants: Vec<ParticipantInfo> = rows.into_iter().map(|r| ParticipantInfo {
            user_id: r.user_id,
            name: None, // Would need user service
            avatar_url: None,
            role: match r.role.as_str() {
                "owner" => ParticipantRole::Owner,
                "admin" => ParticipantRole::Admin,
                _ => ParticipantRole::Member,
            },
        }).collect();

        let has_more = (offset + limit) < total_count;

        Ok((participants, total_count, has_more))
    }

    /// Create a new conversation
    pub async fn create_conversation(
        &self,
        conversation_type: ConversationType,
        created_by: Uuid,
        participants: Vec<Uuid>,
        name: Option<String>,
    ) -> Result<Conversation, ConversationError> {
        // For direct messages, check if conversation already exists
        if conversation_type == ConversationType::Direct && participants.len() == 2 {
            if let Some(existing) = self.direct_lookup
                .find_direct_conversation(participants[0], participants[1], &self.tenant_id)
                .await?
            {
                return self.get_conversation(existing).await?
                    .ok_or(ConversationError::NotFound);
            }
        }

        let id = Uuid::new_v4();
        let type_str = match conversation_type {
            ConversationType::Direct => "direct",
            ConversationType::Group => "group",
        };

        // Start transaction
        let mut tx = self.pool.begin().await
            .map_err(|e| ConversationError::Database(e.to_string()))?;

        // Create conversation
        let row = sqlx::query_as::<_, ConversationRow>(
            r#"
            INSERT INTO conversations (id, tenant_id, type, name, created_by, participant_count)
            VALUES ($1, $2, $3, $4, $5, $6)
            RETURNING id, tenant_id, type, name, avatar_url, created_by, created_at, updated_at, participant_count, last_message_id, last_message_at
            "#,
        )
        .bind(id)
        .bind(&self.tenant_id)
        .bind(type_str)
        .bind(&name)
        .bind(created_by)
        .bind(participants.len() as i32)
        .fetch_one(&mut *tx)
        .await
        .map_err(|e| ConversationError::Database(e.to_string()))?;

        // Add participants
        for (i, &user_id) in participants.iter().enumerate() {
            let role = if user_id == created_by { "owner" } else { "member" };
            sqlx::query(
                r#"
                INSERT INTO conversation_participants (conversation_id, user_id, tenant_id, role)
                VALUES ($1, $2, $3, $4)
                "#,
            )
            .bind(id)
            .bind(user_id)
            .bind(&self.tenant_id)
            .bind(role)
            .execute(&mut *tx)
            .await
            .map_err(|e| ConversationError::Database(e.to_string()))?;
        }

        // For direct messages, create lookup entry
        if conversation_type == ConversationType::Direct && participants.len() == 2 {
            self.direct_lookup
                .register_direct_conversation_tx(&mut tx, participants[0], participants[1], id, &self.tenant_id)
                .await?;
        }

        tx.commit().await
            .map_err(|e| ConversationError::Database(e.to_string()))?;

        Ok(row.into_conversation())
    }

    /// Get a conversation by ID
    pub async fn get_conversation(
        &self,
        conversation_id: Uuid,
    ) -> Result<Option<Conversation>, ConversationError> {
        let row = sqlx::query_as::<_, ConversationRow>(
            r#"
            SELECT id, tenant_id, type, name, avatar_url, created_by, created_at, updated_at, participant_count, last_message_id, last_message_at
            FROM conversations
            WHERE id = $1 AND tenant_id = $2
            "#,
        )
        .bind(conversation_id)
        .bind(&self.tenant_id)
        .fetch_optional(self.pool.as_ref())
        .await
        .map_err(|e| ConversationError::Database(e.to_string()))?;

        Ok(row.map(|r| r.into_conversation()))
    }

    /// Get user's conversation list with last message preview (optimized with participant preview)
    ///
    /// Uses a single optimized query with embedded participant preview to eliminate N+1 queries.
    /// Only loads `preview_count` participants per conversation for bandwidth optimization.
    pub async fn get_user_conversations(
        &self,
        user_id: Uuid,
        before: Option<i64>,
        limit: u32,
    ) -> Result<(Vec<ConversationSummary>, bool), ConversationError> {
        // Use default preview count of 5 participants
        self.get_user_conversations_with_preview(user_id, before, limit, 5).await
    }

    /// Get user's conversation list with configurable participant preview count
    ///
    /// This optimized version uses a single query with embedded participant preview,
    /// eliminating the N+1 query problem. Only the first `preview_count` participants
    /// are loaded for each conversation.
    pub async fn get_user_conversations_with_preview(
        &self,
        user_id: Uuid,
        before: Option<i64>,
        limit: u32,
        preview_count: usize,
    ) -> Result<(Vec<ConversationSummary>, bool), ConversationError> {
        let limit = limit.min(50) as i64;
        let preview_count = preview_count.min(20) as i64; // Cap at 20 for safety

        // Optimized query with participant preview embedded as JSON subquery
        let conversations = if let Some(before_ts) = before {
            let before_time = DateTime::from_timestamp_millis(before_ts)
                .ok_or_else(|| ConversationError::Database("Invalid timestamp".to_string()))?;

            sqlx::query_as::<_, ConversationWithPreviewRow>(
                r#"
                SELECT c.id, c.tenant_id, c.type, c.name, c.avatar_url, c.created_by,
                       c.created_at, c.updated_at, c.participant_count, c.last_message_id, c.last_message_at,
                       (
                           SELECT COALESCE(json_agg(json_build_object('user_id', pp.user_id, 'role', pp.role)), '[]'::json)
                           FROM (
                               SELECT user_id, role
                               FROM conversation_participants
                               WHERE conversation_id = c.id AND tenant_id = $2 AND left_at IS NULL
                               ORDER BY CASE role WHEN 'owner' THEN 0 WHEN 'admin' THEN 1 ELSE 2 END, joined_at
                               LIMIT $5
                           ) pp
                       ) as participant_preview,
                       p.is_muted
                FROM conversations c
                INNER JOIN conversation_participants p ON c.id = p.conversation_id
                WHERE p.user_id = $1 AND p.tenant_id = $2 AND p.left_at IS NULL
                    AND c.updated_at < $3
                ORDER BY c.updated_at DESC
                LIMIT $4
                "#,
            )
            .bind(user_id)
            .bind(&self.tenant_id)
            .bind(before_time)
            .bind(limit + 1)
            .bind(preview_count)
            .fetch_all(self.pool.as_ref())
            .await
            .map_err(|e| ConversationError::Database(e.to_string()))?
        } else {
            sqlx::query_as::<_, ConversationWithPreviewRow>(
                r#"
                SELECT c.id, c.tenant_id, c.type, c.name, c.avatar_url, c.created_by,
                       c.created_at, c.updated_at, c.participant_count, c.last_message_id, c.last_message_at,
                       (
                           SELECT COALESCE(json_agg(json_build_object('user_id', pp.user_id, 'role', pp.role)), '[]'::json)
                           FROM (
                               SELECT user_id, role
                               FROM conversation_participants
                               WHERE conversation_id = c.id AND tenant_id = $2 AND left_at IS NULL
                               ORDER BY CASE role WHEN 'owner' THEN 0 WHEN 'admin' THEN 1 ELSE 2 END, joined_at
                               LIMIT $4
                           ) pp
                       ) as participant_preview,
                       p.is_muted
                FROM conversations c
                INNER JOIN conversation_participants p ON c.id = p.conversation_id
                WHERE p.user_id = $1 AND p.tenant_id = $2 AND p.left_at IS NULL
                ORDER BY c.updated_at DESC
                LIMIT $3
                "#,
            )
            .bind(user_id)
            .bind(&self.tenant_id)
            .bind(limit + 1)
            .bind(preview_count)
            .fetch_all(self.pool.as_ref())
            .await
            .map_err(|e| ConversationError::Database(e.to_string()))?
        };

        let has_more = conversations.len() > limit as usize;

        // Build summaries from the optimized query result
        let mut summaries = Vec::with_capacity(limit as usize);
        for conv in conversations.into_iter().take(limit as usize) {
            // Parse participant preview from JSON
            let participant_infos: Vec<ParticipantInfo> = conv.participant_preview
                .map(|preview| {
                    preview.0.into_iter().map(|p| ParticipantInfo {
                        user_id: p.user_id,
                        name: None,
                        avatar_url: None,
                        role: match p.role.as_str() {
                            "owner" => ParticipantRole::Owner,
                            "admin" => ParticipantRole::Admin,
                            _ => ParticipantRole::Member,
                        },
                    }).collect()
                })
                .unwrap_or_default();

            // Get last message preview if exists
            let last_message = if let Some(msg_id) = conv.last_message_id {
                self.get_last_message_preview(msg_id).await?
            } else {
                None
            };

            summaries.push(ConversationSummary {
                id: conv.id,
                conversation_type: match conv.conversation_type.as_str() {
                    "group" => ConversationType::Group,
                    _ => ConversationType::Direct,
                },
                name: conv.name,
                avatar_url: conv.avatar_url,
                participant_count: conv.participant_count as u32,
                participants: participant_infos,
                last_message,
                unread_count: 0, // TODO: Get from Redis
                updated_at: conv.updated_at.timestamp_millis(),
                is_muted: conv.is_muted,
            });
        }

        Ok((summaries, has_more))
    }

    /// Get last message preview for a conversation
    async fn get_last_message_preview(&self, message_id: Uuid) -> Result<Option<LastMessagePreview>, ConversationError> {
        #[derive(FromRow)]
        struct MessagePreviewRow {
            id: Uuid,
            sender_id: Uuid,
            content: String,
            content_type: String,
            created_at: DateTime<Utc>,
        }

        let row = sqlx::query_as::<_, MessagePreviewRow>(
            r#"
            SELECT id, sender_id, content, content_type, created_at
            FROM messages
            WHERE id = $1
            "#,
        )
        .bind(message_id)
        .fetch_optional(self.pool.as_ref())
        .await
        .map_err(|e| ConversationError::Database(e.to_string()))?;

        Ok(row.map(|r| {
            let content_preview = if r.content.len() > 100 {
                format!("{}...", &r.content[..97])
            } else {
                r.content
            };

            LastMessagePreview {
                message_id: r.id,
                sender_id: r.sender_id,
                content_preview,
                content_type: match r.content_type.as_str() {
                    "image" => ContentType::Image,
                    "file" => ContentType::File,
                    "system" => ContentType::System,
                    _ => ContentType::Text,
                },
                created_at: r.created_at.timestamp_millis(),
            }
        }))
    }

    /// Update the last message of a conversation
    pub async fn update_last_message(
        &self,
        conversation_id: Uuid,
        message_id: Uuid,
    ) -> Result<(), ConversationError> {
        let now = Utc::now();

        sqlx::query(
            r#"
            UPDATE conversations
            SET last_message_id = $1, last_message_at = $2, updated_at = $2
            WHERE id = $3 AND tenant_id = $4
            "#,
        )
        .bind(message_id)
        .bind(now)
        .bind(conversation_id)
        .bind(&self.tenant_id)
        .execute(self.pool.as_ref())
        .await
        .map_err(|e| ConversationError::Database(e.to_string()))?;

        Ok(())
    }

    /// Find or create a direct conversation between two users
    pub async fn find_or_create_direct(
        &self,
        user1: Uuid,
        user2: Uuid,
    ) -> Result<Conversation, ConversationError> {
        // Check if exists
        if let Some(conv_id) = self.direct_lookup
            .find_direct_conversation(user1, user2, &self.tenant_id)
            .await?
        {
            return self.get_conversation(conv_id).await?
                .ok_or(ConversationError::NotFound);
        }

        // Create new
        self.create_conversation(
            ConversationType::Direct,
            user1,
            vec![user1, user2],
            None,
        ).await
    }

    /// Add a participant to a conversation
    pub async fn add_participant(
        &self,
        conversation_id: Uuid,
        user_id: Uuid,
        added_by: Uuid,
    ) -> Result<(), ConversationError> {
        // Verify adder is a participant
        if !self.is_participant(conversation_id, added_by).await? {
            return Err(ConversationError::NotParticipant);
        }

        // Check if user is already a participant
        if self.is_participant(conversation_id, user_id).await? {
            return Ok(()); // Already a participant
        }

        let mut tx = self.pool.begin().await
            .map_err(|e| ConversationError::Database(e.to_string()))?;

        // Add participant
        sqlx::query(
            r#"
            INSERT INTO conversation_participants
                (conversation_id, user_id, tenant_id, role, joined_at)
            VALUES ($1, $2, $3, 'member', NOW())
            ON CONFLICT (conversation_id, user_id) DO UPDATE
            SET left_at = NULL, joined_at = NOW()
            "#,
        )
        .bind(conversation_id)
        .bind(user_id)
        .bind(&self.tenant_id)
        .execute(&mut *tx)
        .await
        .map_err(|e| ConversationError::Database(e.to_string()))?;

        // Update participant count
        sqlx::query(
            r#"
            UPDATE conversations
            SET participant_count = (
                SELECT COUNT(*) FROM conversation_participants
                WHERE conversation_id = $1 AND left_at IS NULL
            ), updated_at = NOW()
            WHERE id = $1
            "#,
        )
        .bind(conversation_id)
        .execute(&mut *tx)
        .await
        .map_err(|e| ConversationError::Database(e.to_string()))?;

        tx.commit().await
            .map_err(|e| ConversationError::Database(e.to_string()))?;

        tracing::info!(
            conversation_id = %conversation_id,
            user_id = %user_id,
            added_by = %added_by,
            "Participant added to conversation"
        );

        Ok(())
    }

    /// Remove a participant from a conversation
    pub async fn remove_participant(
        &self,
        conversation_id: Uuid,
        user_id: Uuid,
        removed_by: Uuid,
    ) -> Result<(), ConversationError> {
        // Verify remover is a participant
        if !self.is_participant(conversation_id, removed_by).await? {
            return Err(ConversationError::NotParticipant);
        }

        let mut tx = self.pool.begin().await
            .map_err(|e| ConversationError::Database(e.to_string()))?;

        // Mark participant as left
        sqlx::query(
            r#"
            UPDATE conversation_participants
            SET left_at = NOW()
            WHERE conversation_id = $1 AND user_id = $2 AND tenant_id = $3
            "#,
        )
        .bind(conversation_id)
        .bind(user_id)
        .bind(&self.tenant_id)
        .execute(&mut *tx)
        .await
        .map_err(|e| ConversationError::Database(e.to_string()))?;

        // Update participant count
        sqlx::query(
            r#"
            UPDATE conversations
            SET participant_count = (
                SELECT COUNT(*) FROM conversation_participants
                WHERE conversation_id = $1 AND left_at IS NULL
            ), updated_at = NOW()
            WHERE id = $1
            "#,
        )
        .bind(conversation_id)
        .execute(&mut *tx)
        .await
        .map_err(|e| ConversationError::Database(e.to_string()))?;

        tx.commit().await
            .map_err(|e| ConversationError::Database(e.to_string()))?;

        tracing::info!(
            conversation_id = %conversation_id,
            user_id = %user_id,
            removed_by = %removed_by,
            "Participant removed from conversation"
        );

        Ok(())
    }

    /// Leave a conversation (self-remove)
    pub async fn leave_conversation(
        &self,
        conversation_id: Uuid,
        user_id: Uuid,
    ) -> Result<(), ConversationError> {
        self.remove_participant(conversation_id, user_id, user_id).await
    }

    /// Get a single conversation summary for a user
    pub async fn get_conversation_summary(
        &self,
        conversation_id: Uuid,
        user_id: Uuid,
    ) -> Result<Option<ConversationSummary>, ConversationError> {
        // Get the conversation
        let conv = match self.get_conversation(conversation_id).await? {
            Some(c) => c,
            None => return Ok(None),
        };

        // Get participants
        let participants = self.get_participants(conversation_id).await?;
        let participant_infos: Vec<ParticipantInfo> = participants.iter().map(|p| ParticipantInfo {
            user_id: p.user_id,
            name: None,
            avatar_url: None,
            role: p.role,
        }).collect();

        // Get last message preview
        let last_message = if let Some(msg_id) = conv.last_message_id {
            self.get_last_message_preview(msg_id).await?
        } else {
            None
        };

        // Check if user has muted this conversation
        let is_muted = participants.iter()
            .find(|p| p.user_id == user_id)
            .map(|p| p.is_muted)
            .unwrap_or(false);

        Ok(Some(ConversationSummary {
            id: conv.id,
            conversation_type: conv.conversation_type,
            name: conv.name,
            avatar_url: conv.avatar_url,
            participant_count: conv.participant_count,
            participants: participant_infos,
            last_message,
            unread_count: 0, // Will be updated by caller from Redis
            updated_at: conv.updated_at.timestamp_millis(),
            is_muted,
        }))
    }

    /// Get participant info for a user in a conversation
    pub async fn get_participant_info(
        &self,
        conversation_id: Uuid,
        user_id: Uuid,
    ) -> Result<Option<ConversationParticipant>, ConversationError> {
        let row = sqlx::query_as::<_, ParticipantRow>(
            r#"
            SELECT conversation_id, user_id, tenant_id, role, joined_at, left_at,
                   last_read_message_id, last_read_at, is_muted, muted_at
            FROM conversation_participants
            WHERE conversation_id = $1 AND user_id = $2 AND tenant_id = $3
            "#,
        )
        .bind(conversation_id)
        .bind(user_id)
        .bind(&self.tenant_id)
        .fetch_optional(self.pool.as_ref())
        .await
        .map_err(|e| ConversationError::Database(e.to_string()))?;

        Ok(row.map(|r| ConversationParticipant {
            conversation_id: r.conversation_id,
            user_id: r.user_id,
            tenant_id: r.tenant_id,
            role: match r.role.as_str() {
                "owner" => crate::message::ParticipantRole::Owner,
                "admin" => crate::message::ParticipantRole::Admin,
                _ => crate::message::ParticipantRole::Member,
            },
            joined_at: r.joined_at,
            left_at: r.left_at,
            last_read_message_id: r.last_read_message_id,
            last_read_at: r.last_read_at,
            is_muted: r.is_muted,
            muted_at: r.muted_at,
        }))
    }

    // ==================== Muting Methods ====================

    /// Mute a conversation for a user
    /// Returns the timestamp when muted
    pub async fn mute_conversation(
        &self,
        conversation_id: Uuid,
        user_id: Uuid,
    ) -> Result<i64, ConversationError> {
        // Verify user is a participant
        if !self.is_participant(conversation_id, user_id).await? {
            return Err(ConversationError::NotParticipant);
        }

        let now = Utc::now();

        sqlx::query(
            r#"
            UPDATE conversation_participants
            SET is_muted = TRUE, muted_at = $1
            WHERE conversation_id = $2 AND user_id = $3 AND tenant_id = $4
            "#,
        )
        .bind(now)
        .bind(conversation_id)
        .bind(user_id)
        .bind(&self.tenant_id)
        .execute(self.pool.as_ref())
        .await
        .map_err(|e| ConversationError::Database(e.to_string()))?;

        tracing::info!(
            user_id = %user_id,
            conversation_id = %conversation_id,
            "Conversation muted"
        );

        Ok(now.timestamp_millis())
    }

    /// Unmute a conversation for a user
    pub async fn unmute_conversation(
        &self,
        conversation_id: Uuid,
        user_id: Uuid,
    ) -> Result<(), ConversationError> {
        // Verify user is a participant
        if !self.is_participant(conversation_id, user_id).await? {
            return Err(ConversationError::NotParticipant);
        }

        sqlx::query(
            r#"
            UPDATE conversation_participants
            SET is_muted = FALSE, muted_at = NULL
            WHERE conversation_id = $1 AND user_id = $2 AND tenant_id = $3
            "#,
        )
        .bind(conversation_id)
        .bind(user_id)
        .bind(&self.tenant_id)
        .execute(self.pool.as_ref())
        .await
        .map_err(|e| ConversationError::Database(e.to_string()))?;

        tracing::info!(
            user_id = %user_id,
            conversation_id = %conversation_id,
            "Conversation unmuted"
        );

        Ok(())
    }

    /// Check if a user has muted a conversation
    pub async fn is_conversation_muted(
        &self,
        conversation_id: Uuid,
        user_id: Uuid,
    ) -> Result<bool, ConversationError> {
        let result: Option<(bool,)> = sqlx::query_as(
            r#"
            SELECT is_muted
            FROM conversation_participants
            WHERE conversation_id = $1 AND user_id = $2 AND tenant_id = $3 AND left_at IS NULL
            "#,
        )
        .bind(conversation_id)
        .bind(user_id)
        .bind(&self.tenant_id)
        .fetch_optional(self.pool.as_ref())
        .await
        .map_err(|e| ConversationError::Database(e.to_string()))?;

        Ok(result.map(|(muted,)| muted).unwrap_or(false))
    }

    /// Get all muted user IDs for a conversation
    /// Used by router to filter push notifications
    pub async fn get_muted_user_ids(
        &self,
        conversation_id: Uuid,
    ) -> Result<Vec<Uuid>, ConversationError> {
        let rows: Vec<(Uuid,)> = sqlx::query_as(
            r#"
            SELECT user_id
            FROM conversation_participants
            WHERE conversation_id = $1 AND tenant_id = $2 AND left_at IS NULL AND is_muted = TRUE
            "#,
        )
        .bind(conversation_id)
        .bind(&self.tenant_id)
        .fetch_all(self.pool.as_ref())
        .await
        .map_err(|e| ConversationError::Database(e.to_string()))?;

        Ok(rows.into_iter().map(|(id,)| id).collect())
    }

    /// Get all muted conversation IDs for a user
    pub async fn get_muted_conversations(
        &self,
        user_id: Uuid,
    ) -> Result<Vec<Uuid>, ConversationError> {
        let rows: Vec<(Uuid,)> = sqlx::query_as(
            r#"
            SELECT conversation_id
            FROM conversation_participants
            WHERE user_id = $1 AND tenant_id = $2 AND left_at IS NULL AND is_muted = TRUE
            "#,
        )
        .bind(user_id)
        .bind(&self.tenant_id)
        .fetch_all(self.pool.as_ref())
        .await
        .map_err(|e| ConversationError::Database(e.to_string()))?;

        Ok(rows.into_iter().map(|(id,)| id).collect())
    }
}

/// Conversation errors
#[derive(Debug, thiserror::Error)]
pub enum ConversationError {
    #[error("Conversation not found")]
    NotFound,

    #[error("Database error: {0}")]
    Database(String),

    #[error("User is not a participant")]
    NotParticipant,
}
