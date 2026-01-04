//! WebSocket handler

use std::sync::Arc;

use axum::{
    extract::{
        ws::{Message, WebSocket, WebSocketUpgrade},
        Query, State,
    },
    response::IntoResponse,
};
use futures::{SinkExt, StreamExt};
use serde::Deserialize;
use tokio::sync::mpsc;
use uuid::Uuid;

use crate::auth::Claims;
use crate::compression::{CompressionCodec, create_codec};
use crate::connection::Connection;
use crate::domain::validation::{limits, sanitize_message_content, sanitize_conversation_name};
use crate::message::{ClientMessage, OutboundMessage, ReactionAction, ServerMessage};
use crate::notification::NotificationPayload;
use crate::server::AppState;

#[derive(Deserialize)]
pub struct WsQuery {
    token: String,
}

pub async fn websocket_handler(
    ws: WebSocketUpgrade,
    Query(query): Query<WsQuery>,
    State(state): State<AppState>,
) -> impl IntoResponse {
    // Validate JWT token
    let claims = match state.jwt_validator.validate(&query.token) {
        Ok(claims) => claims,
        Err(e) => {
            tracing::warn!(error = %e, "WebSocket auth failed");
            return ws.on_upgrade(|mut socket| async move {
                let error = ServerMessage::error("AUTH_FAILED", "Invalid token");
                let _ = socket.send(Message::Text(serde_json::to_string(&error).unwrap().into())).await;
                let _ = socket.close().await;
            });
        }
    };

    ws.on_upgrade(move |socket| handle_socket(socket, claims, state))
}

async fn handle_socket(socket: WebSocket, claims: Claims, state: AppState) {
    let user_id = match claims.user_id() {
        Ok(id) => id,
        Err(e) => {
            tracing::warn!(error = %e, "Invalid user ID in claims");
            return;
        }
    };

    let tenant_id = claims.tenant_id();
    let connection_id = Uuid::new_v4();

    let (mut ws_sender, mut ws_receiver) = socket.split();

    // Create channel for outbound messages
    let (tx, mut rx) = mpsc::unbounded_channel::<OutboundMessage>();

    // Create and register connection
    let connection = Connection::new(connection_id, user_id, tenant_id.clone(), tx);
    if let Err(e) = state.connection_manager.register(connection) {
        tracing::warn!(
            user_id = %user_id,
            error = %e,
            "Failed to register connection"
        );
        let error = ServerMessage::error("CONNECTION_LIMIT", e.to_string());
        let _ = ws_sender.send(Message::Text(serde_json::to_string(&error).unwrap().into())).await;
        return;
    }

    tracing::info!(
        connection_id = %connection_id,
        user_id = %user_id,
        "WebSocket connected"
    );

    // Send authentication confirmation
    let auth_msg = ServerMessage::authenticated(user_id);
    let _ = ws_sender.send(Message::Text(serde_json::to_string(&auth_msg).unwrap().into())).await;

    // Register session in cluster
    if let Some(ref session_store) = state.session_store {
        let _ = session_store.register_session(user_id).await;
    }

    // Update presence
    if let Some(ref presence) = state.presence_tracker {
        let _ = presence.mark_online(user_id).await;
    }

    // Deliver queued offline messages
    deliver_offline_messages(user_id, &state).await;

    // Clone what we need for cleanup (before moving state)
    let connection_manager = state.connection_manager.clone();
    let session_store_cleanup = state.session_store.clone();
    let presence_tracker_cleanup = state.presence_tracker.clone();
    let presence_broadcaster_cleanup = state.presence_broadcaster.clone();

    // Create compression codec from settings
    let compression_codec: Option<Arc<CompressionCodec>> = if state.settings.compression.enabled {
        Some(create_codec(
            &state.settings.compression.algorithm,
            state.settings.compression.level,
            state.settings.compression.threshold,
            state.settings.compression.max_decompressed_size,
        ))
    } else {
        None
    };

    // Track if client supports compression (set after receiving Capabilities message)
    let client_supports_compression = Arc::new(std::sync::atomic::AtomicBool::new(false));
    let client_compression_flag = client_supports_compression.clone();
    let codec_for_send = compression_codec.clone();

    // Spawn task to forward outbound messages to WebSocket
    let send_task = tokio::spawn(async move {
        while let Some(msg) = rx.recv().await {
            match msg.to_json() {
                Ok(json) => {
                    // Use compression if both server and client support it
                    let ws_msg = if client_compression_flag.load(std::sync::atomic::Ordering::Relaxed) {
                        if let Some(ref codec) = codec_for_send {
                            match codec.compress_json(&json) {
                                Ok(compressed) => {
                                    // Track compression metrics
                                    let original_len = json.len();
                                    let compressed_len = compressed.len();
                                    if compressed_len < original_len {
                                        crate::metrics::record_compression(original_len, compressed_len);
                                    }
                                    Message::Binary(compressed.into())
                                }
                                Err(e) => {
                                    tracing::warn!(error = %e, "Compression failed, sending uncompressed");
                                    Message::Text(json.into())
                                }
                            }
                        } else {
                            Message::Text(json.into())
                        }
                    } else {
                        Message::Text(json.into())
                    };

                    if ws_sender.send(ws_msg).await.is_err() {
                        break;
                    }
                }
                Err(e) => {
                    tracing::error!(error = %e, "Failed to serialize message");
                }
            }
        }
    });

    // Handle incoming messages
    let codec_for_recv = compression_codec.clone();
    let recv_task = tokio::spawn(async move {
        while let Some(result) = ws_receiver.next().await {
            match result {
                Ok(Message::Text(text)) => {
                    match serde_json::from_str::<ClientMessage>(&text) {
                        Ok(msg) => {
                            tracing::debug!(user_id = %user_id, msg_type = ?std::mem::discriminant(&msg), "Received client message");
                            // Handle Capabilities message specially to enable compression
                            if let ClientMessage::Capabilities { compression, .. } = &msg {
                                if state.settings.compression.enabled && compression.contains(&"zstd".to_string()) {
                                    client_supports_compression.store(true, std::sync::atomic::Ordering::Relaxed);
                                    let ack = ServerMessage::capabilities_ack(
                                        Some("zstd".to_string()),
                                        state.settings.compression.threshold,
                                    );
                                    state.connection_manager.send_to_user(&user_id, ack.into()).await;
                                    tracing::debug!(user_id = %user_id, "Compression enabled for client");
                                } else {
                                    let ack = ServerMessage::capabilities_ack(None, 0);
                                    state.connection_manager.send_to_user(&user_id, ack.into()).await;
                                }
                            } else {
                                handle_client_message(user_id, msg, &state).await;
                            }
                        }
                        Err(e) => {
                            tracing::warn!(user_id = %user_id, error = %e, raw = %text, "Failed to parse client message");
                            send_error(user_id, "INVALID_MESSAGE", format!("Failed to parse message: {}", e), &state).await;
                        }
                    }
                }
                Ok(Message::Binary(data)) => {
                    // Handle compressed binary messages
                    if let Some(ref codec) = codec_for_recv {
                        match codec.decompress_to_string(&data) {
                            Ok(text) => {
                                match serde_json::from_str::<ClientMessage>(&text) {
                                    Ok(msg) => {
                                        tracing::debug!(user_id = %user_id, msg_type = ?std::mem::discriminant(&msg), "Received compressed client message");
                                        handle_client_message(user_id, msg, &state).await;
                                    }
                                    Err(e) => {
                                        tracing::warn!(user_id = %user_id, error = %e, "Failed to parse decompressed message");
                                        send_error(user_id, "INVALID_MESSAGE", format!("Failed to parse message: {}", e), &state).await;
                                    }
                                }
                            }
                            Err(e) => {
                                tracing::warn!(user_id = %user_id, error = %e, "Failed to decompress binary message");
                                send_error(user_id, "DECOMPRESSION_FAILED", format!("Failed to decompress: {}", e), &state).await;
                            }
                        }
                    } else {
                        tracing::warn!(user_id = %user_id, "Received binary message but compression not enabled");
                        send_error(user_id, "BINARY_NOT_SUPPORTED", "Binary messages not supported".to_string(), &state).await;
                    }
                }
                Ok(Message::Close(_)) => break,
                Err(e) => {
                    tracing::debug!(error = %e, "WebSocket error");
                    break;
                }
                _ => {}
            }
        }
    });

    // Wait for either task to complete
    tokio::select! {
        _ = send_task => {},
        _ = recv_task => {},
    }

    // Cleanup
    connection_manager.unregister(connection_id);

    // Unregister session from cluster
    if let Some(ref session_store) = session_store_cleanup {
        let _ = session_store.unregister_session(user_id).await;
    }

    // Update presence and notify subscribers
    if let Some(ref presence) = presence_tracker_cleanup {
        let is_fully_offline = presence.mark_offline(user_id).await.unwrap_or(true);

        if is_fully_offline {
            // Clear all subscriptions and notify subscribers
            let _ = presence.clear_subscriptions(user_id).await;

            // Broadcast offline status to subscribers
            if let Some(ref broadcaster) = presence_broadcaster_cleanup {
                let _ = broadcaster.broadcast_to_subscribers(
                    user_id,
                    crate::message::PresenceStatus::Offline,
                ).await;
            }
        }
    }

    tracing::info!(
        connection_id = %connection_id,
        user_id = %user_id,
        "WebSocket disconnected"
    );
}

async fn handle_client_message(user_id: Uuid, msg: ClientMessage, state: &AppState) {
    match msg {
        ClientMessage::Ping => {
            let pong = ServerMessage::Pong;
            state.connection_manager.send_to_user(&user_id, pong.into()).await;
        }

        ClientMessage::SendMessage {
            conversation_id,
            content,
            content_type,
            reply_to,
            client_message_id,
            mentions,
        } => {
            handle_send_message(
                user_id,
                conversation_id,
                content,
                content_type,
                reply_to,
                client_message_id,
                mentions,
                state,
            ).await;
        }

        ClientMessage::MarkRead {
            conversation_id,
            message_id,
        } => {
            handle_mark_read(user_id, conversation_id, message_id, state).await;
        }

        ClientMessage::Typing {
            conversation_id,
            is_typing,
        } => {
            handle_typing(user_id, conversation_id, is_typing, state).await;
        }

        ClientMessage::FetchHistory {
            conversation_id,
            before,
            limit,
        } => {
            handle_fetch_history(user_id, conversation_id, before, limit, state).await;
        }

        ClientMessage::FetchConversations { before, limit } => {
            handle_fetch_conversations(user_id, before, limit, state).await;
        }

        ClientMessage::RecallMessage {
            conversation_id,
            message_id,
        } => {
            handle_recall_message(user_id, conversation_id, message_id, state).await;
        }

        ClientMessage::EditMessage {
            conversation_id: _,
            message_id,
            new_content,
        } => {
            handle_edit_message(user_id, message_id, new_content, state).await;
        }

        ClientMessage::ToggleReaction {
            conversation_id,
            message_id,
            emoji,
        } => {
            handle_toggle_reaction(user_id, conversation_id, message_id, emoji, state).await;
        }

        ClientMessage::CreateConversation {
            conversation_type,
            participants,
            name,
        } => {
            handle_create_conversation(user_id, conversation_type, participants, name, state).await;
        }

        ClientMessage::UpdatePresence { status } => {
            handle_update_presence(user_id, status, state).await;
        }

        ClientMessage::SubscribePresence { user_ids } => {
            handle_subscribe_presence(user_id, user_ids, state).await;
        }

        ClientMessage::UnsubscribePresence { user_ids } => {
            handle_unsubscribe_presence(user_id, user_ids, state).await;
        }

        ClientMessage::SyncUnread => {
            handle_sync_unread(user_id, state).await;
        }

        ClientMessage::GetReactions { message_ids } => {
            handle_get_reactions(user_id, message_ids, state).await;
        }

        ClientMessage::PinMessage {
            conversation_id,
            message_id,
        } => {
            handle_pin_message(user_id, conversation_id, message_id, state).await;
        }

        ClientMessage::UnpinMessage {
            conversation_id,
            message_id,
        } => {
            handle_unpin_message(user_id, conversation_id, message_id, state).await;
        }

        ClientMessage::MuteConversation { conversation_id } => {
            handle_mute_conversation(user_id, conversation_id, state).await;
        }

        ClientMessage::UnmuteConversation { conversation_id } => {
            handle_unmute_conversation(user_id, conversation_id, state).await;
        }

        ClientMessage::BlockUser { user_id: target_user_id } => {
            handle_block_user(user_id, target_user_id, state).await;
        }

        ClientMessage::UnblockUser { user_id: target_user_id } => {
            handle_unblock_user(user_id, target_user_id, state).await;
        }

        ClientMessage::GetBlockedUsers => {
            handle_get_blocked_users(user_id, state).await;
        }

        ClientMessage::ForwardMessage {
            message_id,
            source_conversation_id,
            target_conversation_ids,
        } => {
            handle_forward_message(user_id, message_id, source_conversation_id, target_conversation_ids, state).await;
        }

        ClientMessage::Authenticate { .. } => {
            // Already authenticated via query param
            tracing::debug!(user_id = %user_id, "Re-auth attempted on authenticated connection");
        }
    }
}

async fn handle_send_message(
    user_id: Uuid,
    conversation_id: Uuid,
    content: String,
    content_type: crate::message::ContentType,
    reply_to: Option<Uuid>,
    client_message_id: Option<String>,
    mentions: Vec<Uuid>,
    state: &AppState,
) {
    tracing::info!(
        user_id = %user_id,
        conversation_id = %conversation_id,
        content_len = content.len(),
        "Processing SendMessage"
    );

    // Validate content length
    if content.is_empty() {
        send_error(user_id, "EMPTY_CONTENT", "Message content cannot be empty".to_string(), state).await;
        return;
    }
    if content.len() > limits::MAX_MESSAGE_LENGTH {
        send_error(
            user_id,
            "CONTENT_TOO_LONG",
            format!("Message exceeds {} characters", limits::MAX_MESSAGE_LENGTH),
            state,
        ).await;
        return;
    }

    // Validate mentions count
    if mentions.len() > limits::MAX_MENTIONS_PER_MESSAGE {
        send_error(
            user_id,
            "TOO_MANY_MENTIONS",
            format!("Max {} mentions allowed", limits::MAX_MENTIONS_PER_MESSAGE),
            state,
        ).await;
        return;
    }

    // Sanitize content for XSS prevention
    let content = sanitize_message_content(&content);

    // Check rate limit before processing
    match state.rate_limiter.check_user_rate(user_id).await {
        Ok(result) => {
            if !result.allowed {
                let retry_after = result.retry_after.map(|d| d.as_secs()).unwrap_or(60);
                send_error(
                    user_id,
                    "RATE_LIMITED",
                    format!("Rate limit exceeded. Retry after {} seconds", retry_after),
                    state,
                ).await;
                return;
            }
        }
        Err(e) => {
            tracing::warn!(user_id = %user_id, error = %e, "Rate limit check failed, allowing message");
            // Continue on error - fail open
        }
    }

    let handler = match &state.message_handler {
        Some(h) => h,
        None => {
            send_error(user_id, "SERVICE_UNAVAILABLE", "Message service not available".to_string(), state).await;
            return;
        }
    };

    match handler
        .handle_send_message(
            user_id,
            conversation_id,
            content,
            content_type,
            reply_to,
            client_message_id.clone(),
            mentions,
        )
        .await
    {
        Ok(message) => {
            // Send confirmation to sender
            let confirmation = ServerMessage::MessageSent {
                conversation_id,
                message_id: message.id,
                client_message_id,
                created_at: message.created_at,
            };
            state.connection_manager.send_to_user(&user_id, confirmation.into()).await;

            // Increment unread counts for other participants
            if let Some(ref conv_service) = state.conversation_service {
                if let Ok(participants) = conv_service.get_participant_ids(conversation_id).await {
                    if let Some(ref receipt_tracker) = state.receipt_tracker {
                        for participant_id in participants {
                            if participant_id != user_id {
                                let _ = receipt_tracker.increment_unread(participant_id, conversation_id).await;
                            }
                        }
                    }
                }
            }

            // Enqueue link previews for background processing
            if let Some(ref link_preview_service) = state.link_preview_service {
                if let Err(e) = link_preview_service.enqueue_previews(message.id, &message.content).await {
                    tracing::warn!(
                        message_id = %message.id,
                        error = %e,
                        "Failed to enqueue link previews"
                    );
                }
            }
        }
        Err(e) => {
            tracing::error!(user_id = %user_id, error = %e, "Failed to send message");
            send_error(user_id, "SEND_FAILED", e.to_string(), state).await;
        }
    }
}

async fn handle_mark_read(
    user_id: Uuid,
    conversation_id: Uuid,
    message_id: Uuid,
    state: &AppState,
) {
    let tracker = match &state.receipt_tracker {
        Some(t) => t,
        None => return,
    };

    match tracker.mark_read(conversation_id, user_id, message_id).await {
        Ok(result) => {
            // Broadcast read receipt to other participants (across cluster)
            if let Some(ref conv_service) = state.conversation_service {
                if let Ok(participants) = conv_service.get_participant_ids(conversation_id).await {
                    let receipt = ServerMessage::ReadReceipt {
                        conversation_id,
                        user_id,
                        message_id,
                        read_at: result.read_at,
                    };

                    // Pre-serialize for efficient multi-send
                    let outbound = match OutboundMessage::preserialized(&receipt) {
                        Ok(o) => o,
                        Err(e) => {
                            tracing::error!(error = %e, "Failed to serialize read receipt");
                            return;
                        }
                    };

                    for participant_id in participants {
                        if participant_id != user_id {
                            // Try local delivery first
                            if state.connection_manager.has_user(&participant_id) {
                                state.connection_manager.send_to_user(&participant_id, outbound.clone()).await;
                            } else if let Some(ref cluster_router) = state.cluster_router {
                                // Route through cluster (with offline queue for important messages)
                                let _ = cluster_router.route_to_user_with_queue(
                                    participant_id,
                                    outbound.clone(),
                                    receipt.clone(),
                                ).await;
                            }
                        }
                    }
                }
            }

            tracing::debug!(
                user_id = %user_id,
                conversation_id = %conversation_id,
                message_id = %message_id,
                "Messages marked as read"
            );
        }
        Err(e) => {
            tracing::warn!(user_id = %user_id, error = %e, "Failed to mark as read");
        }
    }
}

async fn handle_typing(
    user_id: Uuid,
    conversation_id: Uuid,
    is_typing: bool,
    state: &AppState,
) {
    // Update typing status in Redis cache
    if let Some(ref cache) = state.redis_cache {
        let _ = cache.set_typing(conversation_id, user_id, is_typing).await;
    }

    // Broadcast to other participants (across cluster, but no offline queue - typing is ephemeral)
    if let Some(ref conv_service) = state.conversation_service {
        if let Ok(participants) = conv_service.get_participant_ids(conversation_id).await {
            let typing_msg = ServerMessage::Typing {
                conversation_id,
                user_id,
                is_typing,
            };

            // Pre-serialize for efficient multi-send
            let outbound = match OutboundMessage::preserialized(&typing_msg) {
                Ok(o) => o,
                Err(_) => return,
            };

            for participant_id in participants {
                if participant_id != user_id {
                    // Try local delivery first
                    if state.connection_manager.has_user(&participant_id) {
                        state.connection_manager.send_to_user(&participant_id, outbound.clone()).await;
                    } else if let Some(ref cluster_router) = state.cluster_router {
                        // Route through cluster without offline queue (typing is ephemeral)
                        let _ = cluster_router.route_to_user(participant_id, outbound.clone()).await;
                    }
                }
            }
        }
    }
}

async fn handle_fetch_history(
    user_id: Uuid,
    conversation_id: Uuid,
    before: Option<Uuid>,
    limit: Option<u32>,
    state: &AppState,
) {
    // Verify user is a participant
    if let Some(ref conv_service) = state.conversation_service {
        match conv_service.is_participant(conversation_id, user_id).await {
            Ok(false) => {
                send_error(user_id, "NOT_PARTICIPANT", "You are not a participant".to_string(), state).await;
                return;
            }
            Err(e) => {
                send_error(user_id, "FETCH_FAILED", e.to_string(), state).await;
                return;
            }
            _ => {}
        }
    }

    let storage = match &state.message_storage {
        Some(s) => s,
        None => {
            send_error(user_id, "SERVICE_UNAVAILABLE", "Message service not available".to_string(), state).await;
            return;
        }
    };

    let limit = limit.unwrap_or(50).min(100);

    match storage.get_history(conversation_id, before, limit).await {
        Ok((messages, has_more)) => {
            let history = ServerMessage::History {
                conversation_id,
                messages,
                has_more,
            };
            state.connection_manager.send_to_user(&user_id, history.into()).await;
        }
        Err(e) => {
            tracing::error!(user_id = %user_id, error = %e, "Failed to fetch history");
            send_error(user_id, "FETCH_FAILED", e.to_string(), state).await;
        }
    }
}

async fn handle_fetch_conversations(
    user_id: Uuid,
    before: Option<i64>,
    limit: Option<u32>,
    state: &AppState,
) {
    let conv_service = match &state.conversation_service {
        Some(s) => s,
        None => {
            send_error(user_id, "SERVICE_UNAVAILABLE", "Conversation service not available".to_string(), state).await;
            return;
        }
    };

    let limit = limit.unwrap_or(20).min(50);

    match conv_service.get_user_conversations(user_id, before, limit).await {
        Ok((mut conversations, has_more)) => {
            // Update unread counts from Redis
            for conv in &mut conversations {
                if let Some(ref tracker) = state.receipt_tracker {
                    conv.unread_count = tracker.get_unread_count(user_id, conv.id).await.unwrap_or(0);
                }
            }

            let response = ServerMessage::Conversations {
                conversations,
                has_more,
            };
            state.connection_manager.send_to_user(&user_id, response.into()).await;
        }
        Err(e) => {
            tracing::error!(user_id = %user_id, error = %e, "Failed to fetch conversations");
            send_error(user_id, "FETCH_FAILED", e.to_string(), state).await;
        }
    }
}

async fn handle_recall_message(
    user_id: Uuid,
    conversation_id: Uuid,
    message_id: Uuid,
    state: &AppState,
) {
    let handler = match &state.message_handler {
        Some(h) => h,
        None => {
            send_error(user_id, "SERVICE_UNAVAILABLE", "Message service not available".to_string(), state).await;
            return;
        }
    };

    match handler.handle_recall_message(user_id, conversation_id, message_id).await {
        Ok(()) => {
            tracing::info!(
                user_id = %user_id,
                message_id = %message_id,
                "Message recalled"
            );
        }
        Err(e) => {
            tracing::warn!(user_id = %user_id, error = %e, "Failed to recall message");
            send_error(user_id, "RECALL_FAILED", e.to_string(), state).await;
        }
    }
}

async fn handle_edit_message(
    user_id: Uuid,
    message_id: Uuid,
    new_content: String,
    state: &AppState,
) {
    // Validate content length
    if new_content.is_empty() {
        send_error(user_id, "EMPTY_CONTENT", "Message content cannot be empty".to_string(), state).await;
        return;
    }
    if new_content.len() > limits::MAX_MESSAGE_LENGTH {
        send_error(
            user_id,
            "CONTENT_TOO_LONG",
            format!("Message exceeds {} characters", limits::MAX_MESSAGE_LENGTH),
            state,
        ).await;
        return;
    }

    // Sanitize content for XSS prevention
    let new_content = sanitize_message_content(&new_content);

    let handler = match &state.message_handler {
        Some(h) => h,
        None => {
            send_error(user_id, "SERVICE_UNAVAILABLE", "Message service not available".to_string(), state).await;
            return;
        }
    };

    match handler.handle_edit_message(user_id, message_id, new_content).await {
        Ok(message) => {
            // Send confirmation to sender
            let confirmation = ServerMessage::MessageEdited {
                conversation_id: message.conversation_id,
                message_id: message.id,
                new_content: message.content,
                edited_at: message.updated_at.unwrap_or(message.created_at),
                mentions: message.mentions,
            };
            state.connection_manager.send_to_user(&user_id, confirmation.into()).await;

            tracing::info!(
                user_id = %user_id,
                message_id = %message_id,
                "Message edited"
            );
        }
        Err(e) => {
            tracing::warn!(user_id = %user_id, error = %e, "Failed to edit message");
            send_error(user_id, "EDIT_FAILED", e.to_string(), state).await;
        }
    }
}

async fn handle_toggle_reaction(
    user_id: Uuid,
    conversation_id: Uuid,
    message_id: Uuid,
    emoji: String,
    state: &AppState,
) {
    // Validate emoji
    if emoji.is_empty() || emoji.len() > limits::MAX_EMOJI_LENGTH {
        send_error(
            user_id,
            "INVALID_EMOJI",
            format!("Emoji must be 1-{} characters", limits::MAX_EMOJI_LENGTH),
            state,
        ).await;
        return;
    }

    let reaction_service = match &state.reaction_service {
        Some(s) => s,
        None => {
            send_error(user_id, "SERVICE_UNAVAILABLE", "Reaction service not available".to_string(), state).await;
            return;
        }
    };

    match reaction_service.toggle_reaction(message_id, user_id, &emoji).await {
        Ok(action) => {
            // Broadcast reaction update to conversation participants (across cluster)
            if let Some(ref conv_service) = state.conversation_service {
                if let Ok(participants) = conv_service.get_participant_ids(conversation_id).await {
                    let update = ServerMessage::ReactionUpdate {
                        conversation_id,
                        message_id,
                        user_id,
                        emoji: emoji.clone(),
                        action,
                    };

                    // Pre-serialize for efficient multi-send
                    let outbound = match OutboundMessage::preserialized(&update) {
                        Ok(o) => o,
                        Err(e) => {
                            tracing::error!(error = %e, "Failed to serialize reaction update");
                            return;
                        }
                    };

                    for participant_id in participants {
                        // Try local delivery first
                        if state.connection_manager.has_user(&participant_id) {
                            state.connection_manager.send_to_user(&participant_id, outbound.clone()).await;
                        } else if let Some(ref cluster_router) = state.cluster_router {
                            // Route through cluster with offline queue
                            let _ = cluster_router.route_to_user_with_queue(
                                participant_id,
                                outbound.clone(),
                                update.clone(),
                            ).await;
                        }
                    }
                }
            }

            // Send push notification for reaction adds
            if action == ReactionAction::Add {
                if let Some(ref publisher) = state.notification_publisher {
                    // Get message author from storage
                    if let Some(ref storage) = state.message_storage {
                        if let Ok(Some(message)) = storage.get_message(message_id).await {
                            // Only notify if the message author is not the reactor
                            if message.sender_id != user_id {
                                let payload = NotificationPayload::reaction(
                                    conversation_id,
                                    message_id,
                                    user_id,
                                    emoji,
                                    "add",
                                );
                                publisher.notify_reaction(message.sender_id, payload).await;
                            }
                        }
                    }
                }
            }
        }
        Err(e) => {
            tracing::warn!(user_id = %user_id, error = %e, "Failed to toggle reaction");
            send_error(user_id, "REACTION_FAILED", e.to_string(), state).await;
        }
    }
}

async fn handle_create_conversation(
    user_id: Uuid,
    conversation_type: crate::message::ConversationType,
    participants: Vec<Uuid>,
    name: Option<String>,
    state: &AppState,
) {
    // Validate name length
    if let Some(ref n) = name {
        if n.len() > limits::MAX_CONVERSATION_NAME_LENGTH {
            send_error(
                user_id,
                "NAME_TOO_LONG",
                format!("Conversation name exceeds {} characters", limits::MAX_CONVERSATION_NAME_LENGTH),
                state,
            ).await;
            return;
        }
    }

    // Validate participant count
    if participants.len() > limits::MAX_PARTICIPANTS_PER_CONVERSATION {
        send_error(
            user_id,
            "TOO_MANY_PARTICIPANTS",
            format!("Max {} participants allowed", limits::MAX_PARTICIPANTS_PER_CONVERSATION),
            state,
        ).await;
        return;
    }

    // Sanitize conversation name
    let name = name.map(|n| sanitize_conversation_name(&n));

    let conv_service = match &state.conversation_service {
        Some(s) => s,
        None => {
            send_error(user_id, "SERVICE_UNAVAILABLE", "Conversation service not available".to_string(), state).await;
            return;
        }
    };

    // Ensure creator is in participants
    let mut all_participants = participants;
    if !all_participants.contains(&user_id) {
        all_participants.push(user_id);
    }

    match conv_service.create_conversation(conversation_type.clone(), user_id, all_participants, name).await {
        Ok(conversation) => {
            let updated_at = conversation.last_message_at
                .map(|dt| dt.timestamp_millis())
                .unwrap_or_else(|| conversation.created_at.timestamp_millis());

            let summary = crate::message::ConversationSummary {
                id: conversation.id,
                conversation_type,
                name: conversation.name,
                avatar_url: conversation.avatar_url,
                participant_count: conversation.participant_count,
                participants: vec![],
                last_message: None,
                unread_count: 0,
                updated_at,
                is_muted: false,
            };

            let response = ServerMessage::ConversationCreated { conversation: summary };
            state.connection_manager.send_to_user(&user_id, response.into()).await;
        }
        Err(e) => {
            tracing::error!(user_id = %user_id, error = %e, "Failed to create conversation");
            send_error(user_id, "CREATE_FAILED", e.to_string(), state).await;
        }
    }
}

async fn handle_update_presence(
    user_id: Uuid,
    status: crate::message::PresenceStatus,
    state: &AppState,
) {
    if let Some(ref tracker) = state.presence_tracker {
        if let Err(e) = tracker.update_presence(user_id, status.clone()).await {
            tracing::warn!(user_id = %user_id, error = %e, "Failed to update presence");
            return;
        }

        // Broadcast status change to subscribers
        if let Some(ref broadcaster) = state.presence_broadcaster {
            let _ = broadcaster.broadcast_to_subscribers(user_id, status).await;
        }
    }
}

async fn handle_subscribe_presence(
    user_id: Uuid,
    target_user_ids: Vec<Uuid>,
    state: &AppState,
) {
    // Limit subscriptions to prevent abuse
    let target_user_ids: Vec<Uuid> = target_user_ids.into_iter().take(100).collect();

    if target_user_ids.is_empty() {
        return;
    }

    if let Some(ref tracker) = state.presence_tracker {
        if let Err(e) = tracker.subscribe(user_id, &target_user_ids).await {
            tracing::warn!(user_id = %user_id, error = %e, "Failed to subscribe to presence");
            send_error(user_id, "SUBSCRIBE_FAILED", e.to_string(), state).await;
            return;
        }

        // Send initial presence status for subscribed users
        if let Some(ref broadcaster) = state.presence_broadcaster {
            let _ = broadcaster.send_initial_presence(user_id, &target_user_ids).await;
        }

        tracing::debug!(
            user_id = %user_id,
            target_count = target_user_ids.len(),
            "Subscribed to presence updates"
        );
    }
}

async fn handle_unsubscribe_presence(
    user_id: Uuid,
    target_user_ids: Vec<Uuid>,
    state: &AppState,
) {
    if target_user_ids.is_empty() {
        return;
    }

    if let Some(ref tracker) = state.presence_tracker {
        if let Err(e) = tracker.unsubscribe(user_id, &target_user_ids).await {
            tracing::warn!(user_id = %user_id, error = %e, "Failed to unsubscribe from presence");
        }

        tracing::debug!(
            user_id = %user_id,
            target_count = target_user_ids.len(),
            "Unsubscribed from presence updates"
        );
    }
}

async fn send_error(user_id: Uuid, code: &str, message: String, state: &AppState) {
    let error = ServerMessage::error(code, message);
    state.connection_manager.send_to_user(&user_id, error.into()).await;
}

/// Deliver queued offline messages to a newly connected user
async fn deliver_offline_messages(user_id: Uuid, state: &AppState) {
    match state.offline_queue.drain_messages(user_id).await {
        Ok(messages) => {
            if messages.is_empty() {
                return;
            }

            tracing::info!(
                user_id = %user_id,
                count = messages.len(),
                "Delivering offline messages"
            );

            for queued in messages {
                let outbound: OutboundMessage = queued.message.into();
                state.connection_manager.send_to_user(&user_id, outbound).await;
            }
        }
        Err(e) => {
            tracing::warn!(
                user_id = %user_id,
                error = %e,
                "Failed to retrieve offline messages"
            );
        }
    }
}

/// Handle sync unread request - returns all unread counts for the user
async fn handle_sync_unread(user_id: Uuid, state: &AppState) {
    if let Some(ref cache) = state.redis_cache {
        match cache.get_unread_sync(user_id).await {
            Ok((total, per_conversation)) => {
                let response = ServerMessage::UnreadSync {
                    total,
                    per_conversation,
                };
                state.connection_manager.send_to_user(&user_id, response.into()).await;
            }
            Err(e) => {
                tracing::warn!(user_id = %user_id, error = %e, "Failed to sync unread counts");
                send_error(user_id, "SYNC_FAILED", e.to_string(), state).await;
            }
        }
    } else {
        // No Redis cache available, return empty counts
        let response = ServerMessage::UnreadSync {
            total: 0,
            per_conversation: std::collections::HashMap::new(),
        };
        state.connection_manager.send_to_user(&user_id, response.into()).await;
    }
}

/// Handle get reactions request - returns reactions for specified messages
async fn handle_get_reactions(
    user_id: Uuid,
    message_ids: Vec<Uuid>,
    state: &AppState,
) {
    let reaction_service = match &state.reaction_service {
        Some(s) => s,
        None => {
            send_error(user_id, "SERVICE_UNAVAILABLE", "Reaction service not available".to_string(), state).await;
            return;
        }
    };

    // Limit the number of messages to prevent abuse
    let message_ids: Vec<Uuid> = message_ids.into_iter().take(100).collect();

    match reaction_service.get_reactions_batch(&message_ids).await {
        Ok(batch_reactions) => {
            // Convert to ReactionInfo format
            let mut reactions_map: std::collections::HashMap<Uuid, Vec<crate::message::ReactionInfo>> =
                std::collections::HashMap::new();

            for (msg_id, emoji_reactions) in batch_reactions {
                let reaction_infos: Vec<crate::message::ReactionInfo> = emoji_reactions
                    .into_iter()
                    .map(|(emoji, users)| {
                        let user_reacted = users.contains(&user_id);
                        crate::message::ReactionInfo {
                            emoji,
                            count: users.len() as u32,
                            users,
                            user_reacted,
                        }
                    })
                    .collect();
                reactions_map.insert(msg_id, reaction_infos);
            }

            let response = ServerMessage::Reactions {
                reactions: reactions_map,
            };
            state.connection_manager.send_to_user(&user_id, response.into()).await;
        }
        Err(e) => {
            tracing::warn!(user_id = %user_id, error = %e, "Failed to get reactions");
            send_error(user_id, "FETCH_FAILED", e.to_string(), state).await;
        }
    }
}

/// Handle pin message request
async fn handle_pin_message(
    user_id: Uuid,
    conversation_id: Uuid,
    message_id: Uuid,
    state: &AppState,
) {
    let handler = match &state.message_handler {
        Some(h) => h,
        None => {
            send_error(user_id, "SERVICE_UNAVAILABLE", "Message service not available".to_string(), state).await;
            return;
        }
    };

    match handler.handle_pin_message(user_id, conversation_id, message_id).await {
        Ok(pinned_at) => {
            tracing::info!(
                user_id = %user_id,
                conversation_id = %conversation_id,
                message_id = %message_id,
                pinned_at = %pinned_at,
                "Message pinned"
            );
        }
        Err(e) => {
            tracing::warn!(user_id = %user_id, error = %e, "Failed to pin message");
            send_error(user_id, "PIN_FAILED", e.to_string(), state).await;
        }
    }
}

/// Handle unpin message request
async fn handle_unpin_message(
    user_id: Uuid,
    conversation_id: Uuid,
    message_id: Uuid,
    state: &AppState,
) {
    let handler = match &state.message_handler {
        Some(h) => h,
        None => {
            send_error(user_id, "SERVICE_UNAVAILABLE", "Message service not available".to_string(), state).await;
            return;
        }
    };

    match handler.handle_unpin_message(user_id, conversation_id, message_id).await {
        Ok(()) => {
            tracing::info!(
                user_id = %user_id,
                conversation_id = %conversation_id,
                message_id = %message_id,
                "Message unpinned"
            );
        }
        Err(e) => {
            tracing::warn!(user_id = %user_id, error = %e, "Failed to unpin message");
            send_error(user_id, "UNPIN_FAILED", e.to_string(), state).await;
        }
    }
}

/// Handle mute conversation request
async fn handle_mute_conversation(
    user_id: Uuid,
    conversation_id: Uuid,
    state: &AppState,
) {
    let handler = match &state.message_handler {
        Some(h) => h,
        None => {
            send_error(user_id, "SERVICE_UNAVAILABLE", "Message service not available".to_string(), state).await;
            return;
        }
    };

    match handler.handle_mute_conversation(user_id, conversation_id).await {
        Ok(muted_at) => {
            tracing::info!(
                user_id = %user_id,
                conversation_id = %conversation_id,
                muted_at = %muted_at,
                "Conversation muted"
            );
        }
        Err(e) => {
            tracing::warn!(user_id = %user_id, error = %e, "Failed to mute conversation");
            send_error(user_id, "MUTE_FAILED", e.to_string(), state).await;
        }
    }
}

/// Handle unmute conversation request
async fn handle_unmute_conversation(
    user_id: Uuid,
    conversation_id: Uuid,
    state: &AppState,
) {
    let handler = match &state.message_handler {
        Some(h) => h,
        None => {
            send_error(user_id, "SERVICE_UNAVAILABLE", "Message service not available".to_string(), state).await;
            return;
        }
    };

    match handler.handle_unmute_conversation(user_id, conversation_id).await {
        Ok(()) => {
            tracing::info!(
                user_id = %user_id,
                conversation_id = %conversation_id,
                "Conversation unmuted"
            );
        }
        Err(e) => {
            tracing::warn!(user_id = %user_id, error = %e, "Failed to unmute conversation");
            send_error(user_id, "UNMUTE_FAILED", e.to_string(), state).await;
        }
    }
}

// ==================== User Blocking Handlers ====================

/// Handle block user request
async fn handle_block_user(
    user_id: Uuid,
    target_user_id: Uuid,
    state: &AppState,
) {
    // Cannot block yourself
    if user_id == target_user_id {
        send_error(user_id, "CANNOT_BLOCK_SELF", "Cannot block yourself".to_string(), state).await;
        return;
    }

    let blocking_service = match &state.blocking_service {
        Some(s) => s,
        None => {
            send_error(user_id, "SERVICE_UNAVAILABLE", "Blocking service not available".to_string(), state).await;
            return;
        }
    };

    match blocking_service.block_user(user_id, target_user_id, None).await {
        Ok(blocked_at) => {
            // Unsubscribe from each other's presence
            if let Some(ref presence) = state.presence_tracker {
                let _ = presence.unsubscribe(user_id, &[target_user_id]).await;
                let _ = presence.unsubscribe(target_user_id, &[user_id]).await;
            }

            // Send confirmation
            let response = ServerMessage::UserBlocked {
                user_id: target_user_id,
                blocked_at: blocked_at.timestamp_millis(),
            };
            state.connection_manager.send_to_user(&user_id, response.into()).await;

            tracing::info!(
                user_id = %user_id,
                target_user_id = %target_user_id,
                "User blocked"
            );
        }
        Err(e) => {
            tracing::warn!(user_id = %user_id, error = %e, "Failed to block user");
            send_error(user_id, "BLOCK_FAILED", e.to_string(), state).await;
        }
    }
}

/// Handle unblock user request
async fn handle_unblock_user(
    user_id: Uuid,
    target_user_id: Uuid,
    state: &AppState,
) {
    let blocking_service = match &state.blocking_service {
        Some(s) => s,
        None => {
            send_error(user_id, "SERVICE_UNAVAILABLE", "Blocking service not available".to_string(), state).await;
            return;
        }
    };

    match blocking_service.unblock_user(user_id, target_user_id).await {
        Ok(()) => {
            // Send confirmation
            let response = ServerMessage::UserUnblocked {
                user_id: target_user_id,
            };
            state.connection_manager.send_to_user(&user_id, response.into()).await;

            tracing::info!(
                user_id = %user_id,
                target_user_id = %target_user_id,
                "User unblocked"
            );
        }
        Err(e) => {
            tracing::warn!(user_id = %user_id, error = %e, "Failed to unblock user");
            send_error(user_id, "UNBLOCK_FAILED", e.to_string(), state).await;
        }
    }
}

/// Handle get blocked users request
async fn handle_get_blocked_users(
    user_id: Uuid,
    state: &AppState,
) {
    let blocking_service = match &state.blocking_service {
        Some(s) => s,
        None => {
            send_error(user_id, "SERVICE_UNAVAILABLE", "Blocking service not available".to_string(), state).await;
            return;
        }
    };

    match blocking_service.get_blocked_users(user_id).await {
        Ok(users) => {
            let response = ServerMessage::BlockedUsers { users };
            state.connection_manager.send_to_user(&user_id, response.into()).await;
        }
        Err(e) => {
            tracing::warn!(user_id = %user_id, error = %e, "Failed to get blocked users");
            send_error(user_id, "FETCH_FAILED", e.to_string(), state).await;
        }
    }
}

// ==================== Message Forwarding Handlers ====================

/// Handle forward message request
async fn handle_forward_message(
    user_id: Uuid,
    message_id: Uuid,
    source_conversation_id: Uuid,
    target_conversation_ids: Vec<Uuid>,
    state: &AppState,
) {
    let handler = match &state.message_handler {
        Some(h) => h,
        None => {
            send_error(user_id, "SERVICE_UNAVAILABLE", "Message service not available".to_string(), state).await;
            return;
        }
    };

    match handler.handle_forward_message(
        user_id,
        message_id,
        source_conversation_id,
        target_conversation_ids,
    ).await {
        Ok(results) => {
            // Send results to the user
            let response = ServerMessage::MessageForwarded {
                source_message_id: message_id,
                results,
            };
            state.connection_manager.send_to_user(&user_id, response.into()).await;

            tracing::info!(
                user_id = %user_id,
                message_id = %message_id,
                "Message forwarded"
            );
        }
        Err(e) => {
            tracing::warn!(user_id = %user_id, error = %e, "Failed to forward message");
            send_error(user_id, "FORWARD_FAILED", e.to_string(), state).await;
        }
    }
}
