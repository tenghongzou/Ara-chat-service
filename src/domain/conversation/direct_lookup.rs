//! Direct message lookup - O(1) lookup for private conversations

use std::sync::Arc;

use sha2::{Sha256, Digest};
use sqlx::{PgPool, Postgres, Transaction};
use uuid::Uuid;

use super::service::ConversationError;

/// Fast lookup for direct (1:1) conversations
pub struct DirectMessageLookup {
    pool: Arc<PgPool>,
}

impl DirectMessageLookup {
    pub fn new(pool: Arc<PgPool>) -> Self {
        Self { pool }
    }

    /// Generate a deterministic hash for a user pair
    /// Ensures (user_a, user_b) == (user_b, user_a)
    pub fn generate_pair_hash(user1: Uuid, user2: Uuid) -> Vec<u8> {
        let mut hasher = Sha256::new();

        // Sort UUIDs to ensure consistent ordering
        let (first, second) = if user1 < user2 {
            (user1, user2)
        } else {
            (user2, user1)
        };

        hasher.update(first.as_bytes());
        hasher.update(second.as_bytes());

        hasher.finalize().to_vec()
    }

    /// Find existing direct conversation between two users
    pub async fn find_direct_conversation(
        &self,
        user1: Uuid,
        user2: Uuid,
        tenant_id: &str,
    ) -> Result<Option<Uuid>, ConversationError> {
        let hash = Self::generate_pair_hash(user1, user2);

        let result: Option<(Uuid,)> = sqlx::query_as(
            r#"
            SELECT conversation_id
            FROM direct_message_lookup
            WHERE user_pair_hash = $1 AND tenant_id = $2
            "#,
        )
        .bind(&hash)
        .bind(tenant_id)
        .fetch_optional(self.pool.as_ref())
        .await
        .map_err(|e| ConversationError::Database(e.to_string()))?;

        Ok(result.map(|(id,)| id))
    }

    /// Register a new direct conversation (within a transaction)
    pub async fn register_direct_conversation_tx(
        &self,
        tx: &mut Transaction<'_, Postgres>,
        user1: Uuid,
        user2: Uuid,
        conversation_id: Uuid,
        tenant_id: &str,
    ) -> Result<(), ConversationError> {
        let hash = Self::generate_pair_hash(user1, user2);

        // Sort users for consistent storage
        let (first, second) = if user1 < user2 {
            (user1, user2)
        } else {
            (user2, user1)
        };

        sqlx::query(
            r#"
            INSERT INTO direct_message_lookup (user_pair_hash, conversation_id, user1_id, user2_id, tenant_id)
            VALUES ($1, $2, $3, $4, $5)
            ON CONFLICT (user_pair_hash) DO NOTHING
            "#,
        )
        .bind(&hash)
        .bind(conversation_id)
        .bind(first)
        .bind(second)
        .bind(tenant_id)
        .execute(&mut **tx)
        .await
        .map_err(|e| ConversationError::Database(e.to_string()))?;

        Ok(())
    }

    /// Register a new direct conversation (standalone)
    pub async fn register_direct_conversation(
        &self,
        user1: Uuid,
        user2: Uuid,
        conversation_id: Uuid,
        tenant_id: &str,
    ) -> Result<(), ConversationError> {
        let hash = Self::generate_pair_hash(user1, user2);

        // Sort users for consistent storage
        let (first, second) = if user1 < user2 {
            (user1, user2)
        } else {
            (user2, user1)
        };

        sqlx::query(
            r#"
            INSERT INTO direct_message_lookup (user_pair_hash, conversation_id, user1_id, user2_id, tenant_id)
            VALUES ($1, $2, $3, $4, $5)
            ON CONFLICT (user_pair_hash) DO NOTHING
            "#,
        )
        .bind(&hash)
        .bind(conversation_id)
        .bind(first)
        .bind(second)
        .bind(tenant_id)
        .execute(self.pool.as_ref())
        .await
        .map_err(|e| ConversationError::Database(e.to_string()))?;

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_pair_hash_is_order_independent() {
        let user_a = Uuid::new_v4();
        let user_b = Uuid::new_v4();

        let hash_ab = DirectMessageLookup::generate_pair_hash(user_a, user_b);
        let hash_ba = DirectMessageLookup::generate_pair_hash(user_b, user_a);

        assert_eq!(hash_ab, hash_ba);
    }

    #[test]
    fn test_different_pairs_have_different_hashes() {
        let user_a = Uuid::new_v4();
        let user_b = Uuid::new_v4();
        let user_c = Uuid::new_v4();

        let hash_ab = DirectMessageLookup::generate_pair_hash(user_a, user_b);
        let hash_ac = DirectMessageLookup::generate_pair_hash(user_a, user_c);

        assert_ne!(hash_ab, hash_ac);
    }
}
