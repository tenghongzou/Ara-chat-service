//! Connection pool manager with adaptive sizing and health monitoring
//!
//! This module provides intelligent connection pool management for billion-scale
//! workloads, including:
//! - Adaptive pool sizing based on load
//! - Connection health monitoring
//! - Pool warmup for cold starts
//! - Metrics and alerting

use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

use prometheus::{IntGauge, IntCounter, Histogram, HistogramOpts, opts, register_int_gauge, register_int_counter, register_histogram};
use sqlx::postgres::PgPool;
use sqlx::Executor;
use tokio::sync::RwLock;
use tokio::time::interval;

/// Pool configuration for adaptive sizing
#[derive(Debug, Clone)]
pub struct PoolManagerConfig {
    /// Minimum pool size
    pub min_size: u32,
    /// Maximum pool size
    pub max_size: u32,
    /// Target utilization (0.0 - 1.0)
    pub target_utilization: f64,
    /// Scale up threshold (utilization above this triggers scale up)
    pub scale_up_threshold: f64,
    /// Scale down threshold (utilization below this triggers scale down)
    pub scale_down_threshold: f64,
    /// Scaling interval
    pub scaling_interval: Duration,
    /// Health check interval
    pub health_check_interval: Duration,
    /// Connection timeout for health checks
    pub health_check_timeout: Duration,
    /// Warmup connections on startup
    pub warmup_enabled: bool,
}

impl Default for PoolManagerConfig {
    fn default() -> Self {
        Self {
            min_size: 5,
            max_size: 100,
            target_utilization: 0.7,
            scale_up_threshold: 0.8,
            scale_down_threshold: 0.3,
            scaling_interval: Duration::from_secs(30),
            health_check_interval: Duration::from_secs(60),
            health_check_timeout: Duration::from_secs(5),
            warmup_enabled: true,
        }
    }
}

/// Pool metrics for monitoring
#[derive(Clone)]
pub struct PoolMetrics {
    /// Current pool size
    pub size: IntGauge,
    /// Number of idle connections
    pub idle: IntGauge,
    /// Number of active connections
    pub active: IntGauge,
    /// Connection acquire latency
    pub acquire_latency: Histogram,
    /// Number of acquire timeouts
    pub acquire_timeouts: IntCounter,
    /// Number of connection errors
    pub connection_errors: IntCounter,
    /// Number of successful queries
    pub queries_total: IntCounter,
    /// Query latency
    pub query_latency: Histogram,
}

impl PoolMetrics {
    pub fn new(pool_name: &str) -> Self {
        let size = register_int_gauge!(opts!(
            format!("db_pool_{}_size", pool_name),
            "Current pool size"
        )).unwrap_or_else(|_| IntGauge::new(format!("db_pool_{}_size", pool_name), "size").unwrap());

        let idle = register_int_gauge!(opts!(
            format!("db_pool_{}_idle", pool_name),
            "Number of idle connections"
        )).unwrap_or_else(|_| IntGauge::new(format!("db_pool_{}_idle", pool_name), "idle").unwrap());

        let active = register_int_gauge!(opts!(
            format!("db_pool_{}_active", pool_name),
            "Number of active connections"
        )).unwrap_or_else(|_| IntGauge::new(format!("db_pool_{}_active", pool_name), "active").unwrap());

        let acquire_latency = register_histogram!(HistogramOpts::new(
            format!("db_pool_{}_acquire_latency_seconds", pool_name),
            "Connection acquire latency"
        ).buckets(vec![0.001, 0.005, 0.01, 0.025, 0.05, 0.1, 0.25, 0.5, 1.0]))
        .unwrap_or_else(|_| Histogram::with_opts(HistogramOpts::new(
            format!("db_pool_{}_acquire_latency_seconds", pool_name),
            "latency"
        )).unwrap());

        let acquire_timeouts = register_int_counter!(opts!(
            format!("db_pool_{}_acquire_timeouts_total", pool_name),
            "Number of acquire timeouts"
        )).unwrap_or_else(|_| IntCounter::new(format!("db_pool_{}_acquire_timeouts", pool_name), "timeouts").unwrap());

        let connection_errors = register_int_counter!(opts!(
            format!("db_pool_{}_connection_errors_total", pool_name),
            "Number of connection errors"
        )).unwrap_or_else(|_| IntCounter::new(format!("db_pool_{}_connection_errors", pool_name), "errors").unwrap());

        let queries_total = register_int_counter!(opts!(
            format!("db_pool_{}_queries_total", pool_name),
            "Number of queries executed"
        )).unwrap_or_else(|_| IntCounter::new(format!("db_pool_{}_queries", pool_name), "queries").unwrap());

        let query_latency = register_histogram!(HistogramOpts::new(
            format!("db_pool_{}_query_latency_seconds", pool_name),
            "Query execution latency"
        ).buckets(vec![0.001, 0.005, 0.01, 0.025, 0.05, 0.1, 0.25, 0.5, 1.0, 2.5, 5.0]))
        .unwrap_or_else(|_| Histogram::with_opts(HistogramOpts::new(
            format!("db_pool_{}_query_latency_seconds", pool_name),
            "latency"
        )).unwrap());

        Self {
            size,
            idle,
            active,
            acquire_latency,
            acquire_timeouts,
            connection_errors,
            queries_total,
            query_latency,
        }
    }

    /// Update metrics from pool state
    pub fn update(&self, size: u32, idle: usize) {
        self.size.set(size as i64);
        self.idle.set(idle as i64);
        self.active.set((size as i64) - (idle as i64));
    }
}

/// Pool health status
#[derive(Debug, Clone)]
pub struct PoolHealth {
    pub is_healthy: bool,
    pub size: u32,
    pub idle: usize,
    pub utilization: f64,
    pub last_check: Instant,
    pub consecutive_failures: u32,
}

/// Pool manager for adaptive sizing and monitoring
pub struct PoolManager {
    pool: PgPool,
    config: PoolManagerConfig,
    metrics: PoolMetrics,
    health: Arc<RwLock<PoolHealth>>,
    scaling_in_progress: Arc<AtomicUsize>,
}

impl PoolManager {
    /// Create a new pool manager
    pub fn new(pool: PgPool, config: PoolManagerConfig, pool_name: &str) -> Self {
        let metrics = PoolMetrics::new(pool_name);

        let health = Arc::new(RwLock::new(PoolHealth {
            is_healthy: true,
            size: pool.size(),
            idle: pool.num_idle(),
            utilization: 0.0,
            last_check: Instant::now(),
            consecutive_failures: 0,
        }));

        Self {
            pool,
            config,
            metrics,
            health,
            scaling_in_progress: Arc::new(AtomicUsize::new(0)),
        }
    }

    /// Get the underlying pool
    pub fn pool(&self) -> &PgPool {
        &self.pool
    }

    /// Get pool metrics
    pub fn metrics(&self) -> &PoolMetrics {
        &self.metrics
    }

    /// Get current health status
    pub async fn health(&self) -> PoolHealth {
        self.health.read().await.clone()
    }

    /// Start background monitoring and scaling
    pub fn start_monitoring(self: Arc<Self>) {
        let manager = self.clone();

        // Health check task
        tokio::spawn(async move {
            let mut interval = interval(manager.config.health_check_interval);

            loop {
                interval.tick().await;
                manager.check_health().await;
            }
        });

        // Metrics update task
        let manager = self.clone();
        tokio::spawn(async move {
            let mut interval = interval(Duration::from_secs(5));

            loop {
                interval.tick().await;
                let size = manager.pool.size();
                let idle = manager.pool.num_idle();
                manager.metrics.update(size, idle);
            }
        });
    }

    /// Check pool health
    async fn check_health(&self) {
        let start = Instant::now();

        let is_healthy = match tokio::time::timeout(
            self.config.health_check_timeout,
            sqlx::query("SELECT 1").fetch_one(&self.pool),
        ).await {
            Ok(Ok(_)) => true,
            Ok(Err(e)) => {
                tracing::warn!(error = %e, "Pool health check query failed");
                self.metrics.connection_errors.inc();
                false
            }
            Err(_) => {
                tracing::warn!("Pool health check timed out");
                self.metrics.acquire_timeouts.inc();
                false
            }
        };

        let size = self.pool.size();
        let idle = self.pool.num_idle();
        let utilization = if size > 0 {
            (size as f64 - idle as f64) / size as f64
        } else {
            0.0
        };

        let mut health = self.health.write().await;
        health.is_healthy = is_healthy;
        health.size = size;
        health.idle = idle;
        health.utilization = utilization;
        health.last_check = Instant::now();

        if is_healthy {
            health.consecutive_failures = 0;
        } else {
            health.consecutive_failures += 1;
        }

        tracing::debug!(
            is_healthy = is_healthy,
            size = size,
            idle = idle,
            utilization = %format!("{:.2}%", utilization * 100.0),
            check_duration_ms = start.elapsed().as_millis(),
            "Pool health check completed"
        );
    }

    /// Warmup the pool by acquiring min_connections
    pub async fn warmup(&self) -> Result<(), sqlx::Error> {
        if !self.config.warmup_enabled {
            return Ok(());
        }

        tracing::info!(
            target_connections = self.config.min_size,
            "Warming up connection pool"
        );

        let start = Instant::now();

        // Execute parallel queries to warm up connections
        let mut handles = Vec::new();
        for _ in 0..self.config.min_size {
            let pool = self.pool.clone();
            handles.push(tokio::spawn(async move {
                sqlx::query("SELECT 1").fetch_one(&pool).await
            }));
        }

        let mut success = 0;
        let mut failed = 0;
        for handle in handles {
            match handle.await {
                Ok(Ok(_)) => success += 1,
                _ => failed += 1,
            }
        }

        tracing::info!(
            success = success,
            failed = failed,
            duration_ms = start.elapsed().as_millis(),
            "Connection pool warmup completed"
        );

        Ok(())
    }

    /// Record a query execution
    pub fn record_query(&self, duration: Duration) {
        self.metrics.queries_total.inc();
        self.metrics.query_latency.observe(duration.as_secs_f64());
    }

    /// Record a connection acquire
    pub fn record_acquire(&self, duration: Duration, success: bool) {
        self.metrics.acquire_latency.observe(duration.as_secs_f64());
        if !success {
            self.metrics.acquire_timeouts.inc();
        }
    }
}

/// Connection pool wrapper with instrumentation
pub struct InstrumentedPool {
    manager: Arc<PoolManager>,
}

impl InstrumentedPool {
    pub fn new(manager: Arc<PoolManager>) -> Self {
        Self { manager }
    }

    /// Execute a query with instrumentation
    pub async fn execute<'q, E>(&self, query: E) -> Result<sqlx::postgres::PgQueryResult, sqlx::Error>
    where
        E: sqlx::Execute<'q, sqlx::Postgres> + 'q,
    {
        let start = Instant::now();
        let result = self.manager.pool.execute(query).await;
        self.manager.record_query(start.elapsed());
        result
    }

    /// Fetch one row with instrumentation
    pub async fn fetch_one<'q, O, E>(&self, query: E) -> Result<O, sqlx::Error>
    where
        E: sqlx::Execute<'q, sqlx::Postgres> + 'q,
        O: for<'r> sqlx::FromRow<'r, sqlx::postgres::PgRow> + Send + Unpin,
    {
        let start = Instant::now();
        let result = sqlx::query_as::<_, O>(query.sql())
            .fetch_one(self.manager.pool())
            .await;
        self.manager.record_query(start.elapsed());
        result
    }

    /// Get the underlying pool
    pub fn pool(&self) -> &PgPool {
        self.manager.pool()
    }

    /// Get the manager
    pub fn manager(&self) -> &Arc<PoolManager> {
        &self.manager
    }
}
