# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

## [1.11.0] - 2026-01-05

### Added
- **Connection Multiplexing - Conversation Subscription**: Per-connection conversation subscriptions for bandwidth optimization
  - Two subscription modes:
    - **Legacy mode** (default): Receive all messages - backward compatible with existing clients
    - **Explicit mode**: Only receive messages for subscribed conversations - enabled on first SubscribeConversations
  - Per-connection subscription management with O(1) DashSet lookup
  - Max 100 subscriptions per connection (configurable, ~3.2KB memory overhead)
  - Auto-subscribe triggers:
    - On FetchHistory: Auto-subscribe to the conversation being fetched
    - On SendMessage: Auto-subscribe to the conversation being messaged
  - System messages (mentions, etc.) bypass subscription filter and are always delivered
  - WebSocket client messages:
    - `SubscribeConversations { conversation_ids: Vec<Uuid> }` - Subscribe to conversations
    - `UnsubscribeConversations { conversation_ids: Vec<Uuid> }` - Unsubscribe from conversations
  - WebSocket server message:
    - `SubscriptionUpdated { subscribed, unsubscribed, total_subscriptions }` - Confirmation
  - Subscription filtering at MessageRouter layer for all message types:
    - Regular messages, message edits, recalls
    - Typing indicators, thread updates
    - Pin/unpin notifications
  - Subscription metrics for monitoring:
    - `chat_subscription_filtered_total` - Messages filtered (not delivered) due to subscription
    - `chat_subscription_changes_total` - Subscribe/unsubscribe operations by action
    - `chat_subscription_count` - Histogram of subscriptions per connection
  - Configurable via `CHAT__SUBSCRIPTION__*` environment variables:
    - `CHAT__SUBSCRIPTION__MAX_SUBSCRIPTIONS` - Max subscriptions per connection (default: 100)
    - `CHAT__SUBSCRIPTION__AUTO_SUBSCRIBE_ON_FETCH_HISTORY` - Auto-subscribe on FetchHistory (default: true)
    - `CHAT__SUBSCRIPTION__AUTO_SUBSCRIBE_ON_SEND_MESSAGE` - Auto-subscribe on SendMessage (default: true)
  - Expected bandwidth savings: 50-80% for mobile users with many conversations

## [1.10.0] - 2026-01-05

### Added
- **Message Compression**: Application-level zstd compression for WebSocket messages
  - zstd compression using the `zstd` crate for optimal ratio and speed
  - Configurable compression threshold (default 1KB, skip small messages)
  - Configurable compression level (1-22, default 3)
  - Capability negotiation during WebSocket handshake
    - Client sends `Capabilities` message with supported compression algorithms
    - Server responds with `CapabilitiesAck` confirming compression settings
  - Backward compatible with non-compressed clients
    - Legacy clients continue to receive JSON text messages
    - Only clients that announce compression support receive binary messages
  - Binary message format with 1-byte flags header:
    - Bit 0: compressed (1) or raw (0)
    - Bits 1-2: algorithm (00=zstd)
    - Bits 3-7: reserved
  - Compression metrics for monitoring:
    - `chat_compression_ratio` - Histogram of compression ratios
    - `chat_messages_compressed_total` - Counter of compressed messages
    - `chat_messages_uncompressed_total` - Counter of uncompressed messages
    - `chat_compression_bytes_saved_total` - Total bytes saved
  - Configurable via `CHAT__COMPRESSION__*` environment variables:
    - `CHAT__COMPRESSION__ENABLED` - Enable/disable compression (default: true)
    - `CHAT__COMPRESSION__ALGORITHM` - Algorithm (zstd or none)
    - `CHAT__COMPRESSION__LEVEL` - Compression level 1-22 (default: 3)
    - `CHAT__COMPRESSION__THRESHOLD` - Min size to compress (default: 1024)
    - `CHAT__COMPRESSION__MAX_DECOMPRESSED_SIZE` - Max decompressed size (default: 10MB)
  - Expected bandwidth savings: 30-50% for typical chat usage

## [1.9.0] - 2026-01-04

### Added
- **Email Notification for Offline Users**: Send email notifications when offline users receive messages
  - SMTP backend via lettre crate with TLS support
  - SendGrid HTTP API backend for cloud deployment
  - Configurable delay before sending (default 2 minutes, avoids email for quick reconnects)
  - Message batching within time window (default 5 minutes)
  - User email preferences:
    - Enable/disable email notifications
    - Message notifications toggle
    - @mention notifications toggle
    - Digest mode (immediate or daily)
    - Quiet hours (UTC-based, skip sending during specified hours)
  - Rate limiting (default 5 emails per hour per user)
  - HTML and plain text email templates
  - Background queue processor (10-second interval)
  - REST API endpoints:
    - `GET /api/v1/email/preferences` - Get user email preferences
    - `PUT /api/v1/email/preferences` - Update email preferences
    - `POST /api/v1/email/test` - Send test email to verify configuration
    - `GET /api/v1/email/status` - Check email service status
  - Configurable via `CHAT__EMAIL__*` environment variables
  - Database migration: `016_email_notifications.sql`
- **Custom Emoji Support**: User-uploaded custom emojis with pack management
  - Upload custom emoji images (PNG, GIF, WebP)
  - Emoji packs for grouping related emojis
  - Shortcode format (`:emoji_name:`) with 2-50 char alphanumeric+underscore
  - Search emojis by name or shortcode
  - 64x64 thumbnail generation
  - Content hash deduplication (SHA256)
  - Max 256KB per emoji, auto-resize to 128x128 max
  - Tenant-scoped emojis for multi-tenant isolation
  - GIF animation preservation
  - REST API endpoints:
    - `POST /api/v1/emojis` - Upload custom emoji
    - `GET /api/v1/emojis` - List custom emojis
    - `GET /api/v1/emojis/search` - Search emojis
    - `GET /api/v1/emojis/{id}` - Get emoji details
    - `DELETE /api/v1/emojis/{id}` - Delete emoji
    - `POST /api/v1/emoji-packs` - Create pack
    - `GET /api/v1/emoji-packs` - List packs
    - `GET /api/v1/emoji-packs/{id}` - Get pack with emojis
    - `PATCH /api/v1/emoji-packs/{id}` - Update pack
    - `DELETE /api/v1/emoji-packs/{id}` - Delete pack
  - Database migration: `015_custom_emojis.sql`
- **Markdown Rendering Hints**: Position-based formatting hints for client-side rendering
  - Parse markdown content synchronously on message send
  - Support for bold, italic, inline code, code blocks (with language)
  - Support for links, strikethrough, headings (1-6), blockquotes, list items
  - Fast pre-filter (`might_contain_markdown`) for plain text
  - Store hints inline with message as JSONB
  - Skip parsing for non-text content (images, files)
  - 23 unit tests for markdown module
  - Database migration: `014_rendering_hints.sql`
- **Link Preview**: Extract and display Open Graph metadata from URLs
  - Extract URLs from message content (max 5 per message)
  - Fetch Open Graph metadata asynchronously in background
  - Cache previews in Redis (24-hour TTL)
  - Store in PostgreSQL for persistence
  - Background worker processes pending previews every 5 seconds
  - Real-time WebSocket notification (LinkPreviewReady)
  - REST API endpoints:
    - `GET /api/v1/messages/{id}/previews` - Get link previews for a message
    - `POST /api/v1/messages/{id}/previews/refresh` - Re-fetch failed previews
  - Database migration: `013_link_previews.sql`
- **Message Forwarding**: Forward messages to one or more conversations
  - Forward messages (up to 10 conversations per request)
  - Preserve original message metadata (sender, conversation, content)
  - Block check for DM conversations
  - Real-time WebSocket notification (MessageForwarded)
  - WebSocket client message: ForwardMessage
  - REST API endpoint:
    - `POST /api/v1/messages/{id}/forward` - Forward a message
  - Database migration: `012_message_forwarding.sql`
- OpenAPI 3.0 specification (`openapi.yaml`)
- Bilingual documentation (English and Traditional Chinese)
- Environment configuration template (`.env.example`)
- Contributing guidelines (`CONTRIBUTING.md`)
- **Notification Service Integration**: Push notifications via Redis Pub/Sub
  - New message notifications for offline users
  - @mention notifications (high priority)
  - Emoji reaction notifications
  - Configurable notification types via settings
  - Channel format: `notification:user:{user_id}`
- **GDPR Compliance**: Full GDPR support for user data management
  - Data Export (Article 20 - Data Portability): Export user data to JSON
  - Data Deletion (Article 17 - Right to Erasure): Delete or anonymize user data
  - Audit Logging (Article 30 - Records of Processing): 7-year retention
  - REST API endpoints:
    - `POST /api/v1/gdpr/export` - Request data export
    - `GET /api/v1/gdpr/export/{id}` - Get export status
    - `DELETE /api/v1/gdpr/data` - Request data deletion
    - `GET /api/v1/gdpr/audit` - View audit log
  - Configurable via `CHAT__GDPR__*` environment variables
- **Message Pinning**: Pin important messages in conversations
  - Pin/unpin messages (Owner and Admin roles only)
  - Fetch pinned messages list with pagination
  - Real-time WebSocket notifications (MessagePinned, MessageUnpinned)
  - WebSocket client messages: PinMessage, UnpinMessage
  - REST API endpoints:
    - `POST /api/v1/conversations/{id}/messages/{msg_id}/pin` - Pin a message
    - `DELETE /api/v1/conversations/{id}/messages/{msg_id}/pin` - Unpin a message
    - `GET /api/v1/conversations/{id}/pinned` - Get pinned messages
  - Database migration: `009_message_pins.sql`
- **Conversation Muting**: Mute conversations to skip push notifications
  - Mute/unmute conversations (any participant)
  - Muted users still receive WebSocket messages (real-time updates)
  - Muted users do NOT receive push notifications
  - @mentions override mute status (always notify)
  - WebSocket client messages: MuteConversation, UnmuteConversation
  - WebSocket server messages: ConversationMuted, ConversationUnmuted
  - REST API endpoints:
    - `POST /api/v1/conversations/{id}/mute` - Mute a conversation
    - `DELETE /api/v1/conversations/{id}/mute` - Unmute a conversation
    - `GET /api/v1/conversations/muted` - Get muted conversations
  - Database migration: `010_conversation_muting.sql`
- **User Blocking**: Block users to prevent messaging and presence visibility
  - Block/unblock users with persistent storage
  - Bidirectional DM blocking (neither party can message)
  - Message filtering in group chats (blocked users' messages hidden)
  - Presence hiding (blocked users cannot see each other's status)
  - Blocked users list retrieval
  - WebSocket client messages: BlockUser, UnblockUser, GetBlockedUsers
  - WebSocket server messages: UserBlocked, UserUnblocked, BlockedUsers
  - REST API endpoints:
    - `POST /api/v1/users/{id}/block` - Block a user
    - `DELETE /api/v1/users/{id}/block` - Unblock a user
    - `GET /api/v1/blocked-users` - Get blocked users list
  - Database migration: `011_user_blocking.sql`

## [1.0.0] - 2026-01-03

### Added

#### Core Messaging
- Private 1:1 conversations
- Group chat with unlimited participants
- Message history with cursor-based pagination
- Message recall (within 2-minute window)
- Message editing (within 15-minute window)
- Client message ID for deduplication (idempotent sends)

#### Engagement Features
- @mentions with participant validation
- Emoji reactions (toggle add/remove)
- Typing indicators
- Read receipts with unread count tracking

#### Real-time Infrastructure
- WebSocket connections with heartbeat keepalive
- Presence tracking (online/away/busy/offline)
- Presence subscriptions with automatic updates
- Offline message queue (7-day retention, 1000 messages per user)

#### Cluster Mode
- Multi-instance deployment via Redis Pub/Sub
- Cross-instance message routing
- Session store for user location tracking
- Graceful shutdown with client notification

#### Database
- PostgreSQL with connection pooling
- Date-partitioned message tables (pg_partman compatible)
- O(1) direct message lookup via SHA256 hash
- GIN index for @mention queries
- Citus-compatible sharding (1024 shards)

#### Security & Performance
- JWT authentication (HS256)
- Multi-tenant data isolation
- Distributed rate limiting (60 msg/min per user)
- Circuit breaker for external service calls
- Per-user connection limits (default: 5)
- Global connection limits (default: 100K)

#### Observability
- Prometheus metrics endpoint
- OpenTelemetry distributed tracing
- Kubernetes health probes (liveness/readiness)
- Structured JSON logging

#### API
- REST API for CRUD operations
- WebSocket API for real-time messaging
- Message search with PostgreSQL full-text search

### Infrastructure
- Multi-stage Docker build
- Non-root container user (UID 1000)
- K6 load testing suite

---

## Version History

| Version | Date | Description |
|---------|------|-------------|
| 1.11.0 | 2026-01-05 | Connection multiplexing (conversation subscription) |
| 1.10.0 | 2026-01-05 | Message compression (zstd) |
| 1.9.0 | 2026-01-04 | Email notifications for offline users |
| 1.8.0 | 2026-01-04 | Custom emoji support |
| 1.7.0 | 2026-01-04 | Markdown rendering hints |
| 1.6.0 | 2026-01-04 | Link preview |
| 1.5.0 | 2026-01-03 | Message forwarding |
| 1.4.0 | 2026-01-03 | User blocking |
| 1.3.0 | 2026-01-03 | Message pinning, conversation muting |
| 1.2.0 | 2026-01-03 | Threading, notifications, GDPR |
| 1.1.0 | 2026-01-03 | File handling |
| 1.0.0 | 2026-01-03 | Initial release |

## Migration Notes

### Upgrading to 1.0.0

This is the initial release. No migration required.

### Database Migrations

Run migrations with:
```bash
sqlx migrate run
```

Current migrations:
1. `001_conversations.sql` - Conversations and participants
2. `002_messages.sql` - Messages with partitioning
3. `003_reactions.sql` - Emoji reactions
4. `004_read_receipts.sql` - Read receipt tracking
5. `009_message_pins.sql` - Message pinning support
6. `010_conversation_muting.sql` - Conversation muting support
7. `011_user_blocking.sql` - User blocking support
8. `012_message_forwarding.sql` - Message forwarding support
9. `013_link_previews.sql` - Link preview metadata storage
10. `014_rendering_hints.sql` - Markdown rendering hints (JSONB column)
11. `015_custom_emojis.sql` - Custom emoji and emoji packs
12. `016_email_notifications.sql` - Email preferences, queue, and rate limits
