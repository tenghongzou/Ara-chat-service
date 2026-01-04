//! Attachment API handlers

use axum::{
    body::Body,
    extract::{Multipart, Path, Query, State},
    http::{header, HeaderMap, StatusCode},
    response::IntoResponse,
    Json,
};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::attachment::{AttachmentError, AttachmentResponse, UploadRequest};
use crate::server::AppState;

use super::rest::ErrorResponse;

/// Query parameters for listing attachments
#[derive(Deserialize)]
pub struct ListAttachmentsQuery {
    pub limit: Option<i64>,
    pub offset: Option<i64>,
}

/// Response for attachment list
#[derive(Serialize)]
pub struct AttachmentListResponse {
    pub attachments: Vec<AttachmentResponse>,
    pub total: usize,
}

/// Extract user ID from Authorization header
fn extract_user_id(
    headers: &HeaderMap,
    state: &AppState,
) -> Result<Uuid, (StatusCode, Json<ErrorResponse>)> {
    let auth_header = headers
        .get("authorization")
        .and_then(|h| h.to_str().ok())
        .ok_or_else(|| {
            (
                StatusCode::UNAUTHORIZED,
                Json(ErrorResponse {
                    code: "UNAUTHORIZED".to_string(),
                    message: "Missing Authorization header".to_string(),
                }),
            )
        })?;

    let token = auth_header
        .strip_prefix("Bearer ")
        .or_else(|| auth_header.strip_prefix("bearer "))
        .ok_or_else(|| {
            (
                StatusCode::UNAUTHORIZED,
                Json(ErrorResponse {
                    code: "INVALID_TOKEN".to_string(),
                    message: "Invalid Authorization header format".to_string(),
                }),
            )
        })?;

    let claims = state.jwt_validator.validate(token).map_err(|e| {
        (
            StatusCode::UNAUTHORIZED,
            Json(ErrorResponse {
                code: "INVALID_TOKEN".to_string(),
                message: e.to_string(),
            }),
        )
    })?;

    claims.user_id().map_err(|e| {
        (
            StatusCode::UNAUTHORIZED,
            Json(ErrorResponse {
                code: "INVALID_TOKEN".to_string(),
                message: e.to_string(),
            }),
        )
    })
}

/// Convert AttachmentError to HTTP response
fn attachment_error_response(e: AttachmentError) -> (StatusCode, Json<ErrorResponse>) {
    let status = StatusCode::from_u16(e.status_code()).unwrap_or(StatusCode::INTERNAL_SERVER_ERROR);
    (
        status,
        Json(ErrorResponse {
            code: e.error_code().to_string(),
            message: e.to_string(),
        }),
    )
}

/// Upload a file to a conversation
///
/// POST /api/v1/conversations/{id}/upload
pub async fn upload_file(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(conversation_id): Path<Uuid>,
    mut multipart: Multipart,
) -> Result<Json<AttachmentResponse>, (StatusCode, Json<ErrorResponse>)> {
    let user_id = extract_user_id(&headers, &state)?;

    let attachment_service = state.attachment_service.as_ref().ok_or_else(|| {
        (
            StatusCode::SERVICE_UNAVAILABLE,
            Json(ErrorResponse {
                code: "SERVICE_UNAVAILABLE".to_string(),
                message: "Attachment service not available".to_string(),
            }),
        )
    })?;

    // Parse multipart form
    let mut file_name = None;
    let mut file_data = None;
    let mut mime_type = None;

    while let Some(field) = multipart.next_field().await.map_err(|e| {
        (
            StatusCode::BAD_REQUEST,
            Json(ErrorResponse {
                code: "INVALID_MULTIPART".to_string(),
                message: e.to_string(),
            }),
        )
    })? {
        let name = field.name().map(|s| s.to_string());

        if name.as_deref() == Some("file") {
            file_name = field.file_name().map(|s| s.to_string());
            mime_type = field.content_type().map(|s| s.to_string());

            file_data = Some(field.bytes().await.map_err(|e| {
                (
                    StatusCode::BAD_REQUEST,
                    Json(ErrorResponse {
                        code: "READ_ERROR".to_string(),
                        message: format!("Failed to read file data: {}", e),
                    }),
                )
            })?);
        }
    }

    let file_name = file_name.ok_or_else(|| {
        (
            StatusCode::BAD_REQUEST,
            Json(ErrorResponse {
                code: "MISSING_FILE".to_string(),
                message: "No file provided in multipart form".to_string(),
            }),
        )
    })?;

    let file_data = file_data.ok_or_else(|| {
        (
            StatusCode::BAD_REQUEST,
            Json(ErrorResponse {
                code: "MISSING_FILE".to_string(),
                message: "No file data provided".to_string(),
            }),
        )
    })?;

    // Guess MIME type if not provided
    let mime_type = mime_type.unwrap_or_else(|| {
        mime_guess::from_path(&file_name)
            .first_or_octet_stream()
            .to_string()
    });

    let request = UploadRequest {
        conversation_id,
        file_name,
        data: file_data.to_vec(),
        mime_type,
    };

    match attachment_service.upload(user_id, request).await {
        Ok(attachment) => {
            let response = attachment_service.to_response(attachment);
            Ok(Json(response))
        }
        Err(e) => {
            tracing::error!(user_id = %user_id, error = %e, "Failed to upload file");
            Err(attachment_error_response(e))
        }
    }
}

/// Get attachment info
///
/// GET /api/v1/files/{id}
pub async fn get_attachment(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(attachment_id): Path<Uuid>,
) -> Result<Json<AttachmentResponse>, (StatusCode, Json<ErrorResponse>)> {
    let user_id = extract_user_id(&headers, &state)?;

    let attachment_service = state.attachment_service.as_ref().ok_or_else(|| {
        (
            StatusCode::SERVICE_UNAVAILABLE,
            Json(ErrorResponse {
                code: "SERVICE_UNAVAILABLE".to_string(),
                message: "Attachment service not available".to_string(),
            }),
        )
    })?;

    let attachment = attachment_service
        .get(attachment_id)
        .await
        .map_err(attachment_error_response)?;

    // Verify user is a participant in the conversation
    if let Some(ref conv_service) = state.conversation_service {
        let is_participant = conv_service
            .is_participant(attachment.conversation_id, user_id)
            .await
            .map_err(|e| {
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Json(ErrorResponse {
                        code: "CHECK_FAILED".to_string(),
                        message: e.to_string(),
                    }),
                )
            })?;

        if !is_participant {
            return Err((
                StatusCode::FORBIDDEN,
                Json(ErrorResponse {
                    code: "NOT_PARTICIPANT".to_string(),
                    message: "You are not a participant in this conversation".to_string(),
                }),
            ));
        }
    }

    let response = attachment_service.to_response(attachment);
    Ok(Json(response))
}

/// Download attachment file
///
/// GET /api/v1/files/{id}/download
pub async fn download_attachment(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(attachment_id): Path<Uuid>,
) -> Result<impl IntoResponse, (StatusCode, Json<ErrorResponse>)> {
    let user_id = extract_user_id(&headers, &state)?;

    let attachment_service = state.attachment_service.as_ref().ok_or_else(|| {
        (
            StatusCode::SERVICE_UNAVAILABLE,
            Json(ErrorResponse {
                code: "SERVICE_UNAVAILABLE".to_string(),
                message: "Attachment service not available".to_string(),
            }),
        )
    })?;

    let attachment = attachment_service
        .get(attachment_id)
        .await
        .map_err(attachment_error_response)?;

    // Verify user is a participant
    if let Some(ref conv_service) = state.conversation_service {
        let is_participant = conv_service
            .is_participant(attachment.conversation_id, user_id)
            .await
            .map_err(|e| {
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Json(ErrorResponse {
                        code: "CHECK_FAILED".to_string(),
                        message: e.to_string(),
                    }),
                )
            })?;

        if !is_participant {
            return Err((
                StatusCode::FORBIDDEN,
                Json(ErrorResponse {
                    code: "NOT_PARTICIPANT".to_string(),
                    message: "You are not a participant in this conversation".to_string(),
                }),
            ));
        }
    }

    // If we have a public URL, redirect to it
    if let Some(url) = attachment_service.get_download_url(&attachment) {
        return Ok((
            StatusCode::TEMPORARY_REDIRECT,
            [(header::LOCATION, url)],
            Body::empty(),
        )
            .into_response());
    }

    // Otherwise, return a not implemented error
    Err((
        StatusCode::NOT_IMPLEMENTED,
        Json(ErrorResponse {
            code: "NOT_IMPLEMENTED".to_string(),
            message: "Direct download not available, use the URL from attachment info".to_string(),
        }),
    ))
}

/// Delete an attachment
///
/// DELETE /api/v1/files/{id}
pub async fn delete_attachment(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(attachment_id): Path<Uuid>,
) -> Result<StatusCode, (StatusCode, Json<ErrorResponse>)> {
    let user_id = extract_user_id(&headers, &state)?;

    let attachment_service = state.attachment_service.as_ref().ok_or_else(|| {
        (
            StatusCode::SERVICE_UNAVAILABLE,
            Json(ErrorResponse {
                code: "SERVICE_UNAVAILABLE".to_string(),
                message: "Attachment service not available".to_string(),
            }),
        )
    })?;

    attachment_service
        .delete(attachment_id, user_id)
        .await
        .map_err(attachment_error_response)?;

    Ok(StatusCode::NO_CONTENT)
}

/// List attachments in a conversation
///
/// GET /api/v1/conversations/{id}/attachments
pub async fn list_attachments(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(conversation_id): Path<Uuid>,
    Query(query): Query<ListAttachmentsQuery>,
) -> Result<Json<AttachmentListResponse>, (StatusCode, Json<ErrorResponse>)> {
    let user_id = extract_user_id(&headers, &state)?;

    // Verify user is a participant
    if let Some(ref conv_service) = state.conversation_service {
        let is_participant = conv_service
            .is_participant(conversation_id, user_id)
            .await
            .map_err(|e| {
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Json(ErrorResponse {
                        code: "CHECK_FAILED".to_string(),
                        message: e.to_string(),
                    }),
                )
            })?;

        if !is_participant {
            return Err((
                StatusCode::FORBIDDEN,
                Json(ErrorResponse {
                    code: "NOT_PARTICIPANT".to_string(),
                    message: "You are not a participant in this conversation".to_string(),
                }),
            ));
        }
    }

    let attachment_service = state.attachment_service.as_ref().ok_or_else(|| {
        (
            StatusCode::SERVICE_UNAVAILABLE,
            Json(ErrorResponse {
                code: "SERVICE_UNAVAILABLE".to_string(),
                message: "Attachment service not available".to_string(),
            }),
        )
    })?;

    let limit = query.limit.unwrap_or(50).min(100);
    let offset = query.offset.unwrap_or(0);

    let attachments = attachment_service
        .list_by_conversation(conversation_id, limit, offset)
        .await
        .map_err(attachment_error_response)?;

    let total = attachments.len();
    let responses: Vec<AttachmentResponse> = attachments
        .into_iter()
        .map(|a| attachment_service.to_response(a))
        .collect();

    Ok(Json(AttachmentListResponse {
        attachments: responses,
        total,
    }))
}
