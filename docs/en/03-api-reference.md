# API Reference

The Chat Service provides both REST API and WebSocket interfaces.

## Authentication

All endpoints require JWT authentication.

**REST API:** Bearer token in Authorization header
```
Authorization: Bearer <JWT_TOKEN>
```

**WebSocket:** Token in query parameter
```
ws://localhost:8082/ws?token=<JWT_TOKEN>
```

### JWT Claims

```json
{
  "sub": "user-uuid",
  "exp": 1704067200,
  "iat": 1703980800,
  "iss": "ara-services",
  "aud": "ara-services",
  "tenant_id": "default"
}
```

---

## REST API

Base URL: `http://localhost:8082`

### Health Endpoints

#### GET /health
Basic health check.

**Response:**
```json
{
  "status": "ok"
}
```

#### GET /health/live
Kubernetes liveness probe.

#### GET /health/ready
Kubernetes readiness probe (checks Redis, PostgreSQL).

#### GET /metrics
Prometheus metrics endpoint.

---

### Conversations

#### GET /api/v1/conversations
List user's conversations with pagination.

**Query Parameters:**
| Parameter | Type | Default | Description |
|-----------|------|---------|-------------|
| `before` | string | - | Cursor for pagination (timestamp) |
| `limit` | integer | 20 | Results per page (max: 50) |

**Response:**
```json
{
  "conversations": [
    {
      "id": "uuid",
      "conversation_type": "direct",
      "name": null,
      "avatar_url": null,
      "participant_count": 2,
      "participants": [],
      "last_message": {
        "message_id": "uuid",
        "sender_id": "uuid",
        "content_preview": "Hello!",
        "content_type": "text",
        "created_at": 1704067200000
      },
      "unread_count": 5,
      "updated_at": 1704067200000
    }
  ],
  "has_more": true
}
```

#### POST /api/v1/conversations
Create a new conversation.

**Request Body:**
```json
{
  "conversation_type": "group",
  "participants": ["uuid1", "uuid2"],
  "name": "Team Chat"
}
```

**Response:** `ConversationSummary`

#### GET /api/v1/conversations/{id}
Get conversation details.

**Response:** `ConversationSummary`

---

### Messages

#### GET /api/v1/conversations/{id}/messages
Get message history with cursor-based pagination.

**Query Parameters:**
| Parameter | Type | Default | Description |
|-----------|------|---------|-------------|
| `before` | UUID | - | Message ID cursor |
| `limit` | integer | 50 | Results per page (max: 100) |

**Response:**
```json
{
  "messages": [
    {
      "id": "uuid",
      "conversation_id": "uuid",
      "sender_id": "uuid",
      "content": "Hello!",
      "content_type": "text",
      "created_at": 1704067200000,
      "updated_at": null,
      "reply_to_id": null,
      "mentions": [],
      "reactions": {},
      "recalled_at": null
    }
  ],
  "has_more": true
}
```

#### POST /api/v1/conversations/{id}/messages
Send a message.

**Request Body:**
```json
{
  "content": "Hello!",
  "content_type": "text",
  "reply_to": null,
  "mentions": [],
  "client_message_id": "unique-client-id"
}
```

**Response:**
```json
{
  "id": "uuid",
  "conversation_id": "uuid",
  "created_at": 1704067200000,
  "client_message_id": "unique-client-id"
}
```

#### POST /api/v1/conversations/{id}/read
Mark messages as read.

**Request Body:**
```json
{
  "message_id": "uuid"
}
```

---

### Unread Counts

#### GET /api/v1/unread
Get unread counts for all conversations.

**Response:**
```json
{
  "total": 15,
  "per_conversation": {
    "conv-uuid-1": 10,
    "conv-uuid-2": 5
  }
}
```

---

### Search

#### GET /api/v1/search/messages
Search messages across user's conversations.

**Query Parameters:**
| Parameter | Type | Required | Description |
|-----------|------|----------|-------------|
| `q` | string | Yes | Search query (min 2 chars) |
| `conversation_id` | UUID | No | Limit to specific conversation |
| `limit` | integer | No | Results per page (max: 50) |

**Response:**
```json
{
  "messages": [
    {
      "id": "uuid",
      "conversation_id": "uuid",
      "sender_id": "uuid",
      "content_preview": "Hello world...",
      "created_at": 1704067200000,
      "highlight": "Hello <mark>world</mark>"
    }
  ],
  "total_count": 42
}
```

---

## WebSocket Protocol

### Connection

```
ws://localhost:8082/ws?token=<JWT_TOKEN>
```

On successful connection, server sends:
```json
{
  "type": "authenticated",
  "user_id": "uuid"
}
```

### Client → Server Messages

#### Ping (Keepalive)
```json
{
  "type": "Ping"
}
```

#### SendMessage
```json
{
  "type": "SendMessage",
  "payload": {
    "conversation_id": "uuid",
    "content": "Hello!",
    "content_type": "text",
    "reply_to": null,
    "client_message_id": "unique-id",
    "mentions": ["user-uuid"]
  }
}
```

#### MarkRead
```json
{
  "type": "MarkRead",
  "payload": {
    "conversation_id": "uuid",
    "message_id": "uuid"
  }
}
```

#### RecallMessage
```json
{
  "type": "RecallMessage",
  "payload": {
    "conversation_id": "uuid",
    "message_id": "uuid"
  }
}
```

#### EditMessage
```json
{
  "type": "EditMessage",
  "payload": {
    "conversation_id": "uuid",
    "message_id": "uuid",
    "new_content": "Updated content"
  }
}
```

#### ToggleReaction
```json
{
  "type": "ToggleReaction",
  "payload": {
    "conversation_id": "uuid",
    "message_id": "uuid",
    "emoji": "👍"
  }
}
```

#### Typing
```json
{
  "type": "Typing",
  "payload": {
    "conversation_id": "uuid",
    "is_typing": true
  }
}
```

#### FetchHistory
```json
{
  "type": "FetchHistory",
  "payload": {
    "conversation_id": "uuid",
    "before": "message-uuid",
    "limit": 50
  }
}
```

#### FetchConversations
```json
{
  "type": "FetchConversations",
  "payload": {
    "before": 1704067200000,
    "limit": 20
  }
}
```

#### CreateConversation
```json
{
  "type": "CreateConversation",
  "payload": {
    "conversation_type": "group",
    "participants": ["uuid1", "uuid2"],
    "name": "Team Chat"
  }
}
```

#### UpdatePresence
```json
{
  "type": "UpdatePresence",
  "payload": {
    "status": "away"
  }
}
```

#### SubscribePresence
```json
{
  "type": "SubscribePresence",
  "payload": {
    "user_ids": ["uuid1", "uuid2"]
  }
}
```

#### SyncUnread
```json
{
  "type": "SyncUnread"
}
```

#### GetReactions
```json
{
  "type": "GetReactions",
  "payload": {
    "message_ids": ["uuid1", "uuid2"]
  }
}
```

---

### Server → Client Messages

#### pong
```json
{
  "type": "pong"
}
```

#### message
```json
{
  "type": "message",
  "message": {
    "id": "uuid",
    "conversation_id": "uuid",
    "sender_id": "uuid",
    "content": "Hello!",
    "content_type": "text",
    "created_at": 1704067200000,
    "mentions": [],
    "reactions": {}
  }
}
```

#### message_sent
```json
{
  "type": "message_sent",
  "conversation_id": "uuid",
  "message_id": "uuid",
  "client_message_id": "unique-id",
  "created_at": 1704067200000
}
```

#### read_receipt
```json
{
  "type": "read_receipt",
  "conversation_id": "uuid",
  "user_id": "uuid",
  "message_id": "uuid",
  "read_at": 1704067200000
}
```

#### message_recalled
```json
{
  "type": "message_recalled",
  "conversation_id": "uuid",
  "message_id": "uuid",
  "recalled_by": "uuid"
}
```

#### message_edited
```json
{
  "type": "message_edited",
  "conversation_id": "uuid",
  "message_id": "uuid",
  "new_content": "Updated content",
  "edited_at": 1704067200000,
  "mentions": []
}
```

#### reaction_update
```json
{
  "type": "reaction_update",
  "conversation_id": "uuid",
  "message_id": "uuid",
  "user_id": "uuid",
  "emoji": "👍",
  "action": "add"
}
```

#### typing
```json
{
  "type": "typing",
  "conversation_id": "uuid",
  "user_id": "uuid",
  "is_typing": true
}
```

#### presence
```json
{
  "type": "presence",
  "user_id": "uuid",
  "status": "online",
  "last_seen": null
}
```

#### unread_sync
```json
{
  "type": "unread_sync",
  "total": 15,
  "per_conversation": {
    "uuid1": 10,
    "uuid2": 5
  }
}
```

#### error
```json
{
  "type": "error",
  "code": "RATE_LIMITED",
  "message": "Rate limit exceeded. Retry after 60 seconds"
}
```

#### shutdown
```json
{
  "type": "shutdown",
  "reason": "Server maintenance",
  "reconnect_after_seconds": 30
}
```

---

## Error Codes

| Code | HTTP Status | Description |
|------|-------------|-------------|
| `UNAUTHORIZED` | 401 | Missing or invalid token |
| `INVALID_TOKEN` | 401 | Token validation failed |
| `NOT_PARTICIPANT` | 403 | User not in conversation |
| `NOT_FOUND` | 404 | Resource not found |
| `RATE_LIMITED` | 429 | Rate limit exceeded |
| `SERVICE_UNAVAILABLE` | 503 | Internal service error |
| `SEND_FAILED` | 500 | Message send failed |
| `RECALL_FAILED` | 400 | Message recall failed |
| `EDIT_FAILED` | 400 | Message edit failed |

---

## Data Types

### ContentType
```
text | image | file | system
```

### ConversationType
```
direct | group
```

### PresenceStatus
```
online | away | busy | offline
```

### ParticipantRole
```
owner | admin | member
```

### ReactionAction
```
add | remove
```
