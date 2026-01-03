# Installation

This guide covers Docker deployment and local development setup.

## Prerequisites

| Requirement | Version |
|------------|---------|
| Rust | 1.75+ |
| PostgreSQL | 15+ |
| Redis | 7+ |
| Docker | 24+ |
| Docker Compose | 2.20+ |

## Docker Deployment (Recommended)

### Quick Start

```bash
# From project root
cd /path/to/Ara-infra

# Copy environment configuration
cp .env.example .env

# Start services
docker compose up -d chat redis postgres

# Verify health
curl http://localhost:8082/health
```

### Service URLs

| Service | URL |
|---------|-----|
| WebSocket | `ws://localhost:8082/ws?token=JWT` |
| REST API | `http://localhost:8082/api/v1/` |
| Health Check | `http://localhost:8082/health` |
| Metrics | `http://localhost:8082/metrics` |

### Cluster Mode (Multi-Instance)

For high-availability deployment with multiple chat instances:

```bash
# Start cluster with 3 chat nodes
docker compose -f docker-compose.yml -f docker-compose.cluster.yml up -d
```

This starts:
- `chat-1` on port 8082
- `chat-2` on port 8083
- `chat-3` on port 8084

## Local Development

### 1. Install Rust Toolchain

```bash
# Install rustup
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh

# Install stable toolchain
rustup default stable

# Add components
rustup component add clippy rustfmt
```

### 2. Start Dependencies

```bash
# Start PostgreSQL and Redis
docker compose up -d postgres redis

# Or use local installations
# PostgreSQL: brew install postgresql@15
# Redis: brew install redis
```

### 3. Configure Environment

```bash
cd services/chat

# Copy environment template
cp .env.example .env

# Edit configuration
vim .env
```

**Minimum required settings:**
```env
CHAT__JWT__SECRET=your-jwt-secret-key-minimum-32-characters
CHAT__DATABASE__URL=postgres://ara:ara_password@localhost:5432/ara_chat
CHAT__REDIS__URL=redis://localhost:6379
```

### 4. Run Migrations

```bash
# Install sqlx-cli
cargo install sqlx-cli --no-default-features --features postgres

# Run migrations
sqlx migrate run
```

### 5. Start the Service

```bash
# Development mode with hot reload
cargo watch -x run

# Or standard run
cargo run
```

## Configuration Reference

### Server Settings

| Variable | Default | Description |
|----------|---------|-------------|
| `CHAT__HOST` | `0.0.0.0` | Bind address |
| `CHAT__PORT` | `8082` | Listen port |
| `RUN_MODE` | `development` | development / production |
| `RUST_LOG` | `info` | Log level |

### JWT Authentication

| Variable | Required | Description |
|----------|----------|-------------|
| `CHAT__JWT__SECRET` | Yes | JWT signing secret (min 32 chars) |
| `CHAT__JWT__ISSUER` | No | Expected token issuer |
| `CHAT__JWT__AUDIENCE` | No | Expected token audience |

### Database

| Variable | Default | Description |
|----------|---------|-------------|
| `CHAT__DATABASE__URL` | - | PostgreSQL connection URL |
| `CHAT__DATABASE__MAX_CONNECTIONS` | `20` | Maximum pool size |
| `CHAT__DATABASE__MIN_CONNECTIONS` | `5` | Minimum pool size |
| `CHAT__DATABASE__RUN_MIGRATIONS` | `false` | Auto-run migrations |

### Redis

| Variable | Default | Description |
|----------|---------|-------------|
| `CHAT__REDIS__URL` | `redis://localhost:6379` | Redis URL |
| `CHAT__REDIS__POOL_SIZE` | `10` | Connection pool size |
| `CHAT__REDIS__CLUSTER_ENABLED` | `false` | Enable cluster mode |

### WebSocket

| Variable | Default | Description |
|----------|---------|-------------|
| `CHAT__WEBSOCKET__MAX_CONNECTIONS` | `100000` | Global connection limit |
| `CHAT__WEBSOCKET__MAX_CONNECTIONS_PER_USER` | `5` | Per-user limit |
| `CHAT__WEBSOCKET__HEARTBEAT_INTERVAL_SECONDS` | `30` | Ping interval |

### Cluster Mode

| Variable | Default | Description |
|----------|---------|-------------|
| `CHAT__CLUSTER__ENABLED` | `false` | Enable cluster routing |
| `CHAT__CLUSTER__SERVER_ID` | auto-generated | Unique instance ID |

## Database Migrations

### Creating New Migrations

```bash
# Create a new migration
sqlx migrate add <migration_name>

# Edit the generated SQL file
vim migrations/<timestamp>_<migration_name>.sql

# Apply migration
sqlx migrate run

# Rollback (if reversible)
sqlx migrate revert
```

### Existing Migrations

| Migration | Purpose |
|-----------|---------|
| `001_conversations.sql` | Conversations and participants |
| `002_messages.sql` | Messages with partitioning |
| `003_reactions.sql` | Emoji reactions |
| `004_read_receipts.sql` | Read receipt tracking |

## Troubleshooting

### Connection Refused

```bash
# Check if services are running
docker compose ps

# Check logs
docker compose logs chat

# Verify port bindings
netstat -tlnp | grep 8082
```

### Database Connection Failed

```bash
# Test PostgreSQL connection
psql $CHAT__DATABASE__URL -c "SELECT 1"

# Check migrations status
sqlx migrate info
```

### Redis Connection Failed

```bash
# Test Redis connection
redis-cli -u $CHAT__REDIS__URL ping
```

### JWT Validation Errors

1. Ensure `CHAT__JWT__SECRET` matches the backend service
2. Check token expiration (`exp` claim)
3. Verify issuer/audience if configured
