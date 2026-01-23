//! Email notification API handlers

use axum::{
    extract::State,
    http::{HeaderMap, StatusCode},
    Json,
};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::api::error::{ApiError, ErrorResponse};
use crate::auth::Claims;
use crate::email::{EmailError, EmailPreferences, UpdateEmailPreferencesRequest};
use crate::server::AppState;

/// Response for email preferences
#[derive(Debug, Serialize)]
pub struct EmailPreferencesResponse {
    pub user_id: Uuid,
    pub email_address: Option<String>,
    pub email_enabled: bool,
    pub notify_messages: bool,
    pub notify_mentions: bool,
    pub digest_mode: String,
    pub quiet_hours_start: Option<i16>,
    pub quiet_hours_end: Option<i16>,
}

impl From<EmailPreferences> for EmailPreferencesResponse {
    fn from(prefs: EmailPreferences) -> Self {
        Self {
            user_id: prefs.user_id,
            email_address: prefs.email_address,
            email_enabled: prefs.email_enabled,
            notify_messages: prefs.notify_messages,
            notify_mentions: prefs.notify_mentions,
            digest_mode: prefs.digest_mode.as_str().to_string(),
            quiet_hours_start: prefs.quiet_hours_start,
            quiet_hours_end: prefs.quiet_hours_end,
        }
    }
}

/// Request to send a test email
#[derive(Debug, Deserialize)]
pub struct TestEmailRequest {
    pub email_address: String,
}

/// Response for test email
#[derive(Debug, Serialize)]
pub struct TestEmailResponse {
    pub success: bool,
    pub message: String,
}

/// Extract JWT claims from Authorization header
fn extract_auth(
    state: &AppState,
    headers: &HeaderMap,
) -> Result<Claims, (StatusCode, Json<ErrorResponse>)> {
    let auth_header = headers
        .get("Authorization")
        .and_then(|h| h.to_str().ok())
        .ok_or_else(|| {
            (
                StatusCode::UNAUTHORIZED,
                Json(ErrorResponse::new("UNAUTHORIZED", "Missing Authorization header")),
            )
        })?;

    let token = auth_header.strip_prefix("Bearer ").ok_or_else(|| {
        (
            StatusCode::UNAUTHORIZED,
            Json(ErrorResponse::new("UNAUTHORIZED", "Invalid Authorization header format")),
        )
    })?;

    state.jwt_validator.validate(token).map_err(|e| {
        (
            StatusCode::UNAUTHORIZED,
            Json(ErrorResponse::new("UNAUTHORIZED", format!("Invalid token: {}", e))),
        )
    })
}

fn email_error_response(e: EmailError) -> (StatusCode, Json<ErrorResponse>) {
    let status = e.status_code();
    (
        status,
        Json(ErrorResponse::new(e.code(), e.to_string())),
    )
}

/// GET /api/v1/email/preferences
/// Get current user's email preferences
pub async fn get_preferences(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Result<Json<EmailPreferencesResponse>, (StatusCode, Json<ErrorResponse>)> {
    let claims = extract_auth(&state, &headers)?;
    let user_id = claims.user_id().map_err(|_| {
        (
            StatusCode::UNAUTHORIZED,
            Json(ErrorResponse::new("INVALID_TOKEN", "Invalid user ID in token")),
        )
    })?;

    let service = state.email_service.as_ref().ok_or_else(|| {
        (
            StatusCode::SERVICE_UNAVAILABLE,
            Json(ErrorResponse::new("SERVICE_UNAVAILABLE", "Email service not available")),
        )
    })?;

    let prefs = service
        .get_preferences(user_id)
        .await
        .map_err(email_error_response)?;

    Ok(Json(prefs.into()))
}

/// PUT /api/v1/email/preferences
/// Update current user's email preferences
pub async fn update_preferences(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(request): Json<UpdateEmailPreferencesRequest>,
) -> Result<Json<EmailPreferencesResponse>, (StatusCode, Json<ErrorResponse>)> {
    let claims = extract_auth(&state, &headers)?;
    let user_id = claims.user_id().map_err(|_| {
        (
            StatusCode::UNAUTHORIZED,
            Json(ErrorResponse::new("INVALID_TOKEN", "Invalid user ID in token")),
        )
    })?;

    let service = state.email_service.as_ref().ok_or_else(|| {
        (
            StatusCode::SERVICE_UNAVAILABLE,
            Json(ErrorResponse::new("SERVICE_UNAVAILABLE", "Email service not available")),
        )
    })?;

    let prefs = service
        .update_preferences(user_id, &request)
        .await
        .map_err(email_error_response)?;

    Ok(Json(prefs.into()))
}

/// POST /api/v1/email/test
/// Send a test email to verify configuration
pub async fn send_test_email(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(request): Json<TestEmailRequest>,
) -> Result<Json<TestEmailResponse>, (StatusCode, Json<ErrorResponse>)> {
    // Verify authentication
    let _claims = extract_auth(&state, &headers)?;

    let service = state.email_service.as_ref().ok_or_else(|| {
        (
            StatusCode::SERVICE_UNAVAILABLE,
            Json(ErrorResponse::new("SERVICE_UNAVAILABLE", "Email service not available")),
        )
    })?;

    service
        .send_test_email(&request.email_address)
        .await
        .map_err(email_error_response)?;

    Ok(Json(TestEmailResponse {
        success: true,
        message: format!("Test email sent to {}", request.email_address),
    }))
}

/// GET /api/v1/email/status
/// Check email service status
pub async fn get_status(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<ErrorResponse>)> {
    // Verify authentication
    let _claims = extract_auth(&state, &headers)?;

    let (enabled, backend) = if let Some(ref service) = state.email_service {
        (service.is_enabled(), Some(service.backend_name()))
    } else {
        (false, None)
    };

    Ok(Json(serde_json::json!({
        "enabled": enabled,
        "backend": backend,
        "configured": state.settings.email.enabled,
    })))
}
