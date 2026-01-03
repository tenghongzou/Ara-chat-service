//! PostgreSQL connection pool

mod sharded_pool;
mod pool_manager;

pub use sharded_pool::{ShardedPool, ShardedPoolConfig, ShardedPoolError, ShardedPoolHealth, ShardedPoolStats};
pub use pool_manager::{PoolManager, PoolManagerConfig, PoolMetrics, PoolHealth, InstrumentedPool};

use sqlx::postgres::{PgPool, PgPoolOptions};
use sqlx::migrate::Migrator;
use std::path::Path;

use crate::config::DatabaseSettings;

/// PostgreSQL connection pool wrapper
pub struct PostgresPool {
    pool: PgPool,
}

impl PostgresPool {
    /// Create a new PostgreSQL pool
    pub async fn new(config: &DatabaseSettings) -> Result<Self, sqlx::Error> {
        let pool = PgPoolOptions::new()
            .max_connections(config.max_connections)
            .connect(&config.url)
            .await?;

        tracing::info!(
            max_connections = config.max_connections,
            "PostgreSQL pool created"
        );

        Ok(Self { pool })
    }

    /// Create a new pool and run migrations
    pub async fn new_with_migrations(
        config: &DatabaseSettings,
        migrations_path: &str,
    ) -> Result<Self, sqlx::Error> {
        let pool = Self::new(config).await?;

        if config.run_migrations {
            pool.run_migrations(migrations_path).await?;
        }

        Ok(pool)
    }

    /// Run database migrations
    pub async fn run_migrations(&self, migrations_path: &str) -> Result<(), sqlx::Error> {
        tracing::info!(path = %migrations_path, "Running database migrations");

        let migrator = Migrator::new(Path::new(migrations_path)).await?;
        migrator.run(&self.pool).await?;

        tracing::info!("Database migrations completed");
        Ok(())
    }

    /// Get a reference to the pool
    pub fn pool(&self) -> &PgPool {
        &self.pool
    }

    /// Check if the pool is healthy
    pub async fn is_healthy(&self) -> bool {
        sqlx::query("SELECT 1")
            .fetch_one(&self.pool)
            .await
            .is_ok()
    }
}
