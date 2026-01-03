//! Integration tests for Ara Chat Service
//!
//! These tests verify the HTTP API endpoints work correctly.
//! Run with: cargo test --test integration

use axum::{
    body::Body,
    http::{header, Request, StatusCode},
    Router,
};
use chrono::{Duration, Utc};
use http_body_util::BodyExt;
use jsonwebtoken::{encode, EncodingKey, Header};
use serde::{Deserialize, Serialize};
use tower::ServiceExt;
use uuid::Uuid;

use ara_chat_service::api::create_router;
use ara_chat_service::config::Settings;
use ara_chat_service::server::AppState;

/// Create a test router with minimal dependencies
fn create_test_router() -> Router {
    let settings = Settings::for_testing();
    let state = AppState::for_testing(settings).expect("Failed to create test state");
    create_router(state)
}

/// JWT claims for test tokens
#[derive(Debug, Serialize, Deserialize)]
struct Claims {
    sub: String,
    exp: i64,
    iat: i64,
    iss: String,
    aud: String,
}

/// Create a test JWT token for authentication
fn create_test_token(user_id: &str) -> String {
    let now = Utc::now();
    let claims = Claims {
        sub: user_id.to_string(),
        exp: (now + Duration::hours(1)).timestamp(),
        iat: now.timestamp(),
        iss: "test-issuer".to_string(),
        aud: "test-audience".to_string(),
    };

    let secret = "test-secret-key-that-is-at-least-32-characters-long";
    encode(
        &Header::default(),
        &claims,
        &EncodingKey::from_secret(secret.as_bytes()),
    )
    .expect("Failed to create test token")
}

// ============================================================================
// Health Endpoint Tests
// ============================================================================

#[tokio::test]
async fn test_health_check_returns_ok() {
    let app = create_test_router();

    let response = app
        .oneshot(Request::builder().uri("/health").body(Body::empty()).unwrap())
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);

    let body = response.into_body().collect().await.unwrap().to_bytes();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();

    assert_eq!(json["status"], "healthy");
}

#[tokio::test]
async fn test_liveness_probe_returns_ok() {
    let app = create_test_router();

    let response = app
        .oneshot(
            Request::builder()
                .uri("/health/live")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);

    let body = response.into_body().collect().await.unwrap().to_bytes();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();

    assert_eq!(json["alive"], true);
}

#[tokio::test]
async fn test_readiness_probe_returns_status() {
    let app = create_test_router();

    let response = app
        .oneshot(
            Request::builder()
                .uri("/health/ready")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    // In test mode, redis/postgres are None so it may return unavailable
    let status = response.status();
    assert!(
        status == StatusCode::OK || status == StatusCode::SERVICE_UNAVAILABLE,
        "Expected OK or SERVICE_UNAVAILABLE, got {:?}",
        status
    );
}

#[tokio::test]
async fn test_detailed_health_returns_components() {
    let app = create_test_router();

    let response = app
        .oneshot(
            Request::builder()
                .uri("/health/detailed")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    let body = response.into_body().collect().await.unwrap().to_bytes();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();

    // Should have status or components field
    assert!(json.get("components").is_some() || json.get("status").is_some());
}

#[tokio::test]
async fn test_prometheus_metrics_returns_text() {
    let app = create_test_router();

    let response = app
        .oneshot(
            Request::builder()
                .uri("/metrics")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
}

#[tokio::test]
async fn test_not_found_returns_404() {
    let app = create_test_router();

    let response = app
        .oneshot(
            Request::builder()
                .uri("/nonexistent")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::NOT_FOUND);
}

// ============================================================================
// Authentication Tests
// ============================================================================

#[tokio::test]
async fn test_get_conversations_without_auth_returns_401() {
    let app = create_test_router();

    let response = app
        .oneshot(
            Request::builder()
                .uri("/api/v1/conversations")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn test_get_conversations_with_invalid_token_returns_401() {
    let app = create_test_router();

    let response = app
        .oneshot(
            Request::builder()
                .uri("/api/v1/conversations")
                .header(header::AUTHORIZATION, "Bearer invalid-token")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn test_get_conversations_with_valid_token() {
    let app = create_test_router();
    let user_id = Uuid::new_v4().to_string();
    let token = create_test_token(&user_id);

    let response = app
        .oneshot(
            Request::builder()
                .uri("/api/v1/conversations")
                .header(header::AUTHORIZATION, format!("Bearer {}", token))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    // Should return 503 because PostgreSQL is not available in test mode
    let status = response.status();
    assert!(
        status == StatusCode::OK || status == StatusCode::SERVICE_UNAVAILABLE,
        "Expected OK or SERVICE_UNAVAILABLE, got {:?}",
        status
    );
}

#[tokio::test]
async fn test_expired_token_returns_401() {
    let now = Utc::now();
    let claims = Claims {
        sub: Uuid::new_v4().to_string(),
        exp: (now - Duration::hours(1)).timestamp(), // Expired 1 hour ago
        iat: (now - Duration::hours(2)).timestamp(),
        iss: "test-issuer".to_string(),
        aud: "test-audience".to_string(),
    };

    let secret = "test-secret-key-that-is-at-least-32-characters-long";
    let token = encode(
        &Header::default(),
        &claims,
        &EncodingKey::from_secret(secret.as_bytes()),
    )
    .unwrap();

    let app = create_test_router();

    let response = app
        .oneshot(
            Request::builder()
                .uri("/api/v1/conversations")
                .header(header::AUTHORIZATION, format!("Bearer {}", token))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn test_wrong_issuer_token_returns_401() {
    let now = Utc::now();
    let claims = Claims {
        sub: Uuid::new_v4().to_string(),
        exp: (now + Duration::hours(1)).timestamp(),
        iat: now.timestamp(),
        iss: "wrong-issuer".to_string(), // Wrong issuer
        aud: "test-audience".to_string(),
    };

    let secret = "test-secret-key-that-is-at-least-32-characters-long";
    let token = encode(
        &Header::default(),
        &claims,
        &EncodingKey::from_secret(secret.as_bytes()),
    )
    .unwrap();

    let app = create_test_router();

    let response = app
        .oneshot(
            Request::builder()
                .uri("/api/v1/conversations")
                .header(header::AUTHORIZATION, format!("Bearer {}", token))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
}

// ============================================================================
// REST API Authorization Tests
// ============================================================================

#[tokio::test]
async fn test_get_unread_counts_without_auth_returns_401() {
    let app = create_test_router();

    let response = app
        .oneshot(
            Request::builder()
                .uri("/api/v1/unread")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn test_search_messages_without_auth_returns_401() {
    let app = create_test_router();

    let response = app
        .oneshot(
            Request::builder()
                .uri("/api/v1/search/messages?q=test")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn test_create_conversation_without_auth_returns_401() {
    let app = create_test_router();

    let response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/v1/conversations")
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(r#"{"conversation_type":"direct","participants":[]}"#))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn test_get_conversation_by_id_without_auth_returns_401() {
    let app = create_test_router();
    let conversation_id = Uuid::new_v4();

    let response = app
        .oneshot(
            Request::builder()
                .uri(format!("/api/v1/conversations/{}", conversation_id))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn test_get_messages_without_auth_returns_401() {
    let app = create_test_router();
    let conversation_id = Uuid::new_v4();

    let response = app
        .oneshot(
            Request::builder()
                .uri(format!("/api/v1/conversations/{}/messages", conversation_id))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn test_send_message_without_auth_returns_401() {
    let app = create_test_router();
    let conversation_id = Uuid::new_v4();

    let response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri(format!("/api/v1/conversations/{}/messages", conversation_id))
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(r#"{"content":"test"}"#))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn test_mark_read_without_auth_returns_401() {
    let app = create_test_router();
    let conversation_id = Uuid::new_v4();
    let message_id = Uuid::new_v4();

    let response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri(format!("/api/v1/conversations/{}/read", conversation_id))
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(format!(r#"{{"message_id":"{}"}}"#, message_id)))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
}

// ============================================================================
// Request ID Middleware Tests
// ============================================================================

#[tokio::test]
async fn test_request_id_header_propagation() {
    let app = create_test_router();
    let request_id = "test-request-123";

    let response = app
        .oneshot(
            Request::builder()
                .uri("/health")
                .header("x-request-id", request_id)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    // Response should contain the same request ID
    let response_id = response.headers().get("x-request-id");
    assert!(response_id.is_some());
    assert_eq!(response_id.unwrap().to_str().unwrap(), request_id);
}

#[tokio::test]
async fn test_request_id_generated_when_not_provided() {
    let app = create_test_router();

    let response = app
        .oneshot(Request::builder().uri("/health").body(Body::empty()).unwrap())
        .await
        .unwrap();

    // Response should have a generated request ID
    let response_id = response.headers().get("x-request-id");
    assert!(response_id.is_some());

    // Should be a valid UUID
    let id_str = response_id.unwrap().to_str().unwrap();
    assert!(Uuid::parse_str(id_str).is_ok());
}

// ============================================================================
// WebSocket Protocol Tests
// ============================================================================

#[tokio::test]
async fn test_websocket_upgrade_without_token_returns_error() {
    let app = create_test_router();

    let response = app
        .oneshot(
            Request::builder()
                .uri("/ws")
                .header(header::CONNECTION, "Upgrade")
                .header(header::UPGRADE, "websocket")
                .header("Sec-WebSocket-Version", "13")
                .header("Sec-WebSocket-Key", "dGhlIHNhbXBsZSBub25jZQ==")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    // Without token query param, Query extractor fails with 400 or 422
    // (missing required query parameter)
    let status = response.status();
    assert!(
        status.is_client_error(),
        "Expected client error, got {:?}",
        status
    );
}

#[tokio::test]
async fn test_websocket_upgrade_with_invalid_token() {
    let app = create_test_router();

    let response = app
        .oneshot(
            Request::builder()
                .uri("/ws?token=invalid-token")
                .header(header::CONNECTION, "Upgrade")
                .header(header::UPGRADE, "websocket")
                .header("Sec-WebSocket-Version", "13")
                .header("Sec-WebSocket-Key", "dGhlIHNhbXBsZSBub25jZQ==")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    // WebSocket upgrade with invalid token should either:
    // - Return 101 (upgrade happens, error sent over WS, then closed)
    // - Return 426 (test framework limitation)
    let status = response.status();
    assert!(
        status == StatusCode::SWITCHING_PROTOCOLS || status == StatusCode::UPGRADE_REQUIRED,
        "Expected 101 or 426, got {:?}",
        status
    );
}

#[tokio::test]
async fn test_websocket_upgrade_with_valid_token() {
    let app = create_test_router();
    let user_id = Uuid::new_v4().to_string();
    let token = create_test_token(&user_id);

    let response = app
        .oneshot(
            Request::builder()
                .uri(format!("/ws?token={}", token))
                .header(header::CONNECTION, "Upgrade")
                .header(header::UPGRADE, "websocket")
                .header("Sec-WebSocket-Version", "13")
                .header("Sec-WebSocket-Key", "dGhlIHNhbXBsZSBub25jZQ==")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    // WebSocket upgrade should return 101 Switching Protocols
    // or 426 (test framework limitation with oneshot)
    let status = response.status();
    assert!(
        status == StatusCode::SWITCHING_PROTOCOLS || status == StatusCode::UPGRADE_REQUIRED,
        "Expected 101 or 426, got {:?}",
        status
    );
}

#[tokio::test]
async fn test_websocket_without_upgrade_headers() {
    let app = create_test_router();
    let user_id = Uuid::new_v4().to_string();
    let token = create_test_token(&user_id);

    let response = app
        .oneshot(
            Request::builder()
                .uri(format!("/ws?token={}", token))
                // Missing WebSocket upgrade headers
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    // Should fail because it's not a valid WebSocket upgrade request
    let status = response.status();
    assert!(
        status.is_client_error(),
        "Expected client error, got {:?}",
        status
    );
}
