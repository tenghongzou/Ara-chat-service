# Architecture

The Ara Chat Service follows Clean Architecture principles with clear separation between layers.

## System Overview

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

## Layer Architecture

```
src/
├── api/                    # Presentation Layer
│   ├── health.rs          # Health check endpoints
│   ├── rest.rs            # REST API handlers
│   ├── routes.rs          # Router configuration
│   └── websocket.rs       # WebSocket handler
│
├── domain/                 # Business Logic Layer
│   ├── cluster/           # Cross-instance routing
│   ├── connection/        # WebSocket connection management
│   ├── conversation/      # Conversation CRUD
│   ├── mention/           # @mention parsing
│   ├── message/           # Message handling & storage
│   ├── presence/          # Online/offline tracking
│   ├── reaction/          # Emoji reactions
│   └── receipt/           # Read receipts
│
├── infrastructure/         # External Dependencies
│   ├── auth/              # JWT validation
│   ├── config/            # Settings management
│   ├── postgres/          # Database pools
│   ├── redis/             # Cache & Pub/Sub
│   ├── ratelimit/         # Rate limiting
│   ├── sharding/          # User sharding
│   ├── circuit_breaker.rs # Fault tolerance
│   └── metrics/           # Prometheus
│
└── server/                 # Application Bootstrap
    ├── mod.rs             # Router setup
    └── state.rs           # AppState initialization
```

## Domain Modules

### Connection Manager
Lock-free connection tracking using DashMap.

**Features:**
- Per-user connection limits (default: 5)
- Global connection limit (default: 100K)
- SmallVec optimization for typical usage (1-2 connections per user)

```rust
// Connection registration flow
ConnectionManager::register(connection)
    -> Check global limit
    -> Check per-user limit
    -> Store in DashMap<UserId, SmallVec<Connection>>
```

### Message Router
Routes messages to recipients across instances.

**Routing Strategies:**
1. **Local Delivery**: Direct channel if user has active connection
2. **Cluster Routing**: Redis Pub/Sub to other instances
3. **Offline Queue**: Store in Redis if user not online (7-day TTL)

```
┌──────────────────────────────────────────────────────────────┐
│                      Message Router                          │
├──────────────────────────────────────────────────────────────┤
│  1. Check local connections (DashMap lookup)                 │
│     ↓ Found → Send via mpsc channel                          │
│                                                              │
│  2. Check cluster session store (Redis)                      │
│     ↓ Found → Publish to chat:cluster:route                  │
│                                                              │
│  3. Queue for offline delivery                               │
│     ↓ Store in Redis list with TTL                           │
└──────────────────────────────────────────────────────────────┘
```

### Conversation Service
Manages conversation lifecycle with O(1) direct message lookups.

**Optimization:**
- SHA256 hash of sorted user IDs for DM lookup
- Eliminates "find conversation by participants" queries

### Presence Tracker
Redis-backed online status with subscription model.

**Data Flow:**
1. User connects → Mark online (Redis SET with TTL)
2. Subscribers notified via Pub/Sub
3. User disconnects → Check remaining connections → Mark offline

## Database Schema

### Tables
| Table | Purpose | Partitioning |
|-------|---------|--------------|
| `conversations` | Metadata | None |
| `conversation_participants` | Membership | None |
| `direct_message_lookup` | O(1) DM lookup | None |
| `messages` | Chat messages | By date (pg_partman) |
| `message_reactions` | Emoji reactions | By date |
| `read_receipts` | Read status | None |

### Indexes
- `idx_messages_conversation`: (conversation_id, created_at DESC)
- `idx_messages_mentions`: GIN(mentions) for @mention queries
- `idx_messages_dedup`: UNIQUE(sender_id, client_message_id)

## Scaling Considerations

### Horizontal Scaling
- Stateless pods behind load balancer
- Redis Pub/Sub for cross-instance messaging
- Sticky sessions not required (cluster routing handles it)

### Vertical Scaling
- Connection pooling (adaptive sizing)
- Async I/O throughout (Tokio runtime)
- Lock-free data structures (DashMap)

### Data Sharding
- 1024 user shards (consistent hashing)
- Citus-compatible schema for billion-scale
- Date-partitioned message tables
