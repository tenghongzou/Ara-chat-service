//! Health check endpoints for production monitoring
//!
//! Provides:
//! - /health - Basic health check
//! - /health/live - Kubernetes liveness probe
//! - /health/ready - Kubernetes readiness probe
//! - /health/detailed - Comprehensive health report
//! - /metrics - Prometheus metrics endpoint

use std::time::Instant;

use axum::{
    extract::State,
    http::StatusCode,
    response::IntoResponse,
    Json,
};
use serde::Serialize;

use crate::metrics;
use crate::server::AppState;

/// Basic health response
#[derive(Serialize)]
pub struct HealthResponse {
    pub status: &'static str,
    pub version: &'static str,
    pub connections: usize,
    pub users: usize,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub redis: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub postgres: Option<bool>,
}

/// Liveness probe response
#[derive(Serialize)]
pub struct LivenessResponse {
    pub alive: bool,
    pub uptime_seconds: u64,
}

/// Readiness probe response
#[derive(Serialize)]
pub struct ReadinessResponse {
    pub ready: bool,
    pub checks: ReadinessChecks,
}

#[derive(Serialize)]
pub struct ReadinessChecks {
    pub redis: CheckResult,
    pub postgres: CheckResult,
    pub connections_available: bool,
}

#[derive(Serialize)]
pub struct CheckResult {
    pub healthy: bool,
    pub latency_ms: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

/// Detailed health report
#[derive(Serialize)]
pub struct DetailedHealthResponse {
    pub status: &'static str,
    pub version: &'static str,
    pub server_id: String,
    pub uptime_seconds: f64,
    pub connections: ConnectionStats,
    pub services: ServiceHealth,
    pub circuit_breakers: CircuitBreakerHealth,
    pub rate_limiting: RateLimitHealth,
    pub memory: MemoryStats,
}

#[derive(Serialize)]
pub struct ConnectionStats {
    pub total: usize,
    pub unique_users: usize,
    pub connections_per_user: f64,
}

#[derive(Serialize)]
pub struct ServiceHealth {
    pub redis: ServiceStatus,
    pub postgres: ServiceStatus,
    pub cluster: ServiceStatus,
}

#[derive(Serialize)]
pub struct ServiceStatus {
    pub status: &'static str,
    pub latency_ms: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub pool_size: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub pool_idle: Option<usize>,
}

#[derive(Serialize)]
pub struct CircuitBreakerHealth {
    pub redis: &'static str,
    pub postgres: &'static str,
    pub cluster: &'static str,
}

#[derive(Serialize)]
pub struct RateLimitHealth {
    pub enabled: bool,
    pub current_usage: f64,
}

#[derive(Serialize)]
pub struct MemoryStats {
    pub allocated_mb: f64,
}

// ========================================
// Handler Functions
// ========================================

/// Basic health check - /health
pub async fn health_check(
    State(state): State<AppState>,
) -> Result<Json<HealthResponse>, StatusCode> {
    let connections = state.connection_manager.total_connections();
    let users = state.connection_manager.unique_users();

    // Quick Redis check
    let redis_healthy = if let Some(ref pool) = state.redis_pool {
        Some(pool.is_healthy().await)
    } else {
        None
    };

    // Quick PostgreSQL check
    let postgres_healthy = if let Some(ref pool) = state.postgres_pool {
        Some(pool.is_healthy().await)
    } else {
        None
    };

    Ok(Json(HealthResponse {
        status: "healthy",
        version: env!("CARGO_PKG_VERSION"),
        connections,
        users,
        redis: redis_healthy,
        postgres: postgres_healthy,
    }))
}

/// Liveness probe - /health/live
/// Returns 200 if the service is running, regardless of dependencies
pub async fn liveness_probe(
    State(state): State<AppState>,
) -> Json<LivenessResponse> {
    // Just check that we're alive and can respond
    let uptime = state.start_time.elapsed().as_secs();

    Json(LivenessResponse {
        alive: true,
        uptime_seconds: uptime,
    })
}

/// Readiness probe - /health/ready
/// Returns 200 only if the service can accept traffic
pub async fn readiness_probe(
    State(state): State<AppState>,
) -> Result<Json<ReadinessResponse>, (StatusCode, Json<ReadinessResponse>)> {
    let start = Instant::now();

    // Check Redis
    let redis_check = if let Some(ref pool) = state.redis_pool {
        let check_start = Instant::now();
        let healthy = pool.is_healthy().await;
        CheckResult {
            healthy,
            latency_ms: Some(check_start.elapsed().as_millis() as u64),
            error: if healthy { None } else { Some("Redis health check failed".to_string()) },
        }
    } else {
        CheckResult {
            healthy: false,
            latency_ms: None,
            error: Some("Redis not configured".to_string()),
        }
    };

    // Check PostgreSQL
    let postgres_check = if let Some(ref pool) = state.postgres_pool {
        let check_start = Instant::now();
        let healthy = pool.is_healthy().await;
        CheckResult {
            healthy,
            latency_ms: Some(check_start.elapsed().as_millis() as u64),
            error: if healthy { None } else { Some("PostgreSQL health check failed".to_string()) },
        }
    } else {
        CheckResult {
            healthy: false,
            latency_ms: None,
            error: Some("PostgreSQL not configured".to_string()),
        }
    };

    // Check connection capacity
    let max_connections = state.settings.websocket.max_connections;
    let current_connections = state.connection_manager.total_connections();
    let connections_available = current_connections < max_connections;

    let checks = ReadinessChecks {
        redis: redis_check,
        postgres: postgres_check,
        connections_available,
    };

    let ready = checks.redis.healthy && checks.postgres.healthy && checks.connections_available;

    let response = ReadinessResponse { ready, checks };

    if ready {
        Ok(Json(response))
    } else {
        Err((StatusCode::SERVICE_UNAVAILABLE, Json(response)))
    }
}

/// Detailed health report - /health/detailed
pub async fn detailed_health(
    State(state): State<AppState>,
) -> Json<DetailedHealthResponse> {
    let connections = state.connection_manager.total_connections();
    let users = state.connection_manager.unique_users();

    // Redis status
    let redis_status = if let Some(ref pool) = state.redis_pool {
        let start = Instant::now();
        let healthy = pool.is_healthy().await;
        ServiceStatus {
            status: if healthy { "healthy" } else { "unhealthy" },
            latency_ms: Some(start.elapsed().as_millis() as u64),
            pool_size: None,
            pool_idle: None,
        }
    } else {
        ServiceStatus {
            status: "not_configured",
            latency_ms: None,
            pool_size: None,
            pool_idle: None,
        }
    };

    // PostgreSQL status
    let postgres_status = if let Some(ref pool) = state.postgres_pool {
        let pool_ref = pool.pool();
        let start = Instant::now();
        let healthy = pool.is_healthy().await;
        ServiceStatus {
            status: if healthy { "healthy" } else { "unhealthy" },
            latency_ms: Some(start.elapsed().as_millis() as u64),
            pool_size: Some(pool_ref.size()),
            pool_idle: Some(pool_ref.num_idle()),
        }
    } else {
        ServiceStatus {
            status: "not_configured",
            latency_ms: None,
            pool_size: None,
            pool_idle: None,
        }
    };

    // Cluster status
    let cluster_status = if state.settings.cluster.enabled {
        ServiceStatus {
            status: "enabled",
            latency_ms: None,
            pool_size: None,
            pool_idle: None,
        }
    } else {
        ServiceStatus {
            status: "disabled",
            latency_ms: None,
            pool_size: None,
            pool_idle: None,
        }
    };

    // Circuit breaker states
    let circuit_breakers = if let Some(ref cbs) = state.circuit_breakers {
        CircuitBreakerHealth {
            redis: cbs.redis.current_state().await,
            postgres: cbs.postgres.current_state().await,
            cluster: cbs.cluster.current_state().await,
        }
    } else {
        CircuitBreakerHealth {
            redis: "not_configured",
            postgres: "not_configured",
            cluster: "not_configured",
        }
    };

    // Determine overall status
    let status = if redis_status.status == "healthy" && postgres_status.status == "healthy" {
        "healthy"
    } else if redis_status.status == "unhealthy" || postgres_status.status == "unhealthy" {
        "degraded"
    } else {
        "unknown"
    };

    Json(DetailedHealthResponse {
        status,
        version: env!("CARGO_PKG_VERSION"),
        server_id: state.settings.cluster.server_id.clone(),
        uptime_seconds: state.start_time.elapsed().as_secs_f64(),
        connections: ConnectionStats {
            total: connections,
            unique_users: users,
            connections_per_user: if users > 0 { connections as f64 / users as f64 } else { 0.0 },
        },
        services: ServiceHealth {
            redis: redis_status,
            postgres: postgres_status,
            cluster: cluster_status,
        },
        circuit_breakers,
        rate_limiting: RateLimitHealth {
            enabled: true,
            current_usage: 0.0, // TODO: Get actual rate limit usage
        },
        memory: MemoryStats {
            allocated_mb: 0.0, // TODO: Get actual memory usage
        },
    })
}

/// Prometheus metrics endpoint - /metrics
pub async fn prometheus_metrics() -> impl IntoResponse {
    let metrics = metrics::gather_metrics();
    (
        StatusCode::OK,
        [("content-type", "text/plain; charset=utf-8")],
        metrics,
    )
}

/// Shutdown endpoint for graceful shutdown - POST /admin/shutdown
pub async fn shutdown_handler(
    State(_state): State<AppState>,
) -> impl IntoResponse {
    // This would typically be protected by admin authentication
    tracing::warn!("Shutdown requested via admin endpoint");
    (StatusCode::ACCEPTED, "Shutdown initiated")
}
