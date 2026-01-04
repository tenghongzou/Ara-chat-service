//! GDPR API Endpoints
//!
//! Provides REST API endpoints for GDPR compliance:
//! - POST /api/v1/gdpr/export - Request data export
//! - GET /api/v1/gdpr/export/{id} - Get export status/download
//! - DELETE /api/v1/gdpr/data - Request data deletion
//! - GET /api/v1/gdpr/audit - Get audit log

use axum::{
    extract::{Path, Query, State},
    http::{HeaderMap, StatusCode},
    Json,
};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::gdpr::{
    AuditLogEntry, DeletionOptions, DeletionResult, ExportResult, GdprError, GdprRequestStatus,
    RequesterType,
};
use crate::server::AppState;

use super::error::ErrorResponse;

// ============================================================================
// Request/Response Types
// ============================================================================

/// Request to export user data
#[derive(Debug, Deserialize)]
pub struct ExportRequest {
    /// Whether to include attachment files in export
    #[serde(default = "default_true")]
    pub include_attachments: bool,
}

fn default_true() -> bool {
    true
}

/// Response for export request
#[derive(Debug, Serialize)]
pub struct ExportResponse {
    pub request_id: Uuid,
    pub status: String,
    pub download_url: Option<String>,
    pub message_count: Option<u64>,
    pub attachment_count: Option<u64>,
    pub export_size_bytes: Option<u64>,
}

impl From<ExportResult> for ExportResponse {
    fn from(result: ExportResult) -> Self {
        let download_url = if result.status == GdprRequestStatus::Completed {
            Some(format!("/api/v1/gdpr/export/{}/download", result.request_id))
        } else {
            None
        };

        Self {
            request_id: result.request_id,
            status: format!("{:?}", result.status).to_lowercase(),
            download_url,
            message_count: Some(result.message_count),
            attachment_count: Some(result.attachment_count),
            export_size_bytes: Some(result.export_size_bytes),
        }
    }
}

/// Request to delete user data
#[derive(Debug, Deserialize)]
pub struct DeletionRequest {
    /// Whether to anonymize messages (true) or hard delete (false)
    #[serde(default)]
    pub anonymize_messages: Option<bool>,
    /// Whether to preserve thread structure
    #[serde(default)]
    pub preserve_thread_structure: Option<bool>,
}

/// Response for deletion request
#[derive(Debug, Serialize)]
pub struct DeletionResponse {
    pub request_id: Uuid,
    pub status: String,
    pub affected: AffectedDataResponse,
}

/// Summary of affected data
#[derive(Debug, Serialize)]
pub struct AffectedDataResponse {
    pub messages_affected: u64,
    pub reactions_deleted: u64,
    pub attachments_deleted: u64,
    pub conversations_left: u64,
    pub read_receipts_deleted: u64,
}

impl From<DeletionResult> for DeletionResponse {
    fn from(result: DeletionResult) -> Self {
        Self {
            request_id: result.request_id,
            status: format!("{:?}", result.status).to_lowercase(),
            affected: AffectedDataResponse {
                messages_affected: result.affected.messages_anonymized
                    + result.affected.messages_deleted,
                reactions_deleted: result.affected.reactions_deleted,
                attachments_deleted: result.affected.attachments_deleted,
                conversations_left: result.affected.conversations_left,
                read_receipts_deleted: result.affected.read_receipts_deleted,
            },
        }
    }
}

/// Query parameters for audit log
#[derive(Debug, Deserialize)]
pub struct AuditLogQuery {
    #[serde(default = "default_limit")]
    pub limit: i64,
}

fn default_limit() -> i64 {
    20
}

/// Response for audit log
#[derive(Debug, Serialize)]
pub struct AuditLogResponse {
    pub entries: Vec<AuditLogEntry>,
    pub count: usize,
}

/// Response for request status
#[derive(Debug, Serialize)]
pub struct RequestStatusResponse {
    pub request_id: Uuid,
    pub status: String,
    pub details: Option<AuditLogEntry>,
}

// ============================================================================
// Helper Functions
// ============================================================================

/// Extract user ID from JWT token in Authorization header
fn extract_user_id(headers: &HeaderMap, state: &AppState) -> Result<Uuid, (StatusCode, Json<ErrorResponse>)> {
    let auth_header = headers
        .get("authorization")
        .and_then(|v| v.to_str().ok())
        .ok_or_else(|| {
            (
                StatusCode::UNAUTHORIZED,
                Json(ErrorResponse::new("UNAUTHORIZED", "Missing Authorization header")),
            )
        })?;

    let token = auth_header
        .strip_prefix("Bearer ")
        .or_else(|| auth_header.strip_prefix("bearer "))
        .ok_or_else(|| {
            (
                StatusCode::UNAUTHORIZED,
                Json(ErrorResponse::new("UNAUTHORIZED", "Invalid Authorization header format")),
            )
        })?;

    let claims = state.jwt_validator.validate(token).map_err(|_| {
        (
            StatusCode::UNAUTHORIZED,
            Json(ErrorResponse::new("INVALID_TOKEN", "Invalid or expired token")),
        )
    })?;

    Uuid::parse_str(&claims.sub).map_err(|_| {
        (
            StatusCode::UNAUTHORIZED,
            Json(ErrorResponse::new("INVALID_TOKEN", "Invalid user ID in token")),
        )
    })
}

/// Extract client IP from headers
fn extract_client_ip(headers: &HeaderMap) -> Option<String> {
    headers
        .get("x-forwarded-for")
        .or_else(|| headers.get("x-real-ip"))
        .and_then(|v| v.to_str().ok())
        .and_then(|s| s.split(',').next())
        .map(|s| s.trim().to_string())
}

/// Extract user agent from headers
fn extract_user_agent(headers: &HeaderMap) -> Option<String> {
    headers
        .get("user-agent")
        .and_then(|v| v.to_str().ok())
        .map(String::from)
}

/// Convert GDPR error to HTTP response
fn gdpr_error_to_response(err: GdprError) -> (StatusCode, Json<ErrorResponse>) {
    let (status, code, message) = match &err {
        GdprError::AlreadyProcessing(_) => (
            StatusCode::CONFLICT,
            "ALREADY_PROCESSING",
            err.to_string(),
        ),
        GdprError::RateLimited { .. } => (
            StatusCode::TOO_MANY_REQUESTS,
            "RATE_LIMITED",
            err.to_string(),
        ),
        GdprError::Unauthorized(_) => (
            StatusCode::UNAUTHORIZED,
            "UNAUTHORIZED",
            err.to_string(),
        ),
        GdprError::Forbidden(_) => (StatusCode::FORBIDDEN, "FORBIDDEN", err.to_string()),
        GdprError::UserNotFound(_) => (StatusCode::NOT_FOUND, "USER_NOT_FOUND", err.to_string()),
        GdprError::RequestNotFound(_) => {
            (StatusCode::NOT_FOUND, "REQUEST_NOT_FOUND", err.to_string())
        }
        GdprError::ServiceUnavailable(_) => (
            StatusCode::SERVICE_UNAVAILABLE,
            "SERVICE_UNAVAILABLE",
            err.to_string(),
        ),
        _ => (
            StatusCode::INTERNAL_SERVER_ERROR,
            "INTERNAL_ERROR",
            "An internal error occurred".to_string(),
        ),
    };

    (status, Json(ErrorResponse::new(code, message)))
}

// ============================================================================
// Handlers
// ============================================================================

/// POST /api/v1/gdpr/export
///
/// Request an export of the authenticated user's data.
/// Returns immediately with a request ID; export is processed synchronously.
pub async fn request_export(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(req): Json<ExportRequest>,
) -> Result<Json<ExportResponse>, (StatusCode, Json<ErrorResponse>)> {
    let user_id = extract_user_id(&headers, &state)?;

    let gdpr_service = state.gdpr_service.as_ref().ok_or_else(|| {
        (
            StatusCode::SERVICE_UNAVAILABLE,
            Json(ErrorResponse::new("SERVICE_UNAVAILABLE", "GDPR service is not available")),
        )
    })?;

    let request_ip = extract_client_ip(&headers);
    let user_agent = extract_user_agent(&headers);

    match gdpr_service
        .request_export(
            user_id,
            Some(user_id),
            RequesterType::User,
            req.include_attachments,
            request_ip,
            user_agent,
        )
        .await
    {
        Ok(result) => {
            tracing::info!(
                user_id = %user_id,
                request_id = %result.request_id,
                "GDPR export completed"
            );
            Ok(Json(result.into()))
        }
        Err(e) => {
            tracing::error!(
                user_id = %user_id,
                error = %e,
                "GDPR export failed"
            );
            Err(gdpr_error_to_response(e))
        }
    }
}

/// GET /api/v1/gdpr/export/{request_id}
///
/// Get the status of an export request.
pub async fn get_export_status(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(request_id): Path<Uuid>,
) -> Result<Json<RequestStatusResponse>, (StatusCode, Json<ErrorResponse>)> {
    let user_id = extract_user_id(&headers, &state)?;

    let gdpr_service = state.gdpr_service.as_ref().ok_or_else(|| {
        (
            StatusCode::SERVICE_UNAVAILABLE,
            Json(ErrorResponse::new("SERVICE_UNAVAILABLE", "GDPR service is not available")),
        )
    })?;

    match gdpr_service.get_request_details(request_id).await {
        Ok(Some(entry)) => {
            // Verify the request belongs to this user
            if entry.subject_user_id != user_id {
                return Err((
                    StatusCode::FORBIDDEN,
                    Json(ErrorResponse::new("FORBIDDEN", "You do not have access to this request")),
                ));
            }

            Ok(Json(RequestStatusResponse {
                request_id,
                status: entry.status.clone(),
                details: Some(entry),
            }))
        }
        Ok(None) => Err((
            StatusCode::NOT_FOUND,
            Json(ErrorResponse::new("NOT_FOUND", "Export request not found")),
        )),
        Err(e) => Err(gdpr_error_to_response(e)),
    }
}

/// DELETE /api/v1/gdpr/data
///
/// Request deletion of the authenticated user's data.
/// This operation is irreversible.
pub async fn request_deletion(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(req): Json<DeletionRequest>,
) -> Result<Json<DeletionResponse>, (StatusCode, Json<ErrorResponse>)> {
    let user_id = extract_user_id(&headers, &state)?;

    let gdpr_service = state.gdpr_service.as_ref().ok_or_else(|| {
        (
            StatusCode::SERVICE_UNAVAILABLE,
            Json(ErrorResponse::new("SERVICE_UNAVAILABLE", "GDPR service is not available")),
        )
    })?;

    let options = DeletionOptions {
        anonymize_messages: req.anonymize_messages.unwrap_or(true),
        preserve_thread_structure: req.preserve_thread_structure.unwrap_or(true),
        ..Default::default()
    };

    let request_ip = extract_client_ip(&headers);
    let user_agent = extract_user_agent(&headers);

    match gdpr_service
        .request_deletion(
            user_id,
            Some(user_id),
            RequesterType::User,
            options,
            request_ip,
            user_agent,
        )
        .await
    {
        Ok(result) => {
            tracing::info!(
                user_id = %user_id,
                request_id = %result.request_id,
                messages_affected = result.affected.messages_anonymized + result.affected.messages_deleted,
                "GDPR deletion completed"
            );
            Ok(Json(result.into()))
        }
        Err(e) => {
            tracing::error!(
                user_id = %user_id,
                error = %e,
                "GDPR deletion failed"
            );
            Err(gdpr_error_to_response(e))
        }
    }
}

/// GET /api/v1/gdpr/audit
///
/// Get the GDPR audit log for the authenticated user.
pub async fn get_audit_log(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(query): Query<AuditLogQuery>,
) -> Result<Json<AuditLogResponse>, (StatusCode, Json<ErrorResponse>)> {
    let user_id = extract_user_id(&headers, &state)?;

    let gdpr_service = state.gdpr_service.as_ref().ok_or_else(|| {
        (
            StatusCode::SERVICE_UNAVAILABLE,
            Json(ErrorResponse::new("SERVICE_UNAVAILABLE", "GDPR service is not available")),
        )
    })?;

    let limit = query.limit.min(100).max(1);

    match gdpr_service.get_audit_log(user_id, limit).await {
        Ok(entries) => {
            let count = entries.len();
            Ok(Json(AuditLogResponse { entries, count }))
        }
        Err(e) => Err(gdpr_error_to_response(e)),
    }
}
