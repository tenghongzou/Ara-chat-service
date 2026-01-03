//! Sharding infrastructure for billion-scale user distribution
//!
//! This module provides consistent hashing-based sharding for distributing
//! users across 1024 shards, enabling horizontal scaling for 100M+ DAU.

use std::hash::{Hash, Hasher};
use std::collections::hash_map::DefaultHasher;
use uuid::Uuid;

/// Number of shards for user distribution
pub const DEFAULT_SHARD_COUNT: u32 = 1024;

/// Shard identifier
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct ShardId(pub u32);

impl ShardId {
    pub fn as_u32(&self) -> u32 {
        self.0
    }

    /// Get the shard group (for Redis Cluster slot mapping)
    /// Groups shards into 16 groups for Redis Cluster's 16384 slots
    pub fn shard_group(&self) -> u32 {
        self.0 / 64 // 1024 shards / 16 groups = 64 shards per group
    }
}

impl std::fmt::Display for ShardId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "shard-{:04}", self.0)
    }
}

/// User sharding strategy
#[derive(Debug, Clone)]
pub struct UserSharder {
    shard_count: u32,
}

impl Default for UserSharder {
    fn default() -> Self {
        Self::new(DEFAULT_SHARD_COUNT)
    }
}

impl UserSharder {
    /// Create a new user sharder with the specified number of shards
    pub fn new(shard_count: u32) -> Self {
        assert!(shard_count > 0 && shard_count.is_power_of_two(),
            "Shard count must be a power of 2");
        Self { shard_count }
    }

    /// Get the shard ID for a user
    pub fn shard_for_user(&self, user_id: Uuid) -> ShardId {
        let hash = self.hash_uuid(user_id);
        ShardId((hash as u32) % self.shard_count)
    }

    /// Get the shard ID for a conversation
    /// Conversations are sharded by their ID to keep all messages together
    pub fn shard_for_conversation(&self, conversation_id: Uuid) -> ShardId {
        let hash = self.hash_uuid(conversation_id);
        ShardId((hash as u32) % self.shard_count)
    }

    /// Get the shard ID for a message
    /// Messages are sharded by conversation_id to keep messages together
    pub fn shard_for_message(&self, conversation_id: Uuid) -> ShardId {
        self.shard_for_conversation(conversation_id)
    }

    /// Hash a UUID consistently
    fn hash_uuid(&self, id: Uuid) -> u64 {
        let mut hasher = DefaultHasher::new();
        id.hash(&mut hasher);
        hasher.finish()
    }

    /// Get the total number of shards
    pub fn shard_count(&self) -> u32 {
        self.shard_count
    }

    /// Get all shard IDs
    pub fn all_shards(&self) -> Vec<ShardId> {
        (0..self.shard_count).map(ShardId).collect()
    }

    /// Get shards in a specific group (for batch operations)
    pub fn shards_in_group(&self, group: u32, group_count: u32) -> Vec<ShardId> {
        let shards_per_group = self.shard_count / group_count;
        let start = group * shards_per_group;
        let end = start + shards_per_group;
        (start..end).map(ShardId).collect()
    }
}

/// Consistent hash ring for dynamic shard assignment
/// Used for distributing load across multiple database nodes
#[derive(Debug, Clone)]
pub struct ConsistentHashRing {
    nodes: Vec<(u64, String)>, // (hash, node_id)
    virtual_nodes: u32,
}

impl ConsistentHashRing {
    /// Create a new consistent hash ring
    pub fn new(virtual_nodes: u32) -> Self {
        Self {
            nodes: Vec::new(),
            virtual_nodes,
        }
    }

    /// Add a node to the ring
    pub fn add_node(&mut self, node_id: &str) {
        for i in 0..self.virtual_nodes {
            let virtual_id = format!("{}#{}", node_id, i);
            let hash = self.hash_string(&virtual_id);
            self.nodes.push((hash, node_id.to_string()));
        }
        self.nodes.sort_by_key(|(hash, _)| *hash);
    }

    /// Remove a node from the ring
    pub fn remove_node(&mut self, node_id: &str) {
        self.nodes.retain(|(_, id)| id != node_id);
    }

    /// Get the node for a given key
    pub fn get_node(&self, key: &str) -> Option<&str> {
        if self.nodes.is_empty() {
            return None;
        }

        let hash = self.hash_string(key);

        // Binary search for the first node with hash >= key hash
        let idx = match self.nodes.binary_search_by_key(&hash, |(h, _)| *h) {
            Ok(i) => i,
            Err(i) => i % self.nodes.len(),
        };

        Some(&self.nodes[idx].1)
    }

    /// Get the node for a shard
    pub fn get_node_for_shard(&self, shard_id: ShardId) -> Option<&str> {
        self.get_node(&shard_id.to_string())
    }

    fn hash_string(&self, s: &str) -> u64 {
        let mut hasher = DefaultHasher::new();
        s.hash(&mut hasher);
        hasher.finish()
    }

    /// Get all nodes in the ring
    pub fn nodes(&self) -> Vec<&str> {
        let mut unique: Vec<&str> = self.nodes.iter().map(|(_, id)| id.as_str()).collect();
        unique.sort();
        unique.dedup();
        unique
    }
}

/// Shard routing configuration
#[derive(Debug, Clone)]
pub struct ShardRouter {
    sharder: UserSharder,
    ring: ConsistentHashRing,
}

impl ShardRouter {
    /// Create a new shard router
    pub fn new(shard_count: u32, virtual_nodes: u32) -> Self {
        Self {
            sharder: UserSharder::new(shard_count),
            ring: ConsistentHashRing::new(virtual_nodes),
        }
    }

    /// Add a database node
    pub fn add_node(&mut self, node_id: &str) {
        self.ring.add_node(node_id);
    }

    /// Remove a database node
    pub fn remove_node(&mut self, node_id: &str) {
        self.ring.remove_node(node_id);
    }

    /// Get the database node for a user
    pub fn node_for_user(&self, user_id: Uuid) -> Option<&str> {
        let shard = self.sharder.shard_for_user(user_id);
        self.ring.get_node_for_shard(shard)
    }

    /// Get the database node for a conversation
    pub fn node_for_conversation(&self, conversation_id: Uuid) -> Option<&str> {
        let shard = self.sharder.shard_for_conversation(conversation_id);
        self.ring.get_node_for_shard(shard)
    }

    /// Get the shard for a user
    pub fn shard_for_user(&self, user_id: Uuid) -> ShardId {
        self.sharder.shard_for_user(user_id)
    }

    /// Get the shard for a conversation
    pub fn shard_for_conversation(&self, conversation_id: Uuid) -> ShardId {
        self.sharder.shard_for_conversation(conversation_id)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_shard_distribution() {
        let sharder = UserSharder::default();

        // Test that same user always gets same shard
        let user_id = Uuid::new_v4();
        let shard1 = sharder.shard_for_user(user_id);
        let shard2 = sharder.shard_for_user(user_id);
        assert_eq!(shard1, shard2);

        // Test distribution across shards
        let mut shard_counts = vec![0u32; DEFAULT_SHARD_COUNT as usize];
        for _ in 0..100_000 {
            let user_id = Uuid::new_v4();
            let shard = sharder.shard_for_user(user_id);
            shard_counts[shard.0 as usize] += 1;
        }

        // Check that distribution is relatively even
        let avg = 100_000 / DEFAULT_SHARD_COUNT;
        let variance = shard_counts.iter()
            .map(|&c| (c as i64 - avg as i64).abs() as u64)
            .sum::<u64>() / DEFAULT_SHARD_COUNT as u64;

        // Variance should be small relative to average
        assert!(variance < avg as u64 / 2, "Distribution too uneven: variance={}", variance);
    }

    #[test]
    fn test_consistent_hash_ring() {
        let mut ring = ConsistentHashRing::new(100);
        ring.add_node("node-1");
        ring.add_node("node-2");
        ring.add_node("node-3");

        // Same key should always map to same node
        let node1 = ring.get_node("test-key-1").unwrap();
        let node2 = ring.get_node("test-key-1").unwrap();
        assert_eq!(node1, node2);

        // Different keys should distribute across nodes
        let mut node_counts = std::collections::HashMap::new();
        for i in 0..1000 {
            let key = format!("key-{}", i);
            let node = ring.get_node(&key).unwrap();
            *node_counts.entry(node.to_string()).or_insert(0) += 1;
        }

        // All nodes should receive some keys
        assert_eq!(node_counts.len(), 3);
        for count in node_counts.values() {
            assert!(*count > 100, "Node received too few keys");
        }
    }
}
