# Advanced Features

This document covers enterprise-grade features for production deployments.

## Cluster Mode

Enable multi-instance deployment for horizontal scaling.

### Configuration

```env
CHAT__CLUSTER__ENABLED=true
CHAT__CLUSTER__SERVER_ID=chat-node-1
CHAT__CLUSTER__SESSION_PREFIX=chat:cluster:sessions
CHAT__CLUSTER__ROUTING_CHANNEL=chat:cluster:route
```

### Architecture

```
┌─────────────────────────────────────────────────────────────┐
│                     Load Balancer                           │
└───────────────────────────┬─────────────────────────────────┘
                            │
        ┌───────────────────┼───────────────────┐
        ▼                   ▼                   ▼
   ┌─────────┐         ┌─────────┐         ┌─────────┐
   │ Chat-1  │         │ Chat-2  │         │ Chat-3  │
   │ :8082   │         │ :8083   │         │ :8084   │
   └────┬────┘         └────┬────┘         └────┬────┘
        │                   │                   │
        └───────────────────┼───────────────────┘
                            │
                    ┌───────▼───────┐
                    │ Redis Pub/Sub │
                    │  (Routing)    │
                    └───────────────┘
```

### Message Routing Flow

1. **User A** connects to **Chat-1**
2. **User B** connects to **Chat-2**
3. **User A** sends message to **User B**
4. **Chat-1** checks local connections → not found
5. **Chat-1** publishes to `chat:cluster:route`
6. **Chat-2** receives via subscription
7. **Chat-2** delivers to **User B**

### Session Store

Sessions are stored in Redis for cross-instance lookup:

```
chat:cluster:sessions:{user_id} → {server_id}
TTL: 120 seconds (refreshed on heartbeat)
```

### Docker Compose Cluster

```yaml
# docker-compose.cluster.yml
services:
  chat-1:
    environment:
      CHAT__CLUSTER__ENABLED: "true"
      CHAT__CLUSTER__SERVER_ID: "chat-1"
    ports:
      - "8082:8082"

  chat-2:
    environment:
      CHAT__CLUSTER__ENABLED: "true"
      CHAT__CLUSTER__SERVER_ID: "chat-2"
    ports:
      - "8083:8082"

  chat-3:
    environment:
      CHAT__CLUSTER__ENABLED: "true"
      CHAT__CLUSTER__SERVER_ID: "chat-3"
    ports:
      - "8084:8082"
```

---

## Database Sharding

For billion-scale deployments using Citus.

### Configuration

```env
CHAT__DATABASE__SHARDING_ENABLED=true
CHAT__DATABASE__SHARD_COUNT=1024
CHAT__DATABASE__COORDINATOR_URL=postgres://citus-coordinator:5432/ara_chat
CHAT__DATABASE__WORKER_NODES=node1=postgres://worker1:5432,node2=postgres://worker2:5432
```

### Sharding Strategy

- **1024 shards** with consistent hashing
- Shard key: `user_id` (CRC16 hash)
- Distribution: Round-robin across worker nodes

### Schema Distribution

| Table | Distribution | Shard Column |
|-------|--------------|--------------|
| `messages` | Distributed | `sender_id` |
| `conversations` | Reference | - |
| `conversation_participants` | Distributed | `user_id` |

### Shard Calculation

```rust
fn calculate_shard(user_id: Uuid, shard_count: u32) -> u32 {
    let hash = crc16::State::<crc16::XMODEM>::calculate(user_id.as_bytes());
    hash as u32 % shard_count
}
```

---

## Rate Limiting

Distributed rate limiting using Redis.

### Configuration

Default: 60 messages per 60 seconds per user.

### Algorithm

Sliding window with Redis:

```
Key: chat:ratelimit:user:{user_id}
TTL: 60 seconds
Value: message count
```

### Behavior

1. Check current count
2. If `count >= limit` → reject with `RATE_LIMITED` error
3. If allowed → increment and proceed

### Response on Limit

```json
{
  "type": "error",
  "code": "RATE_LIMITED",
  "message": "Rate limit exceeded. Retry after 60 seconds"
}
```

### Customization

```rust
// In rate limiter configuration
RateLimiter::new(redis_pool)
    .with_limit(100)           // 100 messages
    .with_window(Duration::from_secs(60))  // per 60 seconds
```

---

## Circuit Breaker

Fault tolerance for external service calls.

### States

```
┌────────┐     failures >= threshold     ┌────────┐
│ CLOSED │ ─────────────────────────────▶│  OPEN  │
└────────┘                               └────────┘
    ▲                                         │
    │                                         │ timeout
    │         ┌───────────┐                   │
    └─────────│ HALF-OPEN │◀──────────────────┘
   success    └───────────┘
```

### Configuration

| Parameter | Default | Description |
|-----------|---------|-------------|
| Failure Threshold | 5 | Consecutive failures to open |
| Success Threshold | 2 | Successes in half-open to close |
| Reset Timeout | 30s | Time before testing again |

### Usage

```rust
let breaker = CircuitBreaker::new()
    .with_failure_threshold(5)
    .with_success_threshold(2)
    .with_reset_timeout(Duration::from_secs(30));

match breaker.call(|| async { redis.get(key).await }).await {
    Ok(result) => { /* success */ }
    Err(CircuitBreakerError::Open) => { /* circuit is open */ }
    Err(CircuitBreakerError::ServiceError(e)) => { /* service failed */ }
}
```

### Monitoring

```promql
# Circuit breaker state (0=closed, 1=half-open, 2=open)
chat_circuit_breaker_state{service="redis"}
```

---

## Offline Message Queue

Store messages for disconnected users.

### Configuration

- Maximum queue size: 1000 messages per user
- TTL: 7 days
- Storage: Redis list

### Redis Structure

```
Key: chat:offline:queue:{user_id}
Type: List (LPUSH/RPOP)
TTL: 604800 seconds (7 days)
```

### Flow

1. User disconnects
2. Messages routed to offline queue
3. User reconnects
4. Queue drained and messages delivered
5. Queue cleared

### Queue Management

```rust
// Store message for offline user
offline_queue.push(user_id, message).await?;

// Drain on reconnection
let messages = offline_queue.drain_messages(user_id).await?;
for msg in messages {
    connection.send(msg).await?;
}
```

---

## Multi-Tenant Mode

Isolate data between tenants.

### Configuration

Tenant ID is extracted from JWT `tenant_id` claim.

### Data Isolation

All database queries include tenant filter:

```sql
SELECT * FROM messages
WHERE tenant_id = $1 AND conversation_id = $2
```

### Indexes

```sql
CREATE INDEX idx_conversations_tenant
ON conversations (tenant_id, updated_at DESC);

CREATE INDEX idx_messages_tenant
ON messages (tenant_id, conversation_id, created_at DESC);
```

### Cross-Tenant Prevention

- Users can only access conversations within their tenant
- Messages are scoped to tenant
- Participant validation includes tenant check

---

## Connection Limits

### Global Limit

```env
CHAT__WEBSOCKET__MAX_CONNECTIONS=100000
```

When reached, new connections receive:
```json
{
  "type": "error",
  "code": "CONNECTION_LIMIT",
  "message": "Maximum connections reached"
}
```

### Per-User Limit

```env
CHAT__WEBSOCKET__MAX_CONNECTIONS_PER_USER=5
```

Prevents single user from consuming too many connections.

### Implementation

```rust
// DashMap for O(1) lookup
connections: DashMap<UserId, SmallVec<[Connection; 4]>>

// SmallVec optimization: most users have 1-2 connections
// Avoids heap allocation for typical case
```

---

## Message Deduplication

Client-side deduplication using `client_message_id`.

### Flow

1. Client generates unique ID before sending
2. Server checks for existing message with same ID
3. If found → return existing message (idempotent)
4. If not found → create new message

### Implementation

```sql
CREATE UNIQUE INDEX idx_messages_dedup
ON messages (sender_id, client_message_id)
WHERE client_message_id IS NOT NULL;
```

### Client Usage

```javascript
const clientMessageId = crypto.randomUUID();

ws.send(JSON.stringify({
  type: 'SendMessage',
  payload: {
    conversation_id: convId,
    content: 'Hello',
    client_message_id: clientMessageId
  }
}));

// Safe to retry on network failure
// Server will return same message_id
```

---

## Presence Subscriptions

Subscribe to other users' online status.

### Subscribe

```json
{
  "type": "SubscribePresence",
  "payload": {
    "user_ids": ["uuid1", "uuid2", "uuid3"]
  }
}
```

**Limit:** 100 subscriptions per request

### Initial Status

Server immediately sends current status:

```json
{
  "type": "presence",
  "user_id": "uuid1",
  "status": "online",
  "last_seen": null
}
```

### Updates

Automatic push when subscribed users change status:

```json
{
  "type": "presence",
  "user_id": "uuid1",
  "status": "offline",
  "last_seen": 1704067200000
}
```

### Unsubscribe

```json
{
  "type": "UnsubscribePresence",
  "payload": {
    "user_ids": ["uuid1"]
  }
}
```
