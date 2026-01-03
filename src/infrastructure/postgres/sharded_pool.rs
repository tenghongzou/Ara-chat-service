//! Sharded PostgreSQL pool for horizontal scaling with Citus
//!
//! This module provides a sharded connection pool that distributes queries
//! across multiple PostgreSQL nodes using Citus distributed tables.

use std::collections::HashMap;
use std::sync::Arc;

use sqlx::postgres::{PgPool, PgPoolOptions};
use sqlx::{Executor, Row};
use tokio::sync::RwLock;
use uuid::Uuid;

use crate::sharding::{ShardId, ShardRouter, DEFAULT_SHARD_COUNT};

/// Configuration for a sharded PostgreSQL cluster
#[derive(Debug, Clone)]
pub struct ShardedPoolConfig {
    /// Coordinator node URL (for Citus)
    pub coordinator_url: String,
    /// Worker node URLs (node_id -> url)
    pub worker_urls: HashMap<String, String>,
    /// Maximum connections per node
    pub max_connections_per_node: u32,
    /// Minimum connections per node
    pub min_connections_per_node: u32,
    /// Connection acquire timeout in seconds
    pub acquire_timeout_seconds: u64,
    /// Connection idle timeout in seconds
    pub idle_timeout_seconds: u64,
    /// Whether to use direct worker connections for reads
    pub direct_reads: bool,
    /// Number of shards
    pub shard_count: u32,
}

impl Default for ShardedPoolConfig {
    fn default() -> Self {
        Self {
            coordinator_url: "postgres://localhost:5432/chat".to_string(),
            worker_urls: HashMap::new(),
            max_connections_per_node: 20,
            min_connections_per_node: 5,
            acquire_timeout_seconds: 30,
            idle_timeout_seconds: 300,
            direct_reads: false,
            shard_count: DEFAULT_SHARD_COUNT,
        }
    }
}

/// Sharded PostgreSQL pool supporting Citus distributed tables
pub struct ShardedPool {
    /// Coordinator pool (for distributed queries and DDL)
    coordinator: PgPool,
    /// Worker pools (node_id -> pool)
    workers: HashMap<String, PgPool>,
    /// Shard router for determining which node handles which shard
    router: Arc<RwLock<ShardRouter>>,
    /// Configuration
    config: ShardedPoolConfig,
}

impl ShardedPool {
    /// Create a new sharded pool
    pub async fn new(config: ShardedPoolConfig) -> Result<Self, ShardedPoolError> {
        // Create coordinator pool
        let coordinator = Self::create_pool(&config.coordinator_url, &config).await?;

        // Create worker pools
        let mut workers = HashMap::new();
        for (node_id, url) in &config.worker_urls {
            let pool = Self::create_pool(url, &config).await?;
            workers.insert(node_id.clone(), pool);
        }

        // Initialize shard router
        let mut router = ShardRouter::new(config.shard_count, 150); // 150 virtual nodes per worker
        for node_id in config.worker_urls.keys() {
            router.add_node(node_id);
        }

        tracing::info!(
            coordinator = %config.coordinator_url,
            workers = config.worker_urls.len(),
            shards = config.shard_count,
            "Sharded PostgreSQL pool created"
        );

        Ok(Self {
            coordinator,
            workers,
            router: Arc::new(RwLock::new(router)),
            config,
        })
    }

    async fn create_pool(url: &str, config: &ShardedPoolConfig) -> Result<PgPool, ShardedPoolError> {
        PgPoolOptions::new()
            .max_connections(config.max_connections_per_node)
            .min_connections(config.min_connections_per_node)
            .acquire_timeout(std::time::Duration::from_secs(config.acquire_timeout_seconds))
            .idle_timeout(std::time::Duration::from_secs(config.idle_timeout_seconds))
            .connect(url)
            .await
            .map_err(|e| ShardedPoolError::Connection(e.to_string()))
    }

    /// Get the coordinator pool (for distributed queries)
    pub fn coordinator(&self) -> &PgPool {
        &self.coordinator
    }

    /// Get a worker pool by node ID
    pub fn worker(&self, node_id: &str) -> Option<&PgPool> {
        self.workers.get(node_id)
    }

    /// Get the pool for a specific user (for user-scoped queries)
    pub async fn pool_for_user(&self, user_id: Uuid) -> &PgPool {
        if !self.config.direct_reads || self.workers.is_empty() {
            return &self.coordinator;
        }

        let router = self.router.read().await;
        if let Some(node_id) = router.node_for_user(user_id) {
            if let Some(pool) = self.workers.get(node_id) {
                return pool;
            }
        }

        &self.coordinator
    }

    /// Get the pool for a specific conversation
    pub async fn pool_for_conversation(&self, conversation_id: Uuid) -> &PgPool {
        if !self.config.direct_reads || self.workers.is_empty() {
            return &self.coordinator;
        }

        let router = self.router.read().await;
        if let Some(node_id) = router.node_for_conversation(conversation_id) {
            if let Some(pool) = self.workers.get(node_id) {
                return pool;
            }
        }

        &self.coordinator
    }

    /// Get the shard ID for a user
    pub async fn shard_for_user(&self, user_id: Uuid) -> ShardId {
        let router = self.router.read().await;
        router.shard_for_user(user_id)
    }

    /// Get the shard ID for a conversation
    pub async fn shard_for_conversation(&self, conversation_id: Uuid) -> ShardId {
        let router = self.router.read().await;
        router.shard_for_conversation(conversation_id)
    }

    /// Add a worker node dynamically
    pub async fn add_worker(&self, node_id: &str, url: &str) -> Result<(), ShardedPoolError> {
        // Note: In production, you'd need interior mutability for workers HashMap
        // This is a simplified implementation
        let mut router = self.router.write().await;
        router.add_node(node_id);

        tracing::info!(node_id = %node_id, "Added worker node to shard router");
        Ok(())
    }

    /// Remove a worker node dynamically
    pub async fn remove_worker(&self, node_id: &str) {
        let mut router = self.router.write().await;
        router.remove_node(node_id);

        tracing::info!(node_id = %node_id, "Removed worker node from shard router");
    }

    /// Initialize Citus distribution for tables
    pub async fn init_citus_distribution(&self) -> Result<(), ShardedPoolError> {
        // Check if Citus is available
        let has_citus: bool = sqlx::query_scalar(
            "SELECT EXISTS(SELECT 1 FROM pg_extension WHERE extname = 'citus')"
        )
        .fetch_one(&self.coordinator)
        .await
        .map_err(|e| ShardedPoolError::Query(e.to_string()))?;

        if !has_citus {
            tracing::warn!("Citus extension not found, skipping distribution setup");
            return Ok(());
        }

        // Distribute tables by their shard columns
        let distributions = vec![
            ("conversations", "id"),
            ("conversation_participants", "conversation_id"),
            ("messages", "conversation_id"),
            ("message_reactions", "message_id"),
            ("read_receipts", "conversation_id"),
        ];

        for (table, column) in distributions {
            let query = format!(
                "SELECT create_distributed_table('{}', '{}', colocate_with => 'conversations')
                 WHERE NOT EXISTS (
                     SELECT 1 FROM citus_tables WHERE table_name = '{}'
                 )",
                table, column, table
            );

            match self.coordinator.execute(query.as_str()).await {
                Ok(_) => tracing::info!(table = %table, column = %column, "Distributed table"),
                Err(e) => {
                    tracing::warn!(table = %table, error = %e, "Failed to distribute table (may already exist)");
                }
            }
        }

        // Create reference tables for small, frequently-accessed data
        let reference_tables = vec!["direct_message_lookup"];
        for table in reference_tables {
            let query = format!(
                "SELECT create_reference_table('{}')
                 WHERE NOT EXISTS (
                     SELECT 1 FROM citus_tables WHERE table_name = '{}'
                 )",
                table, table
            );

            match self.coordinator.execute(query.as_str()).await {
                Ok(_) => tracing::info!(table = %table, "Created reference table"),
                Err(e) => {
                    tracing::warn!(table = %table, error = %e, "Failed to create reference table");
                }
            }
        }

        tracing::info!("Citus distribution initialized");
        Ok(())
    }

    /// Get cluster health status
    pub async fn health_check(&self) -> ShardedPoolHealth {
        let coordinator_healthy = sqlx::query("SELECT 1")
            .fetch_one(&self.coordinator)
            .await
            .is_ok();

        let mut worker_health = HashMap::new();
        for (node_id, pool) in &self.workers {
            let healthy = sqlx::query("SELECT 1")
                .fetch_one(pool)
                .await
                .is_ok();
            worker_health.insert(node_id.clone(), healthy);
        }

        let healthy_workers = worker_health.values().filter(|&&h| h).count();

        ShardedPoolHealth {
            coordinator_healthy,
            worker_health,
            total_workers: self.workers.len(),
            healthy_workers,
        }
    }

    /// Get pool statistics
    pub async fn stats(&self) -> ShardedPoolStats {
        let coord_size = self.coordinator.size();
        let coord_idle = self.coordinator.num_idle();

        let mut worker_stats = HashMap::new();
        for (node_id, pool) in &self.workers {
            worker_stats.insert(node_id.clone(), PoolNodeStats {
                size: pool.size(),
                idle: pool.num_idle(),
            });
        }

        ShardedPoolStats {
            coordinator: PoolNodeStats {
                size: coord_size,
                idle: coord_idle,
            },
            workers: worker_stats,
        }
    }
}

/// Health status of the sharded pool
#[derive(Debug)]
pub struct ShardedPoolHealth {
    pub coordinator_healthy: bool,
    pub worker_health: HashMap<String, bool>,
    pub total_workers: usize,
    pub healthy_workers: usize,
}

impl ShardedPoolHealth {
    pub fn is_healthy(&self) -> bool {
        self.coordinator_healthy && self.healthy_workers > 0
    }

    pub fn is_degraded(&self) -> bool {
        self.coordinator_healthy && self.healthy_workers < self.total_workers
    }
}

/// Statistics for a single pool node
#[derive(Debug)]
pub struct PoolNodeStats {
    pub size: u32,
    pub idle: usize,
}

/// Statistics for the entire sharded pool
#[derive(Debug)]
pub struct ShardedPoolStats {
    pub coordinator: PoolNodeStats,
    pub workers: HashMap<String, PoolNodeStats>,
}

/// Errors from the sharded pool
#[derive(Debug, thiserror::Error)]
pub enum ShardedPoolError {
    #[error("Connection error: {0}")]
    Connection(String),

    #[error("Query error: {0}")]
    Query(String),

    #[error("Shard not found: {0}")]
    ShardNotFound(ShardId),

    #[error("Worker not found: {0}")]
    WorkerNotFound(String),
}
