//! Custom Emoji API handlers

use axum::{
    extract::{Multipart, Path, Query, State},
    http::{HeaderMap, StatusCode},
    Json,
};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::emoji::{
    CustomEmojiResponse, EmojiError, EmojiPackResponse, EmojiSearchResult,
    StandardEmoji,
};
use crate::server::AppState;

use super::rest::ErrorResponse;

/// Query parameters for listing emojis
#[derive(Deserialize)]
pub struct ListEmojisQuery {
    pub pack_id: Option<Uuid>,
    pub limit: Option<i64>,
    pub offset: Option<i64>,
}

/// Query parameters for searching emojis
#[derive(Deserialize)]
pub struct SearchEmojisQuery {
    pub q: String,
    pub limit: Option<i64>,
}

/// Query parameters for listing packs
#[derive(Deserialize)]
pub struct ListPacksQuery {
    pub limit: Option<i64>,
    pub offset: Option<i64>,
}

/// Request body for creating a pack
#[derive(Deserialize)]
pub struct CreatePackBody {
    pub name: String,
    pub description: Option<String>,
}

/// Request body for updating a pack
#[derive(Deserialize)]
pub struct UpdatePackBody {
    pub name: Option<String>,
    pub description: Option<String>,
    pub is_default: Option<bool>,
}

/// Response for emoji list
#[derive(Serialize)]
pub struct EmojiListResponse {
    pub emojis: Vec<CustomEmojiResponse>,
    pub total: usize,
}

/// Response for pack list
#[derive(Serialize)]
pub struct PackListResponse {
    pub packs: Vec<EmojiPackResponse>,
    pub total: usize,
}

/// Extract user ID and tenant ID from Authorization header
fn extract_auth(
    headers: &HeaderMap,
    state: &AppState,
) -> Result<(Uuid, Uuid), (StatusCode, Json<ErrorResponse>)> {
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

    let user_id = claims.user_id().map_err(|e| {
        (
            StatusCode::UNAUTHORIZED,
            Json(ErrorResponse {
                code: "INVALID_TOKEN".to_string(),
                message: e.to_string(),
            }),
        )
    })?;

    // Get tenant_id from claims, parse as UUID or use default for single-tenant mode
    let tenant_id_str = claims.tenant_id();
    let tenant_id = Uuid::parse_str(&tenant_id_str).unwrap_or_else(|_| {
        // Default tenant for single-tenant deployments
        Uuid::nil()
    });

    Ok((user_id, tenant_id))
}

/// Convert EmojiError to HTTP response
fn emoji_error_response(e: EmojiError) -> (StatusCode, Json<ErrorResponse>) {
    let status = StatusCode::from_u16(e.status_code()).unwrap_or(StatusCode::INTERNAL_SERVER_ERROR);
    (
        status,
        Json(ErrorResponse {
            code: e.error_code().to_string(),
            message: e.to_string(),
        }),
    )
}

// ==================== Emoji Endpoints ====================

/// Upload a custom emoji
///
/// POST /api/v1/emojis
pub async fn upload_emoji(
    State(state): State<AppState>,
    headers: HeaderMap,
    mut multipart: Multipart,
) -> Result<Json<CustomEmojiResponse>, (StatusCode, Json<ErrorResponse>)> {
    let (user_id, tenant_id) = extract_auth(&headers, &state)?;

    let emoji_service = state.emoji_service.as_ref().ok_or_else(|| {
        (
            StatusCode::SERVICE_UNAVAILABLE,
            Json(ErrorResponse {
                code: "SERVICE_UNAVAILABLE".to_string(),
                message: "Emoji service not available".to_string(),
            }),
        )
    })?;

    // Parse multipart form
    let mut shortcode = None;
    let mut name = None;
    let mut pack_id = None;
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
        let field_name = field.name().map(|s| s.to_string());

        match field_name.as_deref() {
            Some("shortcode") => {
                shortcode = Some(field.text().await.map_err(|e| {
                    (
                        StatusCode::BAD_REQUEST,
                        Json(ErrorResponse {
                            code: "READ_ERROR".to_string(),
                            message: format!("Failed to read shortcode: {}", e),
                        }),
                    )
                })?);
            }
            Some("name") => {
                name = Some(field.text().await.map_err(|e| {
                    (
                        StatusCode::BAD_REQUEST,
                        Json(ErrorResponse {
                            code: "READ_ERROR".to_string(),
                            message: format!("Failed to read name: {}", e),
                        }),
                    )
                })?);
            }
            Some("pack_id") => {
                let text = field.text().await.map_err(|e| {
                    (
                        StatusCode::BAD_REQUEST,
                        Json(ErrorResponse {
                            code: "READ_ERROR".to_string(),
                            message: format!("Failed to read pack_id: {}", e),
                        }),
                    )
                })?;
                if !text.is_empty() {
                    pack_id = Some(Uuid::parse_str(&text).map_err(|_| {
                        (
                            StatusCode::BAD_REQUEST,
                            Json(ErrorResponse {
                                code: "INVALID_PACK_ID".to_string(),
                                message: "Invalid pack_id format".to_string(),
                            }),
                        )
                    })?);
                }
            }
            Some("file") => {
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
            _ => {}
        }
    }

    let shortcode = shortcode.ok_or_else(|| {
        (
            StatusCode::BAD_REQUEST,
            Json(ErrorResponse {
                code: "MISSING_SHORTCODE".to_string(),
                message: "Shortcode is required".to_string(),
            }),
        )
    })?;

    let name = name.ok_or_else(|| {
        (
            StatusCode::BAD_REQUEST,
            Json(ErrorResponse {
                code: "MISSING_NAME".to_string(),
                message: "Name is required".to_string(),
            }),
        )
    })?;

    let file_data = file_data.ok_or_else(|| {
        (
            StatusCode::BAD_REQUEST,
            Json(ErrorResponse {
                code: "MISSING_FILE".to_string(),
                message: "No file provided in multipart form".to_string(),
            }),
        )
    })?;

    let mime_type = mime_type.unwrap_or_else(|| "image/png".to_string());

    match emoji_service
        .upload_emoji(
            tenant_id,
            user_id,
            &shortcode,
            &name,
            pack_id,
            &file_data,
            &mime_type,
        )
        .await
    {
        Ok(emoji) => Ok(Json(CustomEmojiResponse::from_emoji(emoji))),
        Err(e) => {
            tracing::error!(user_id = %user_id, error = %e, "Failed to upload emoji");
            Err(emoji_error_response(e))
        }
    }
}

/// List custom emojis
///
/// GET /api/v1/emojis
pub async fn list_emojis(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(query): Query<ListEmojisQuery>,
) -> Result<Json<EmojiListResponse>, (StatusCode, Json<ErrorResponse>)> {
    let (_user_id, tenant_id) = extract_auth(&headers, &state)?;

    let emoji_service = state.emoji_service.as_ref().ok_or_else(|| {
        (
            StatusCode::SERVICE_UNAVAILABLE,
            Json(ErrorResponse {
                code: "SERVICE_UNAVAILABLE".to_string(),
                message: "Emoji service not available".to_string(),
            }),
        )
    })?;

    let limit = query.limit.unwrap_or(100).min(500);
    let offset = query.offset.unwrap_or(0);

    let emojis = emoji_service
        .list_emojis(tenant_id, query.pack_id, limit, offset)
        .await
        .map_err(emoji_error_response)?;

    let total = emojis.len();
    let responses: Vec<CustomEmojiResponse> = emojis
        .into_iter()
        .map(CustomEmojiResponse::from_emoji)
        .collect();

    Ok(Json(EmojiListResponse {
        emojis: responses,
        total,
    }))
}

/// Get emoji by ID
///
/// GET /api/v1/emojis/{id}
pub async fn get_emoji(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(emoji_id): Path<Uuid>,
) -> Result<Json<CustomEmojiResponse>, (StatusCode, Json<ErrorResponse>)> {
    let (_user_id, _tenant_id) = extract_auth(&headers, &state)?;

    let emoji_service = state.emoji_service.as_ref().ok_or_else(|| {
        (
            StatusCode::SERVICE_UNAVAILABLE,
            Json(ErrorResponse {
                code: "SERVICE_UNAVAILABLE".to_string(),
                message: "Emoji service not available".to_string(),
            }),
        )
    })?;

    let emoji = emoji_service
        .get_emoji(emoji_id)
        .await
        .map_err(emoji_error_response)?;

    Ok(Json(CustomEmojiResponse::from_emoji(emoji)))
}

/// Delete an emoji
///
/// DELETE /api/v1/emojis/{id}
pub async fn delete_emoji(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(emoji_id): Path<Uuid>,
) -> Result<StatusCode, (StatusCode, Json<ErrorResponse>)> {
    let (user_id, _tenant_id) = extract_auth(&headers, &state)?;

    let emoji_service = state.emoji_service.as_ref().ok_or_else(|| {
        (
            StatusCode::SERVICE_UNAVAILABLE,
            Json(ErrorResponse {
                code: "SERVICE_UNAVAILABLE".to_string(),
                message: "Emoji service not available".to_string(),
            }),
        )
    })?;

    emoji_service
        .delete_emoji(emoji_id, user_id)
        .await
        .map_err(emoji_error_response)?;

    Ok(StatusCode::NO_CONTENT)
}

/// Search emojis
///
/// GET /api/v1/emojis/search
pub async fn search_emojis(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(query): Query<SearchEmojisQuery>,
) -> Result<Json<EmojiSearchResult>, (StatusCode, Json<ErrorResponse>)> {
    let (_user_id, tenant_id) = extract_auth(&headers, &state)?;

    let emoji_service = state.emoji_service.as_ref().ok_or_else(|| {
        (
            StatusCode::SERVICE_UNAVAILABLE,
            Json(ErrorResponse {
                code: "SERVICE_UNAVAILABLE".to_string(),
                message: "Emoji service not available".to_string(),
            }),
        )
    })?;

    let limit = query.limit.unwrap_or(20).min(100);

    // Search custom emojis
    let custom_emojis = emoji_service
        .search_emojis(tenant_id, &query.q, limit)
        .await
        .map_err(emoji_error_response)?;

    // Search standard emojis (basic search in common emojis)
    let standard_emojis = search_standard_emojis(&query.q, (limit - custom_emojis.len() as i64).max(0) as usize);

    Ok(Json(EmojiSearchResult {
        custom: custom_emojis,
        standard: standard_emojis,
    }))
}

// ==================== Pack Endpoints ====================

/// Create an emoji pack
///
/// POST /api/v1/emoji-packs
pub async fn create_pack(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(body): Json<CreatePackBody>,
) -> Result<Json<EmojiPackResponse>, (StatusCode, Json<ErrorResponse>)> {
    let (user_id, tenant_id) = extract_auth(&headers, &state)?;

    let emoji_service = state.emoji_service.as_ref().ok_or_else(|| {
        (
            StatusCode::SERVICE_UNAVAILABLE,
            Json(ErrorResponse {
                code: "SERVICE_UNAVAILABLE".to_string(),
                message: "Emoji service not available".to_string(),
            }),
        )
    })?;

    let pack = emoji_service
        .create_pack(tenant_id, user_id, &body.name, body.description.as_deref())
        .await
        .map_err(emoji_error_response)?;

    Ok(Json(EmojiPackResponse::from_row(pack, 0)))
}

/// List emoji packs
///
/// GET /api/v1/emoji-packs
pub async fn list_packs(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(query): Query<ListPacksQuery>,
) -> Result<Json<PackListResponse>, (StatusCode, Json<ErrorResponse>)> {
    let (_user_id, tenant_id) = extract_auth(&headers, &state)?;

    let emoji_service = state.emoji_service.as_ref().ok_or_else(|| {
        (
            StatusCode::SERVICE_UNAVAILABLE,
            Json(ErrorResponse {
                code: "SERVICE_UNAVAILABLE".to_string(),
                message: "Emoji service not available".to_string(),
            }),
        )
    })?;

    let limit = query.limit.unwrap_or(50).min(100);
    let offset = query.offset.unwrap_or(0);

    let packs = emoji_service
        .list_packs(tenant_id, limit, offset)
        .await
        .map_err(emoji_error_response)?;

    let total = packs.len();
    let responses: Vec<EmojiPackResponse> = packs
        .into_iter()
        .map(|(pack, count)| EmojiPackResponse::from_row(pack, count))
        .collect();

    Ok(Json(PackListResponse {
        packs: responses,
        total,
    }))
}

/// Get pack by ID with emojis
///
/// GET /api/v1/emoji-packs/{id}
pub async fn get_pack(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(pack_id): Path<Uuid>,
) -> Result<Json<PackWithEmojisResponse>, (StatusCode, Json<ErrorResponse>)> {
    let (_user_id, tenant_id) = extract_auth(&headers, &state)?;

    let emoji_service = state.emoji_service.as_ref().ok_or_else(|| {
        (
            StatusCode::SERVICE_UNAVAILABLE,
            Json(ErrorResponse {
                code: "SERVICE_UNAVAILABLE".to_string(),
                message: "Emoji service not available".to_string(),
            }),
        )
    })?;

    let pack = emoji_service
        .get_pack(pack_id)
        .await
        .map_err(emoji_error_response)?;

    let emoji_count = emoji_service
        .get_pack_emoji_count(pack_id)
        .await
        .map_err(emoji_error_response)?;

    let emojis = emoji_service
        .list_emojis(tenant_id, Some(pack_id), 500, 0)
        .await
        .map_err(emoji_error_response)?;

    Ok(Json(PackWithEmojisResponse {
        pack: EmojiPackResponse::from_row(pack, emoji_count),
        emojis: emojis
            .into_iter()
            .map(CustomEmojiResponse::from_emoji)
            .collect(),
    }))
}

/// Update a pack
///
/// PATCH /api/v1/emoji-packs/{id}
pub async fn update_pack(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(pack_id): Path<Uuid>,
    Json(body): Json<UpdatePackBody>,
) -> Result<Json<EmojiPackResponse>, (StatusCode, Json<ErrorResponse>)> {
    let (user_id, _tenant_id) = extract_auth(&headers, &state)?;

    let emoji_service = state.emoji_service.as_ref().ok_or_else(|| {
        (
            StatusCode::SERVICE_UNAVAILABLE,
            Json(ErrorResponse {
                code: "SERVICE_UNAVAILABLE".to_string(),
                message: "Emoji service not available".to_string(),
            }),
        )
    })?;

    let pack = emoji_service
        .update_pack(
            pack_id,
            user_id,
            body.name.as_deref(),
            body.description.as_deref(),
            body.is_default,
        )
        .await
        .map_err(emoji_error_response)?;

    let emoji_count = emoji_service
        .get_pack_emoji_count(pack_id)
        .await
        .map_err(emoji_error_response)?;

    Ok(Json(EmojiPackResponse::from_row(pack, emoji_count)))
}

/// Delete a pack
///
/// DELETE /api/v1/emoji-packs/{id}
pub async fn delete_pack(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(pack_id): Path<Uuid>,
) -> Result<StatusCode, (StatusCode, Json<ErrorResponse>)> {
    let (user_id, _tenant_id) = extract_auth(&headers, &state)?;

    let emoji_service = state.emoji_service.as_ref().ok_or_else(|| {
        (
            StatusCode::SERVICE_UNAVAILABLE,
            Json(ErrorResponse {
                code: "SERVICE_UNAVAILABLE".to_string(),
                message: "Emoji service not available".to_string(),
            }),
        )
    })?;

    emoji_service
        .delete_pack(pack_id, user_id)
        .await
        .map_err(emoji_error_response)?;

    Ok(StatusCode::NO_CONTENT)
}

// ==================== Response Types ====================

/// Response for pack with emojis
#[derive(Serialize)]
pub struct PackWithEmojisResponse {
    pub pack: EmojiPackResponse,
    pub emojis: Vec<CustomEmojiResponse>,
}

// ==================== Helper Functions ====================

/// Search standard Unicode emojis (basic implementation)
fn search_standard_emojis(query: &str, limit: usize) -> Vec<StandardEmoji> {
    let query = query.to_lowercase();

    // Common emoji data (subset for demonstration)
    let emojis = vec![
        ("grinning", "smileys"),
        ("smile", "smileys"),
        ("laughing", "smileys"),
        ("joy", "smileys"),
        ("heart", "symbols"),
        ("fire", "objects"),
        ("thumbsup", "gestures"),
        ("thumbsdown", "gestures"),
        ("clap", "gestures"),
        ("wave", "gestures"),
        ("party", "activities"),
        ("rocket", "travel"),
        ("star", "symbols"),
        ("sun", "nature"),
        ("moon", "nature"),
        ("check", "symbols"),
        ("cross", "symbols"),
        ("question", "symbols"),
        ("exclamation", "symbols"),
        ("100", "symbols"),
    ];

    emojis
        .into_iter()
        .filter(|(name, _)| name.contains(&query))
        .take(limit)
        .map(|(name, category)| StandardEmoji {
            emoji: get_emoji_char(name),
            name: name.to_string(),
            category: category.to_string(),
        })
        .collect()
}

/// Get emoji character by name (basic mapping)
fn get_emoji_char(name: &str) -> String {
    match name {
        "grinning" => "\u{1F600}",
        "smile" => "\u{1F604}",
        "laughing" => "\u{1F606}",
        "joy" => "\u{1F602}",
        "heart" => "\u{2764}\u{FE0F}",
        "fire" => "\u{1F525}",
        "thumbsup" => "\u{1F44D}",
        "thumbsdown" => "\u{1F44E}",
        "clap" => "\u{1F44F}",
        "wave" => "\u{1F44B}",
        "party" => "\u{1F389}",
        "rocket" => "\u{1F680}",
        "star" => "\u{2B50}",
        "sun" => "\u{2600}\u{FE0F}",
        "moon" => "\u{1F319}",
        "check" => "\u{2705}",
        "cross" => "\u{274C}",
        "question" => "\u{2753}",
        "exclamation" => "\u{2757}",
        "100" => "\u{1F4AF}",
        _ => "\u{2753}",
    }
    .to_string()
}
