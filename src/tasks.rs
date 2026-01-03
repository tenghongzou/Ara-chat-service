//! Background tasks for production operations
//!
//! Includes:
//! - Heartbeat monitoring
//! - Metrics collection
//! - Session refresh
//! - Stale connection cleanup
//! - Partition management (permanent storage)

use std::time::{Duration, Instant};

use chrono::Utc;
use tokio::sync::broadcast;

use crate::metrics;
use crate::server::AppState;

/// Manages background tasks
pub struct BackgroundTasks {
    state: AppState,
    shutdown_rx: broadcast::Receiver<()>,
}

impl BackgroundTasks {
    pub fn new(state: AppState, shutdown_rx: broadcast::Receiver<()>) -> Self {
        Self { state, shutdown_rx }
    }

    /// Run all background tasks
    pub async fn run(self) {
        let state = self.state;
        let mut shutdown_rx = self.shutdown_rx;

        // Clone state for each task
        let heartbeat_state = state.clone();
        let metrics_state = state.clone();
        let session_state = state.clone();
        let stale_conn_state = state.clone();
        let partition_state = state.clone();

        // Track active tasks
        metrics::ACTIVE_TASKS.set(5);

        let heartbeat = run_heartbeat(heartbeat_state);
        let metrics_update = run_metrics_update(metrics_state);
        let session_refresh = run_session_refresh(session_state);
        let stale_cleanup = run_stale_connection_cleanup(stale_conn_state);
        let partition_mgmt = run_partition_management(partition_state);

        tokio::select! {
            _ = heartbeat => {
                tracing::warn!("Heartbeat task exited unexpectedly");
            },
            _ = metrics_update => {
                tracing::warn!("Metrics update task exited unexpectedly");
            },
            _ = session_refresh => {
                tracing::warn!("Session refresh task exited unexpectedly");
            },
            _ = stale_cleanup => {
                tracing::warn!("Stale connection cleanup task exited unexpectedly");
            },
            _ = partition_mgmt => {
                tracing::warn!("Partition management task exited unexpectedly");
            },
            _ = shutdown_rx.recv() => {
                tracing::info!("Background tasks received shutdown signal");
            }
        }

        metrics::ACTIVE_TASKS.set(0);
    }
}

/// Heartbeat task - sends periodic heartbeats and detects stale connections
async fn run_heartbeat(state: AppState) {
    let interval = Duration::from_secs(state.settings.websocket.heartbeat_interval_seconds);
    let mut ticker = tokio::time::interval(interval);

    loop {
        ticker.tick().await;

        let start = Instant::now();

        // Broadcast heartbeat to all connections
        // The connection manager will handle individual connection health
        let connection_count = state.connection_manager.total_connections();

        tracing::trace!(
            connections = connection_count,
            duration_ms = start.elapsed().as_millis(),
            "Heartbeat tick"
        );
    }
}

/// Metrics update task - updates Prometheus metrics periodically
async fn run_metrics_update(state: AppState) {
    let mut ticker = tokio::time::interval(Duration::from_secs(15));
    let start_time = Instant::now();

    loop {
        ticker.tick().await;

        // Connection metrics
        let connections = state.connection_manager.total_connections();
        let users = state.connection_manager.unique_users();
        metrics::update_connection_metrics(connections, users);

        // Server uptime
        metrics::SERVER_UPTIME.set(start_time.elapsed().as_secs_f64());

        // Update connections by server
        metrics::CONNECTIONS_BY_SERVER
            .with_label_values(&[&state.settings.cluster.server_id])
            .set(connections as i64);

        // Database pool metrics if available
        if let Some(ref pool) = state.postgres_pool {
            let pool_ref = pool.pool();
            metrics::DB_POOL_SIZE
                .with_label_values(&["main", "active"])
                .set((pool_ref.size() - pool_ref.num_idle() as u32) as i64);
            metrics::DB_POOL_SIZE
                .with_label_values(&["main", "idle"])
                .set(pool_ref.num_idle() as i64);
        }

        tracing::debug!(
            connections = connections,
            users = users,
            uptime_secs = start_time.elapsed().as_secs(),
            "Updated metrics"
        );
    }
}

/// Session refresh task - refreshes TTL for cluster sessions
async fn run_session_refresh(state: AppState) {
    if !state.settings.cluster.enabled {
        std::future::pending::<()>().await;
        return;
    }

    let mut ticker = tokio::time::interval(Duration::from_secs(60));

    loop {
        ticker.tick().await;

        if let Some(ref store) = state.session_store {
            match store.refresh_sessions().await {
                Ok(count) => {
                    metrics::CLUSTER_SESSIONS.set(count as i64);
                    tracing::debug!(refreshed = count, "Refreshed cluster sessions");
                }
                Err(e) => {
                    tracing::warn!(error = %e, "Failed to refresh cluster sessions");
                }
            }
        }
    }
}

/// Stale connection cleanup - removes connections that haven't responded to heartbeats
async fn run_stale_connection_cleanup(state: AppState) {
    let timeout = Duration::from_secs(state.settings.websocket.connection_timeout_seconds);
    let mut ticker = tokio::time::interval(Duration::from_secs(30));

    loop {
        ticker.tick().await;

        let stale_count = state.connection_manager.cleanup_stale_connections(timeout).await;

        if stale_count > 0 {
            tracing::info!(
                cleaned = stale_count,
                timeout_secs = timeout.as_secs(),
                "Cleaned up stale connections"
            );
        }
    }
}

/// Partition management task - creates new partitions for permanent message storage
/// Messages are stored permanently without automatic deletion
async fn run_partition_management(state: AppState) {
    // Run daily at 2 AM UTC
    let mut ticker = tokio::time::interval(Duration::from_secs(3600));

    loop {
        ticker.tick().await;

        let now = Utc::now();

        // Only run at 2 AM UTC
        if now.hour() != 2 {
            continue;
        }

        let Some(ref pool) = state.postgres_pool else {
            continue;
        };

        tracing::info!("Starting partition management (permanent storage mode)");

        // Create partitions for the next 7 days
        for days_ahead in 0..7 {
            let partition_date = (now + chrono::Duration::days(days_ahead)).date_naive();
            let partition_name = format!("messages_p{}", partition_date.format("%Y%m%d"));
            let next_date = partition_date + chrono::Duration::days(1);

            let create_query = format!(
                r#"
                CREATE TABLE IF NOT EXISTS {} PARTITION OF messages
                FOR VALUES FROM ('{}') TO ('{}')
                "#,
                partition_name, partition_date, next_date
            );

            if let Err(e) = sqlx::query(&create_query).execute(pool.pool()).await {
                // Ignore "already exists" errors
                let err_str = e.to_string();
                if !err_str.contains("already exists") {
                    tracing::error!(
                        error = %e,
                        partition = partition_name,
                        "Failed to create partition"
                    );
                }
            } else {
                tracing::debug!(partition = partition_name, "Created partition");
            }
        }

        // Create reactions partitions for the next 7 days
        for days_ahead in 0..7 {
            let partition_date = (now + chrono::Duration::days(days_ahead)).date_naive();
            let partition_name = format!("message_reactions_p{}", partition_date.format("%Y%m%d"));
            let next_date = partition_date + chrono::Duration::days(1);

            let create_query = format!(
                r#"
                CREATE TABLE IF NOT EXISTS {} PARTITION OF message_reactions
                FOR VALUES FROM ('{}') TO ('{}')
                "#,
                partition_name, partition_date, next_date
            );

            if let Err(e) = sqlx::query(&create_query).execute(pool.pool()).await {
                let err_str = e.to_string();
                if !err_str.contains("already exists") {
                    tracing::error!(
                        error = %e,
                        partition = partition_name,
                        "Failed to create reactions partition"
                    );
                }
            }
        }

        tracing::info!("Partition management completed (no cleanup - permanent storage)");

        // Wait until next hour
        tokio::time::sleep(Duration::from_secs(3600)).await;
    }
}

use chrono::Timelike;
