# Ara Chat Service

Real-time instant messaging service with WebSocket support, designed for 100M DAU and 10M peak concurrent connections.

## Features

- **Private & Group Chat** - 1:1 direct messages and group conversations
- **Permanent Message Storage** - Messages stored permanently with partitioned tables
- **Read Receipts** - Track message read status per user
- **Message Recall** - Delete sent messages within time limit
- **@Mentions** - Tag users with notification support
- **Emoji Reactions** - React to messages with emojis
- **Presence Tracking** - Real-time online/offline status
- **Typing Indicators** - Show when users are typing

## Architecture

```
┌─────────────────────────────────────────────────────────────────┐
│                        Load Balancer                            │
└─────────────────────────┬───────────────────────────────────────┘
                          │
        ┌─────────────────┼─────────────────┐
        ▼                 ▼                 ▼
┌───────────────┐ ┌───────────────┐ ┌───────────────┐
│  Chat Pod 1   │ │  Chat Pod 2   │ │  Chat Pod N   │
│  (100K conn)  │ │  (100K conn)  │ │  (100K conn)  │
└───────┬───────┘ └───────┬───────┘ └───────┬───────┘
        │                 │                 │
        └─────────────────┼─────────────────┘
                          │
        ┌─────────────────┼─────────────────┐
        ▼                 ▼                 ▼
┌───────────────┐ ┌───────────────┐ ┌───────────────┐
│ Redis Cluster │ │   PostgreSQL  │ │  Prometheus   │
│  (Pub/Sub)    │ │    (Citus)    │ │   (Metrics)   │
└───────────────┘ └───────────────┘ └───────────────┘
```

## Tech Stack

| Component | Technology |
|-----------|------------|
| Language | Rust 1.75+ |
| Web Framework | Axum 0.8 |
| Async Runtime | Tokio |
| Database | PostgreSQL (Citus for sharding) |
| Cache | Redis Cluster |
| Metrics | Prometheus |
| Tracing | OpenTelemetry |

## Getting Started

### Prerequisites

- Rust 1.75+
- PostgreSQL 15+
- Redis 7+
- Docker & Docker Compose

### Local Development

```bash
# Clone and enter directory
cd services/chat

# Copy environment template
cp .env.example .env

# Run with Docker Compose (from project root)
docker compose up chat redis postgres -d

# Or run locally
cargo run
```

### Configuration

Environment variables:

| Variable | Default | Description |
|----------|---------|-------------|
| `CHAT__HOST` | `0.0.0.0` | Server bind address |
| `CHAT__PORT` | `8082` | Server port |
| `CHAT__JWT__SECRET` | - | JWT signing secret (required) |
| `CHAT__REDIS__URL` | `redis://localhost:6379` | Redis connection URL |
| `CHAT__DATABASE__URL` | - | PostgreSQL connection URL |
| `CHAT__CLUSTER__ENABLED` | `false` | Enable cluster mode |
| `CHAT__CLUSTER__SERVER_ID` | auto-generated | Unique server identifier |

## API Endpoints

### Health & Metrics

| Endpoint | Method | Description |
|----------|--------|-------------|
| `/health` | GET | Basic health check |
| `/health/live` | GET | Kubernetes liveness probe |
| `/health/ready` | GET | Kubernetes readiness probe |
| `/health/detailed` | GET | Detailed health report |
| `/metrics` | GET | Prometheus metrics |

### WebSocket

```
ws://localhost:8082/ws?token=<JWT>
```

### REST API

| Endpoint | Method | Description |
|----------|--------|-------------|
| `/api/v1/conversations` | GET | List user's conversations |
| `/api/v1/conversations` | POST | Create new conversation |
| `/api/v1/conversations/{id}` | GET | Get conversation details |
| `/api/v1/conversations/{id}/messages` | GET | Get message history |
| `/api/v1/conversations/{id}/messages` | POST | Send message |
| `/api/v1/conversations/{id}/read` | POST | Mark as read |
| `/api/v1/unread` | GET | Get unread counts |
| `/api/v1/search/messages` | GET | Search messages |

## WebSocket Protocol

### Client Messages

```json
// Authenticate
{"type": "Authenticate", "payload": {"token": "JWT_TOKEN"}}

// Send message
{"type": "SendMessage", "payload": {
  "conversation_id": "uuid",
  "content": "Hello!",
  "content_type": "Text",
  "mentions": []
}}

// Mark as read
{"type": "MarkRead", "payload": {
  "conversation_id": "uuid",
  "message_id": "uuid"
}}

// Typing indicator
{"type": "Typing", "payload": {
  "conversation_id": "uuid",
  "is_typing": true
}}

// Ping
{"type": "Ping"}
```

### Server Messages

```json
// Authenticated
{"type": "authenticated", "user_id": "uuid"}

// New message
{"type": "message", "message": {...}}

// Message sent confirmation
{"type": "message_sent", "conversation_id": "uuid", "message_id": "uuid"}

// Read receipt
{"type": "read_receipt", "conversation_id": "uuid", "user_id": "uuid", "message_id": "uuid"}

// Typing
{"type": "typing", "conversation_id": "uuid", "user_id": "uuid", "is_typing": true}

// Pong
{"type": "pong"}

// Error
{"type": "error", "code": "ERROR_CODE", "message": "Error description"}
```

## Scaling

### Billion-Scale Infrastructure

| Component | Configuration |
|-----------|---------------|
| User Sharding | 1024 shards with consistent hashing |
| PostgreSQL | Citus distributed tables |
| Redis | Cluster mode with 16384 slots |
| Connection Pools | Adaptive sizing (5-100 connections) |

### Horizontal Scaling

```yaml
# Kubernetes HPA example
apiVersion: autoscaling/v2
kind: HorizontalPodAutoscaler
metadata:
  name: chat-hpa
spec:
  scaleTargetRef:
    apiVersion: apps/v1
    kind: Deployment
    name: chat
  minReplicas: 10
  maxReplicas: 120
  metrics:
  - type: Resource
    resource:
      name: cpu
      target:
        type: Utilization
        averageUtilization: 70
  - type: Pods
    pods:
      metric:
        name: chat_websocket_connections_total
      target:
        type: AverageValue
        averageValue: 80000
```

## Load Testing

K6 load tests are provided in `tests/load/`:

```bash
# WebSocket connection test
k6 run tests/load/websocket_load.js

# Message throughput test
k6 run --vus 100 --duration 10m tests/load/message_throughput.js
```

### Performance Targets

| Metric | Target |
|--------|--------|
| Concurrent Connections | 10M+ |
| Message Latency (p95) | < 100ms |
| Message Throughput | 100K+ msg/s |
| Connection Success Rate | > 99% |

## Monitoring

### Key Metrics

```promql
# Active connections
chat_websocket_connections_total

# Message throughput
rate(chat_messages_sent_total[5m])

# Message latency
histogram_quantile(0.95, rate(chat_message_processing_duration_seconds_bucket[5m]))

# Circuit breaker status
chat_circuit_breaker_state

# Database pool utilization
chat_db_pool_size{state="active"} / chat_db_pool_size{state="idle"}
```

### Alerting Rules

```yaml
groups:
- name: chat
  rules:
  - alert: HighConnectionCount
    expr: chat_websocket_connections_total > 90000
    for: 5m
    labels:
      severity: warning
    annotations:
      summary: High connection count on {{ $labels.instance }}

  - alert: CircuitBreakerOpen
    expr: chat_circuit_breaker_state == 2
    for: 1m
    labels:
      severity: critical
    annotations:
      summary: Circuit breaker open for {{ $labels.service }}
```

## Database Schema

### Tables

- `conversations` - Conversation metadata
- `conversation_participants` - User-conversation mapping
- `direct_message_lookup` - Fast 1:1 conversation lookup
- `messages` - Partitioned message storage
- `message_reactions` - Emoji reactions
- `read_receipts` - Read status tracking

### Migrations

```bash
# Run migrations
sqlx migrate run

# Create new migration
sqlx migrate add <name>
```

## Project Structure

```
services/chat/
├── Cargo.toml
├── Dockerfile
├── migrations/
│   ├── 001_conversations.sql
│   ├── 002_messages.sql
│   ├── 003_reactions.sql
│   └── 004_read_receipts.sql
├── src/
│   ├── main.rs
│   ├── lib.rs
│   ├── api/
│   │   ├── health.rs
│   │   ├── rest.rs
│   │   ├── routes.rs
│   │   └── websocket.rs
│   ├── domain/
│   │   ├── cluster/
│   │   ├── connection/
│   │   ├── conversation/
│   │   ├── mention/
│   │   ├── message/
│   │   ├── presence/
│   │   ├── reaction/
│   │   └── receipt/
│   ├── infrastructure/
│   │   ├── auth/
│   │   ├── circuit_breaker.rs
│   │   ├── config/
│   │   ├── metrics/
│   │   ├── postgres/
│   │   ├── ratelimit/
│   │   ├── redis/
│   │   └── sharding/
│   ├── server/
│   ├── shutdown.rs
│   ├── tasks.rs
│   └── telemetry.rs
└── tests/
    └── load/
        ├── websocket_load.js
        └── message_throughput.js
```

## Development

### Build

```bash
cargo build --release
```

### Test

```bash
cargo test
```

### Lint

```bash
cargo clippy
cargo fmt --check
```

## License

Proprietary - Ara Team
