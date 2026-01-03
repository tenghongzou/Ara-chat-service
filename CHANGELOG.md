# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Added
- OpenAPI 3.0 specification (`openapi.yaml`)
- Bilingual documentation (English and Traditional Chinese)
- Environment configuration template (`.env.example`)
- Contributing guidelines (`CONTRIBUTING.md`)

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
