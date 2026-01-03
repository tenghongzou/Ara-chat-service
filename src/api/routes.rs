//! API routes

use axum::{
    routing::{get, post},
    Router,
};

use crate::server::AppState;
use super::health::{
    health_check,
    liveness_probe,
    readiness_probe,
    detailed_health,
    prometheus_metrics,
};
use super::websocket::websocket_handler;
use super::rest;

/// Create the API router
pub fn create_router(state: AppState) -> Router {
    Router::new()
        // Health check endpoints
        .route("/health", get(health_check))
        .route("/health/live", get(liveness_probe))
        .route("/health/ready", get(readiness_probe))
        .route("/health/detailed", get(detailed_health))
        // Prometheus metrics
        .route("/metrics", get(prometheus_metrics))
        // WebSocket upgrade
        .route("/ws", get(websocket_handler))
        // REST API - Conversations
        .route("/api/v1/conversations", get(rest::get_conversations))
        .route("/api/v1/conversations", post(rest::create_conversation))
        .route("/api/v1/conversations/{id}", get(rest::get_conversation))
        .route("/api/v1/conversations/{id}/messages", get(rest::get_messages))
        .route("/api/v1/conversations/{id}/messages", post(rest::send_message))
        .route("/api/v1/conversations/{id}/read", post(rest::mark_read))
        // REST API - Unread counts
        .route("/api/v1/unread", get(rest::get_unread_counts))
        // REST API - Search
        .route("/api/v1/search/messages", get(rest::search_messages))
        .with_state(state)
}
