# Development Guide

This guide covers local development workflow, coding standards, and debugging tips.

## Development Environment

### Required Tools

| Tool | Purpose | Installation |
|------|---------|--------------|
| Rust 1.75+ | Compiler | `rustup` |
| cargo-watch | Hot reload | `cargo install cargo-watch` |
| sqlx-cli | Migrations | `cargo install sqlx-cli` |
| Docker | Dependencies | docker.com |

### IDE Setup

**VS Code** (Recommended):
```json
{
  "rust-analyzer.cargo.features": "all",
  "rust-analyzer.checkOnSave.command": "clippy"
}
```

**Extensions:**
- rust-analyzer
- Even Better TOML
- crates

## Project Structure

```
services/chat/
├── Cargo.toml           # Dependencies
├── Cargo.lock           # Lock file
├── .env.example         # Environment template
├── migrations/          # SQL migrations
├── src/
│   ├── main.rs          # Entry point
│   ├── lib.rs           # Library exports
│   ├── api/             # HTTP/WS handlers
│   ├── domain/          # Business logic
│   ├── infrastructure/  # External services
│   ├── server/          # App bootstrap
│   ├── shutdown.rs      # Graceful shutdown
│   ├── tasks.rs         # Background tasks
│   └── telemetry.rs     # Tracing setup
└── tests/
    └── load/            # K6 load tests
```

## Development Workflow

### 1. Start Dependencies

```bash
# From project root
docker compose up -d postgres redis

# Verify
docker compose ps
```

### 2. Configure Environment

```bash
cd services/chat
cp .env.example .env
# Edit .env with your settings
```

### 3. Run Migrations

```bash
# Check current status
sqlx migrate info

# Apply pending migrations
sqlx migrate run
```

### 4. Start Development Server

```bash
# With hot reload (recommended)
cargo watch -x run

# Standard run
cargo run

# With debug logging
RUST_LOG=debug cargo run
```

### 5. Test WebSocket Connection

```bash
# Using websocat
websocat "ws://localhost:8082/ws?token=YOUR_JWT"

# Or use the browser console
# new WebSocket('ws://localhost:8082/ws?token=...')
```

## Code Style

### Formatting

```bash
# Format code
cargo fmt

# Check formatting (CI)
cargo fmt --check
```

### Linting

```bash
# Run clippy
cargo clippy

# With all features
cargo clippy --all-features

# Strict mode (warnings as errors)
cargo clippy -- -D warnings
```

### Conventions

1. **Error Handling**: Use `thiserror` for custom errors
   ```rust
   #[derive(Debug, thiserror::Error)]
   pub enum MyError {
       #[error("Operation failed: {0}")]
       OperationFailed(String),
   }
   ```

2. **Async Functions**: Always use `async fn` with `#[tracing::instrument]`
   ```rust
   #[tracing::instrument(skip(state))]
   pub async fn handler(state: &AppState) -> Result<(), Error> {
       // ...
   }
   ```

3. **Module Organization**: One concept per file
   ```
   domain/message/
   ├── mod.rs         # Re-exports
   ├── handler.rs     # MessageHandler
   ├── storage.rs     # MessageStorage
   ├── router.rs      # MessageRouter
   └── types.rs       # Data structures
   ```

## Testing

### Unit Tests

```bash
# Run all tests
cargo test

# Run specific test
cargo test test_name

# With output
cargo test -- --nocapture

# Single-threaded (for integration tests)
cargo test -- --test-threads=1
```

### Writing Tests

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_message_creation() {
        let storage = MessageStorage::new(pool);
        let message = storage.create_message(...).await.unwrap();
        assert_eq!(message.content, "Hello");
    }
}
```

### Load Testing

```bash
cd tests/load

# WebSocket connection test
k6 run websocket_load.js

# Message throughput test
k6 run --vus 100 --duration 5m message_throughput.js

# With custom target
TARGET=ws://staging:8082 k6 run websocket_load.js
```

## Debugging

### Logging

```bash
# Set log level
RUST_LOG=debug cargo run

# Module-specific logging
RUST_LOG=chat_service::domain::message=trace cargo run

# Structured output
RUST_LOG=info,tower_http=debug cargo run
```

### Tracing

Enable OpenTelemetry for distributed tracing:

```env
CHAT__OTEL__ENABLED=true
CHAT__OTEL__ENDPOINT=http://localhost:4317
```

View traces in Jaeger UI: `http://localhost:16686`

### Database Queries

```bash
# Enable SQL logging
RUST_LOG=sqlx=debug cargo run

# Interactive psql
docker compose exec postgres psql -U ara -d ara_chat

# Common queries
SELECT * FROM messages ORDER BY created_at DESC LIMIT 10;
SELECT * FROM conversations WHERE id = 'uuid';
```

### Redis Debugging

```bash
# Interactive CLI
docker compose exec redis redis-cli

# Monitor commands
MONITOR

# Check keys
KEYS chat:*
GET chat:presence:user-uuid
```

### WebSocket Debugging

1. **Browser DevTools**: Network → WS tab
2. **Postman**: WebSocket request
3. **websocat**: CLI tool
   ```bash
   websocat -v ws://localhost:8082/ws?token=...
   ```

## Common Tasks

### Adding a New Endpoint

1. Define handler in `src/api/rest.rs`:
   ```rust
   pub async fn my_handler(
       State(state): State<AppState>,
       headers: HeaderMap,
   ) -> Result<Json<Response>, (StatusCode, Json<ErrorResponse>)> {
       let user_id = extract_user_id(&headers, &state)?;
       // ...
   }
   ```

2. Add route in `src/api/routes.rs`:
   ```rust
   .route("/api/v1/my-endpoint", get(my_handler))
   ```

### Adding a New WebSocket Message

1. Add variant to `ClientMessage` in `src/domain/message/types.rs`:
   ```rust
   pub enum ClientMessage {
       // ...
       MyNewMessage { field: String },
   }
   ```

2. Handle in `src/api/websocket.rs`:
   ```rust
   ClientMessage::MyNewMessage { field } => {
       handle_my_new_message(user_id, field, state).await;
   }
   ```

### Adding a Migration

```bash
# Create migration file
sqlx migrate add add_my_column

# Edit the SQL file
vim migrations/YYYYMMDD_add_my_column.sql

# Apply
sqlx migrate run
```

## Performance Profiling

### CPU Profiling

```bash
# With flamegraph
cargo install flamegraph
cargo flamegraph --bin chat-service
```

### Memory Profiling

```bash
# With heaptrack
heaptrack ./target/release/chat-service
heaptrack_print heaptrack.chat-service.*.gz
```

### Benchmarking

```bash
# Connection throughput
k6 run --vus 1000 --duration 30s tests/load/websocket_load.js

# Message latency
k6 run --vus 100 --duration 5m tests/load/message_throughput.js
```

## Troubleshooting

### Compilation Errors

```bash
# Clean build
cargo clean && cargo build

# Update dependencies
cargo update
```

### Runtime Panics

1. Enable backtrace: `RUST_BACKTRACE=1 cargo run`
2. Check for unwrap() calls
3. Verify environment variables

### Connection Issues

1. Check service health: `curl http://localhost:8082/health`
2. Verify JWT token validity
3. Check Redis/PostgreSQL connectivity
