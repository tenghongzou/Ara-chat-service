//! API routes

use axum::{
    middleware,
    routing::{delete, get, post},
    Router,
};

use crate::server::AppState;
use super::attachment;
use super::gdpr;
use super::health::{
    health_check,
    liveness_probe,
    readiness_probe,
    detailed_health,
    prometheus_metrics,
};
use super::middleware::request_id_middleware;
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
        // REST API - Attachments
        .route("/api/v1/conversations/{id}/upload", post(attachment::upload_file))
        .route("/api/v1/conversations/{id}/attachments", get(attachment::list_attachments))
        .route("/api/v1/files/{id}", get(attachment::get_attachment))
        .route("/api/v1/files/{id}/download", get(attachment::download_attachment))
        .route("/api/v1/files/{id}", delete(attachment::delete_attachment))
        // REST API - Unread counts
        .route("/api/v1/unread", get(rest::get_unread_counts))
        // REST API - Search
        .route("/api/v1/search/messages", get(rest::search_messages))
        // REST API - Threads
        .route("/api/v1/messages/{id}/thread", get(rest::get_thread))
        .route("/api/v1/messages/{id}/context", get(rest::get_reply_context))
        // REST API - Message Pinning
        .route("/api/v1/conversations/{id}/messages/{msg_id}/pin", post(rest::pin_message))
        .route("/api/v1/conversations/{id}/messages/{msg_id}/pin", delete(rest::unpin_message))
        .route("/api/v1/conversations/{id}/pinned", get(rest::get_pinned_messages))
        // REST API - Conversation Muting
        .route("/api/v1/conversations/{id}/mute", post(rest::mute_conversation))
        .route("/api/v1/conversations/{id}/mute", delete(rest::unmute_conversation))
        .route("/api/v1/conversations/muted", get(rest::get_muted_conversations))
        // REST API - GDPR Compliance
        .route("/api/v1/gdpr/export", post(gdpr::request_export))
        .route("/api/v1/gdpr/export/{id}", get(gdpr::get_export_status))
        .route("/api/v1/gdpr/data", delete(gdpr::request_deletion))
        .route("/api/v1/gdpr/audit", get(gdpr::get_audit_log))
        // Apply middleware
        .layer(middleware::from_fn(request_id_middleware))
        .with_state(state)
}
