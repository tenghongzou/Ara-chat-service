# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Added
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
