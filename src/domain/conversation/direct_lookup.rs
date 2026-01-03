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

    #[test]
    fn test_pair_hash_deterministic() {
        // Same pair should always produce the same hash
        let user_a = Uuid::parse_str("550e8400-e29b-41d4-a716-446655440000").unwrap();
        let user_b = Uuid::parse_str("6ba7b810-9dad-11d1-80b4-00c04fd430c8").unwrap();

        let hash1 = DirectMessageLookup::generate_pair_hash(user_a, user_b);
        let hash2 = DirectMessageLookup::generate_pair_hash(user_a, user_b);
        let hash3 = DirectMessageLookup::generate_pair_hash(user_b, user_a);

        assert_eq!(hash1, hash2);
        assert_eq!(hash2, hash3);
    }

    #[test]
    fn test_pair_hash_byte_length() {
        // SHA-256 produces 32 bytes (256 bits)
        let user_a = Uuid::new_v4();
        let user_b = Uuid::new_v4();

        let hash = DirectMessageLookup::generate_pair_hash(user_a, user_b);

        assert_eq!(hash.len(), 32);
    }

    #[test]
    fn test_pair_hash_same_user() {
        // Edge case: user chatting with themselves
        let user = Uuid::new_v4();

        let hash = DirectMessageLookup::generate_pair_hash(user, user);

        // Should still produce a valid 32-byte hash
        assert_eq!(hash.len(), 32);
    }

    #[test]
    fn test_pair_hash_distribution() {
        // Generate many hashes and verify they're all unique
        let users: Vec<Uuid> = (0..10).map(|_| Uuid::new_v4()).collect();
        let mut hashes = std::collections::HashSet::new();

        // Generate all unique pairs
        for i in 0..users.len() {
            for j in (i + 1)..users.len() {
                let hash = DirectMessageLookup::generate_pair_hash(users[i], users[j]);
                hashes.insert(hash);
            }
        }

        // 10 users = 10*9/2 = 45 unique pairs
        assert_eq!(hashes.len(), 45);
    }

    #[test]
    fn test_pair_hash_specific_uuids() {
        // Test with specific UUIDs to ensure consistent behavior across runs
        let user_a = Uuid::parse_str("00000000-0000-0000-0000-000000000001").unwrap();
        let user_b = Uuid::parse_str("00000000-0000-0000-0000-000000000002").unwrap();

        let hash_ab = DirectMessageLookup::generate_pair_hash(user_a, user_b);
        let hash_ba = DirectMessageLookup::generate_pair_hash(user_b, user_a);

        // Both should be identical
        assert_eq!(hash_ab, hash_ba);

        // Hash should be non-zero
        assert!(hash_ab.iter().any(|&b| b != 0));
    }
}
