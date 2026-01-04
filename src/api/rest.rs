//! REST API handlers

use axum::{
    extract::{Path, Query, State},
    http::{HeaderMap, StatusCode},
    Json,
};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::domain::validation::{limits, sanitize_message_content, sanitize_conversation_name};
use crate::message::{ChatMessage, ConversationSummary, ContentType, ConversationType};
use crate::server::AppState;

#[derive(Deserialize)]
pub struct PaginationQuery {
    pub before: Option<String>, // Uuid as string, or timestamp
    pub limit: Option<u32>,
}

#[derive(Deserialize)]
pub struct MessageHistoryQuery {
    pub before: Option<Uuid>,
    pub limit: Option<u32>,
}

#[derive(Serialize)]
pub struct ConversationsResponse {
    pub conversations: Vec<ConversationSummary>,
    pub has_more: bool,
}

#[derive(Serialize)]
pub struct MessagesResponse {
    pub messages: Vec<ChatMessage>,
    pub has_more: bool,
}

#[derive(Serialize)]
pub struct ErrorResponse {
    pub code: String,
    pub message: String,
}

#[derive(Deserialize)]
pub struct CreateConversationRequest {
    pub conversation_type: ConversationType,
    pub participants: Vec<Uuid>,
    pub name: Option<String>,
}

#[derive(Deserialize)]
pub struct SendMessageRequest {
    pub content: String,
    #[serde(default)]
    pub content_type: ContentType,
    pub reply_to: Option<Uuid>,
    #[serde(default)]
    pub mentions: Vec<Uuid>,
    pub client_message_id: Option<String>,
}

#[derive(Serialize)]
pub struct SendMessageResponse {
    pub id: Uuid,
    pub conversation_id: Uuid,
    pub created_at: i64,
    pub client_message_id: Option<String>,
}

#[derive(Deserialize)]
pub struct MarkReadRequest {
    pub message_id: Uuid,
}

/// Extract user ID from Authorization header (Bearer token)
fn extract_user_id(headers: &HeaderMap, state: &AppState) -> Result<Uuid, (StatusCode, Json<ErrorResponse>)> {
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

/// Get user's conversations
pub async fn get_conversations(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(pagination): Query<PaginationQuery>,
) -> Result<Json<ConversationsResponse>, (StatusCode, Json<ErrorResponse>)> {
    let user_id = extract_user_id(&headers, &state)?;

    let conv_service = state.conversation_service.as_ref().ok_or_else(|| {
        (
            StatusCode::SERVICE_UNAVAILABLE,
            Json(ErrorResponse {
                code: "SERVICE_UNAVAILABLE".to_string(),
                message: "Conversation service not available".to_string(),
            }),
        )
    })?;

    let before = pagination.before.and_then(|s| s.parse().ok());
    let limit = pagination.limit.unwrap_or(20).min(50);

    match conv_service.get_user_conversations(user_id, before, limit).await {
        Ok((mut conversations, has_more)) => {
            // Update unread counts from Redis
            for conv in &mut conversations {
                if let Some(ref tracker) = state.receipt_tracker {
                    conv.unread_count = tracker.get_unread_count(user_id, conv.id).await.unwrap_or(0);
                }
            }

            Ok(Json(ConversationsResponse {
                conversations,
                has_more,
            }))
        }
        Err(e) => {
            tracing::error!(user_id = %user_id, error = %e, "Failed to fetch conversations");
            Err((
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ErrorResponse {
                    code: "FETCH_FAILED".to_string(),
                    message: e.to_string(),
                }),
            ))
        }
    }
}

/// Create a new conversation
pub async fn create_conversation(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(req): Json<CreateConversationRequest>,
) -> Result<Json<ConversationSummary>, (StatusCode, Json<ErrorResponse>)> {
    let user_id = extract_user_id(&headers, &state)?;

    // Validate name length
    if let Some(ref name) = req.name {
        if name.len() > limits::MAX_CONVERSATION_NAME_LENGTH {
            return Err((
                StatusCode::BAD_REQUEST,
                Json(ErrorResponse {
                    code: "NAME_TOO_LONG".to_string(),
                    message: format!("Conversation name exceeds {} characters", limits::MAX_CONVERSATION_NAME_LENGTH),
                }),
            ));
        }
    }

    // Validate participant count
    if req.participants.len() > limits::MAX_PARTICIPANTS_PER_CONVERSATION {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(ErrorResponse {
                code: "TOO_MANY_PARTICIPANTS".to_string(),
                message: format!("Max {} participants allowed", limits::MAX_PARTICIPANTS_PER_CONVERSATION),
            }),
        ));
    }

    // Sanitize conversation name
    let name = req.name.map(|n| sanitize_conversation_name(&n));

    let conv_service = state.conversation_service.as_ref().ok_or_else(|| {
        (
            StatusCode::SERVICE_UNAVAILABLE,
            Json(ErrorResponse {
                code: "SERVICE_UNAVAILABLE".to_string(),
                message: "Conversation service not available".to_string(),
            }),
        )
    })?;

    // Ensure creator is in participants
    let mut all_participants = req.participants;
    if !all_participants.contains(&user_id) {
        all_participants.push(user_id);
    }

    match conv_service
        .create_conversation(req.conversation_type.clone(), user_id, all_participants, name)
        .await
    {
        Ok(conversation) => {
            let updated_at = conversation
                .last_message_at
                .map(|dt| dt.timestamp_millis())
                .unwrap_or_else(|| conversation.created_at.timestamp_millis());

            let summary = ConversationSummary {
                id: conversation.id,
                conversation_type: req.conversation_type,
                name: conversation.name,
                avatar_url: conversation.avatar_url,
                participant_count: conversation.participant_count,
                participants: vec![],
                last_message: None,
                unread_count: 0,
                updated_at,
            };

            Ok(Json(summary))
        }
        Err(e) => {
            tracing::error!(user_id = %user_id, error = %e, "Failed to create conversation");
            Err((
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ErrorResponse {
                    code: "CREATE_FAILED".to_string(),
                    message: e.to_string(),
                }),
            ))
        }
    }
}

/// Get messages in a conversation
pub async fn get_messages(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(conversation_id): Path<Uuid>,
    Query(pagination): Query<MessageHistoryQuery>,
) -> Result<Json<MessagesResponse>, (StatusCode, Json<ErrorResponse>)> {
    let user_id = extract_user_id(&headers, &state)?;

    // Verify user is a participant
    if let Some(ref conv_service) = state.conversation_service {
        match conv_service.is_participant(conversation_id, user_id).await {
            Ok(false) => {
                return Err((
                    StatusCode::FORBIDDEN,
                    Json(ErrorResponse {
                        code: "NOT_PARTICIPANT".to_string(),
                        message: "You are not a participant in this conversation".to_string(),
                    }),
                ));
            }
            Err(e) => {
                return Err((
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Json(ErrorResponse {
                        code: "CHECK_FAILED".to_string(),
                        message: e.to_string(),
                    }),
                ));
            }
            _ => {}
        }
    }

    let storage = state.message_storage.as_ref().ok_or_else(|| {
        (
            StatusCode::SERVICE_UNAVAILABLE,
            Json(ErrorResponse {
                code: "SERVICE_UNAVAILABLE".to_string(),
                message: "Message service not available".to_string(),
            }),
        )
    })?;

    let limit = pagination.limit.unwrap_or(50).min(100);

    match storage.get_history(conversation_id, pagination.before, limit).await {
        Ok((messages, has_more)) => Ok(Json(MessagesResponse { messages, has_more })),
        Err(e) => {
            tracing::error!(user_id = %user_id, error = %e, "Failed to fetch messages");
            Err((
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ErrorResponse {
                    code: "FETCH_FAILED".to_string(),
                    message: e.to_string(),
                }),
            ))
        }
    }
}

/// Send a message to a conversation
pub async fn send_message(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(conversation_id): Path<Uuid>,
    Json(req): Json<SendMessageRequest>,
) -> Result<Json<SendMessageResponse>, (StatusCode, Json<ErrorResponse>)> {
    let user_id = extract_user_id(&headers, &state)?;

    // Validate content length
    if req.content.is_empty() {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(ErrorResponse {
                code: "EMPTY_CONTENT".to_string(),
                message: "Message content cannot be empty".to_string(),
            }),
        ));
    }
    if req.content.len() > limits::MAX_MESSAGE_LENGTH {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(ErrorResponse {
                code: "CONTENT_TOO_LONG".to_string(),
                message: format!("Message exceeds {} characters", limits::MAX_MESSAGE_LENGTH),
            }),
        ));
    }

    // Validate mentions count
    if req.mentions.len() > limits::MAX_MENTIONS_PER_MESSAGE {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(ErrorResponse {
                code: "TOO_MANY_MENTIONS".to_string(),
                message: format!("Max {} mentions allowed", limits::MAX_MENTIONS_PER_MESSAGE),
            }),
        ));
    }

    // Sanitize content for XSS prevention
    let content = sanitize_message_content(&req.content);

    // Check rate limit
    match state.rate_limiter.check_user_rate(user_id).await {
        Ok(result) => {
            if !result.allowed {
                let retry_after = result.retry_after.map(|d| d.as_secs()).unwrap_or(60);
                return Err((
                    StatusCode::TOO_MANY_REQUESTS,
                    Json(ErrorResponse {
                        code: "RATE_LIMITED".to_string(),
                        message: format!("Rate limit exceeded. Retry after {} seconds", retry_after),
                    }),
                ));
            }
        }
        Err(e) => {
            tracing::warn!(user_id = %user_id, error = %e, "Rate limit check failed");
        }
    }

    let handler = state.message_handler.as_ref().ok_or_else(|| {
        (
            StatusCode::SERVICE_UNAVAILABLE,
            Json(ErrorResponse {
                code: "SERVICE_UNAVAILABLE".to_string(),
                message: "Message service not available".to_string(),
            }),
        )
    })?;

    match handler
        .handle_send_message(
            user_id,
            conversation_id,
            content,
            req.content_type,
            req.reply_to,
            req.client_message_id.clone(),
            req.mentions,
        )
        .await
    {
        Ok(message) => {
            // Increment unread counts for other participants
            if let Some(ref conv_service) = state.conversation_service {
                if let Ok(participants) = conv_service.get_participant_ids(conversation_id).await {
                    if let Some(ref receipt_tracker) = state.receipt_tracker {
                        for participant_id in participants {
                            if participant_id != user_id {
                                let _ = receipt_tracker
                                    .increment_unread(participant_id, conversation_id)
                                    .await;
                            }
                        }
                    }
                }
            }

            Ok(Json(SendMessageResponse {
                id: message.id,
                conversation_id,
                created_at: message.created_at,
                client_message_id: req.client_message_id,
            }))
        }
        Err(e) => {
            tracing::error!(user_id = %user_id, error = %e, "Failed to send message");
            Err((
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ErrorResponse {
                    code: "SEND_FAILED".to_string(),
                    message: e.to_string(),
                }),
            ))
        }
    }
}

/// Mark messages as read
pub async fn mark_read(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(conversation_id): Path<Uuid>,
    Json(req): Json<MarkReadRequest>,
) -> Result<StatusCode, (StatusCode, Json<ErrorResponse>)> {
    let user_id = extract_user_id(&headers, &state)?;

    let tracker = state.receipt_tracker.as_ref().ok_or_else(|| {
        (
            StatusCode::SERVICE_UNAVAILABLE,
            Json(ErrorResponse {
                code: "SERVICE_UNAVAILABLE".to_string(),
                message: "Receipt service not available".to_string(),
            }),
        )
    })?;

    match tracker.mark_read(conversation_id, user_id, req.message_id).await {
        Ok(_) => Ok(StatusCode::OK),
        Err(e) => {
            tracing::error!(user_id = %user_id, error = %e, "Failed to mark as read");
            Err((
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ErrorResponse {
                    code: "MARK_READ_FAILED".to_string(),
                    message: e.to_string(),
                }),
            ))
        }
    }
}

/// Get conversation details
pub async fn get_conversation(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(conversation_id): Path<Uuid>,
) -> Result<Json<ConversationSummary>, (StatusCode, Json<ErrorResponse>)> {
    let user_id = extract_user_id(&headers, &state)?;

    let conv_service = state.conversation_service.as_ref().ok_or_else(|| {
        (
            StatusCode::SERVICE_UNAVAILABLE,
            Json(ErrorResponse {
                code: "SERVICE_UNAVAILABLE".to_string(),
                message: "Conversation service not available".to_string(),
            }),
        )
    })?;

    // Verify user is a participant
    match conv_service.is_participant(conversation_id, user_id).await {
        Ok(false) => {
            return Err((
                StatusCode::FORBIDDEN,
                Json(ErrorResponse {
                    code: "NOT_PARTICIPANT".to_string(),
                    message: "You are not a participant in this conversation".to_string(),
                }),
            ));
        }
        Err(e) => {
            return Err((
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ErrorResponse {
                    code: "CHECK_FAILED".to_string(),
                    message: e.to_string(),
                }),
            ));
        }
        _ => {}
    }

    match conv_service.get_conversation_summary(conversation_id, user_id).await {
        Ok(Some(mut summary)) => {
            // Update unread count from Redis
            if let Some(ref tracker) = state.receipt_tracker {
                summary.unread_count = tracker.get_unread_count(user_id, conversation_id).await.unwrap_or(0);
            }
            Ok(Json(summary))
        }
        Ok(None) => Err((
            StatusCode::NOT_FOUND,
            Json(ErrorResponse {
                code: "NOT_FOUND".to_string(),
                message: "Conversation not found".to_string(),
            }),
        )),
        Err(e) => {
            tracing::error!(user_id = %user_id, error = %e, "Failed to fetch conversation");
            Err((
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ErrorResponse {
                    code: "FETCH_FAILED".to_string(),
                    message: e.to_string(),
                }),
            ))
        }
    }
}

/// Get unread counts for all conversations
pub async fn get_unread_counts(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Result<Json<crate::message::UnreadSyncData>, (StatusCode, Json<ErrorResponse>)> {
    let user_id = extract_user_id(&headers, &state)?;

    if let Some(ref cache) = state.redis_cache {
        match cache.get_unread_sync(user_id).await {
            Ok((total, per_conversation)) => Ok(Json(crate::message::UnreadSyncData {
                total,
                per_conversation,
            })),
            Err(e) => {
                tracing::error!(user_id = %user_id, error = %e, "Failed to get unread counts");
                Err((
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Json(ErrorResponse {
                        code: "FETCH_FAILED".to_string(),
                        message: e.to_string(),
                    }),
                ))
            }
        }
    } else {
        Ok(Json(crate::message::UnreadSyncData {
            total: 0,
            per_conversation: std::collections::HashMap::new(),
        }))
    }
}

// --- Search API ---

#[derive(Deserialize)]
pub struct SearchQuery {
    pub q: String,
    pub conversation_id: Option<Uuid>,
    pub limit: Option<u32>,
}

#[derive(Serialize)]
pub struct SearchResult {
    pub messages: Vec<MessageSearchResult>,
    pub total_count: u64,
}

#[derive(Serialize)]
pub struct MessageSearchResult {
    pub id: Uuid,
    pub conversation_id: Uuid,
    pub sender_id: Uuid,
    pub content_preview: String,
    pub created_at: i64,
    pub highlight: Option<String>,
}

/// Search messages across user's conversations
pub async fn search_messages(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(query): Query<SearchQuery>,
) -> Result<Json<SearchResult>, (StatusCode, Json<ErrorResponse>)> {
    let user_id = extract_user_id(&headers, &state)?;

    // Validate query
    let search_term = query.q.trim();
    if search_term.is_empty() || search_term.len() < 2 {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(ErrorResponse {
                code: "INVALID_QUERY".to_string(),
                message: "Search query must be at least 2 characters".to_string(),
            }),
        ));
    }

    let storage = state.message_storage.as_ref().ok_or_else(|| {
        (
            StatusCode::SERVICE_UNAVAILABLE,
            Json(ErrorResponse {
                code: "SERVICE_UNAVAILABLE".to_string(),
                message: "Message service not available".to_string(),
            }),
        )
    })?;

    let limit = query.limit.unwrap_or(20).min(50);

    match storage
        .search_messages(user_id, search_term, query.conversation_id, limit)
        .await
    {
        Ok((results, total_count)) => {
            let messages: Vec<MessageSearchResult> = results
                .into_iter()
                .map(|r| MessageSearchResult {
                    id: r.id,
                    conversation_id: r.conversation_id,
                    sender_id: r.sender_id,
                    content_preview: if r.content.len() > 150 {
                        format!("{}...", &r.content[..147])
                    } else {
                        r.content.clone()
                    },
                    created_at: r.created_at,
                    highlight: r.highlight,
                })
                .collect();

            Ok(Json(SearchResult {
                messages,
                total_count,
            }))
        }
        Err(e) => {
            tracing::error!(user_id = %user_id, error = %e, "Failed to search messages");
            Err((
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ErrorResponse {
                    code: "SEARCH_FAILED".to_string(),
                    message: e.to_string(),
                }),
            ))
        }
    }
}

// --- Thread API ---

#[derive(Deserialize)]
pub struct ThreadQuery {
    pub limit: Option<u32>,
}

#[derive(Serialize)]
pub struct ThreadResponse {
    pub root_message: ChatMessage,
    pub replies: Vec<ChatMessage>,
    pub has_more: bool,
    pub total_replies: i32,
}

/// Get thread replies for a message
pub async fn get_thread(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(message_id): Path<Uuid>,
    Query(query): Query<ThreadQuery>,
) -> Result<Json<ThreadResponse>, (StatusCode, Json<ErrorResponse>)> {
    let user_id = extract_user_id(&headers, &state)?;

    let storage = state.message_storage.as_ref().ok_or_else(|| {
        (
            StatusCode::SERVICE_UNAVAILABLE,
            Json(ErrorResponse {
                code: "SERVICE_UNAVAILABLE".to_string(),
                message: "Message service not available".to_string(),
            }),
        )
    })?;

    // Get the root message first
    let root_message = match storage.get_message(message_id).await {
        Ok(Some(msg)) => msg,
        Ok(None) => {
            return Err((
                StatusCode::NOT_FOUND,
                Json(ErrorResponse {
                    code: "MESSAGE_NOT_FOUND".to_string(),
                    message: "Message not found".to_string(),
                }),
            ));
        }
        Err(e) => {
            return Err((
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ErrorResponse {
                    code: "FETCH_FAILED".to_string(),
                    message: e.to_string(),
                }),
            ));
        }
    };

    // Verify user is a participant in the conversation
    if let Some(ref conv_service) = state.conversation_service {
        match conv_service.is_participant(root_message.conversation_id, user_id).await {
            Ok(false) => {
                return Err((
                    StatusCode::FORBIDDEN,
                    Json(ErrorResponse {
                        code: "NOT_PARTICIPANT".to_string(),
                        message: "You are not a participant in this conversation".to_string(),
                    }),
                ));
            }
            Err(e) => {
                return Err((
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Json(ErrorResponse {
                        code: "CHECK_FAILED".to_string(),
                        message: e.to_string(),
                    }),
                ));
            }
            _ => {}
        }
    }

    let limit = query.limit.unwrap_or(50).min(100);

    // Get reply count
    let total_replies = storage.get_reply_count(message_id).await.unwrap_or(0);

    // Get thread replies
    match storage.get_thread_replies(message_id, None, limit).await {
        Ok((replies, has_more)) => Ok(Json(ThreadResponse {
            root_message,
            replies,
            has_more,
            total_replies,
        })),
        Err(e) => {
            tracing::error!(user_id = %user_id, message_id = %message_id, error = %e, "Failed to fetch thread");
            Err((
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ErrorResponse {
                    code: "FETCH_FAILED".to_string(),
                    message: e.to_string(),
                }),
            ))
        }
    }
}

#[derive(Serialize)]
pub struct ReplyContextResponse {
    pub id: Uuid,
    pub sender_id: Uuid,
    pub content_preview: String,
    pub content_type: ContentType,
}

/// Get reply context for a message (preview of original message)
pub async fn get_reply_context(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(message_id): Path<Uuid>,
) -> Result<Json<ReplyContextResponse>, (StatusCode, Json<ErrorResponse>)> {
    let user_id = extract_user_id(&headers, &state)?;

    let storage = state.message_storage.as_ref().ok_or_else(|| {
        (
            StatusCode::SERVICE_UNAVAILABLE,
            Json(ErrorResponse {
                code: "SERVICE_UNAVAILABLE".to_string(),
                message: "Message service not available".to_string(),
            }),
        )
    })?;

    // Get the message first to check conversation membership
    let message = match storage.get_message(message_id).await {
        Ok(Some(msg)) => msg,
        Ok(None) => {
            return Err((
                StatusCode::NOT_FOUND,
                Json(ErrorResponse {
                    code: "MESSAGE_NOT_FOUND".to_string(),
                    message: "Message not found".to_string(),
                }),
            ));
        }
        Err(e) => {
            return Err((
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ErrorResponse {
                    code: "FETCH_FAILED".to_string(),
                    message: e.to_string(),
                }),
            ));
        }
    };

    // Verify user is a participant
    if let Some(ref conv_service) = state.conversation_service {
        match conv_service.is_participant(message.conversation_id, user_id).await {
            Ok(false) => {
                return Err((
                    StatusCode::FORBIDDEN,
                    Json(ErrorResponse {
                        code: "NOT_PARTICIPANT".to_string(),
                        message: "You are not a participant in this conversation".to_string(),
                    }),
                ));
            }
            Err(e) => {
                return Err((
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Json(ErrorResponse {
                        code: "CHECK_FAILED".to_string(),
                        message: e.to_string(),
                    }),
                ));
            }
            _ => {}
        }
    }

    // Get reply context (using the message itself as the context)
    match storage.get_reply_context(message_id).await {
        Ok(Some(context)) => Ok(Json(ReplyContextResponse {
            id: context.id,
            sender_id: context.sender_id,
            content_preview: context.content_preview,
            content_type: context.content_type,
        })),
        Ok(None) => Err((
            StatusCode::NOT_FOUND,
            Json(ErrorResponse {
                code: "MESSAGE_NOT_FOUND".to_string(),
                message: "Message not found or deleted".to_string(),
            }),
        )),
        Err(e) => {
            tracing::error!(user_id = %user_id, message_id = %message_id, error = %e, "Failed to fetch reply context");
            Err((
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ErrorResponse {
                    code: "FETCH_FAILED".to_string(),
                    message: e.to_string(),
                }),
            ))
        }
    }
}
