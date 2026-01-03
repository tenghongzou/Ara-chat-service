# API 參考

聊天服務提供 REST API 和 WebSocket 兩種介面。

## 認證

所有端點都需要 JWT 認證。

**REST API：** Authorization 標頭中的 Bearer token
```
Authorization: Bearer <JWT_TOKEN>
```

**WebSocket：** 查詢參數中的 token
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

基礎 URL：`http://localhost:8082`

### 健康端點

#### GET /health
基本健康檢查。

**回應：**
```json
{
  "status": "ok"
}
```

#### GET /health/live
Kubernetes 存活探針。

#### GET /health/ready
Kubernetes 就緒探針（檢查 Redis、PostgreSQL）。

#### GET /metrics
Prometheus 指標端點。

---

### 對話

#### GET /api/v1/conversations
列出用戶的對話（分頁）。

**查詢參數：**
| 參數 | 類型 | 預設 | 說明 |
|------|------|------|------|
| `before` | string | - | 分頁游標（時間戳） |
| `limit` | integer | 20 | 每頁結果數（最大：50） |

**回應：**
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
        "content_preview": "你好！",
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
建立新對話。

**請求內容：**
```json
{
  "conversation_type": "group",
  "participants": ["uuid1", "uuid2"],
  "name": "團隊群組"
}
```

**回應：** `ConversationSummary`

#### GET /api/v1/conversations/{id}
取得對話詳情。

**回應：** `ConversationSummary`

---

### 訊息

#### GET /api/v1/conversations/{id}/messages
取得訊息歷史（游標分頁）。

**查詢參數：**
| 參數 | 類型 | 預設 | 說明 |
|------|------|------|------|
| `before` | UUID | - | 訊息 ID 游標 |
| `limit` | integer | 50 | 每頁結果數（最大：100） |

**回應：**
```json
{
  "messages": [
    {
      "id": "uuid",
      "conversation_id": "uuid",
      "sender_id": "uuid",
      "content": "你好！",
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
發送訊息。

**請求內容：**
```json
{
  "content": "你好！",
  "content_type": "text",
  "reply_to": null,
  "mentions": [],
  "client_message_id": "unique-client-id"
}
```

**回應：**
```json
{
  "id": "uuid",
  "conversation_id": "uuid",
  "created_at": 1704067200000,
  "client_message_id": "unique-client-id"
}
```

#### POST /api/v1/conversations/{id}/read
標記訊息為已讀。

**請求內容：**
```json
{
  "message_id": "uuid"
}
```

---

### 未讀計數

#### GET /api/v1/unread
取得所有對話的未讀計數。

**回應：**
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

### 搜尋

#### GET /api/v1/search/messages
搜尋用戶對話中的訊息。

**查詢參數：**
| 參數 | 類型 | 必要 | 說明 |
|------|------|------|------|
| `q` | string | 是 | 搜尋關鍵字（最少 2 字元） |
| `conversation_id` | UUID | 否 | 限定特定對話 |
| `limit` | integer | 否 | 每頁結果數（最大：50） |

**回應：**
```json
{
  "messages": [
    {
      "id": "uuid",
      "conversation_id": "uuid",
      "sender_id": "uuid",
      "content_preview": "你好世界...",
      "created_at": 1704067200000,
      "highlight": "你好<mark>世界</mark>"
    }
  ],
  "total_count": 42
}
```

---

## WebSocket 協議

### 連線

```
ws://localhost:8082/ws?token=<JWT_TOKEN>
```

連線成功後，伺服器發送：
```json
{
  "type": "authenticated",
  "user_id": "uuid"
}
```

### 客戶端 → 伺服器訊息

#### Ping（保活）
```json
{
  "type": "Ping"
}
```

#### SendMessage（發送訊息）
```json
{
  "type": "SendMessage",
  "payload": {
    "conversation_id": "uuid",
    "content": "你好！",
    "content_type": "text",
    "reply_to": null,
    "client_message_id": "unique-id",
    "mentions": ["user-uuid"]
  }
}
```

#### MarkRead（標記已讀）
```json
{
  "type": "MarkRead",
  "payload": {
    "conversation_id": "uuid",
    "message_id": "uuid"
  }
}
```

#### RecallMessage（撤回訊息）
```json
{
  "type": "RecallMessage",
  "payload": {
    "conversation_id": "uuid",
    "message_id": "uuid"
  }
}
```

#### EditMessage（編輯訊息）
```json
{
  "type": "EditMessage",
  "payload": {
    "conversation_id": "uuid",
    "message_id": "uuid",
    "new_content": "更新的內容"
  }
}
```

#### ToggleReaction（切換表情反應）
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

#### Typing（輸入中）
```json
{
  "type": "Typing",
  "payload": {
    "conversation_id": "uuid",
    "is_typing": true
  }
}
```

#### CreateConversation（建立對話）
```json
{
  "type": "CreateConversation",
  "payload": {
    "conversation_type": "group",
    "participants": ["uuid1", "uuid2"],
    "name": "團隊群組"
  }
}
```

#### UpdatePresence（更新在線狀態）
```json
{
  "type": "UpdatePresence",
  "payload": {
    "status": "away"
  }
}
```

#### SubscribePresence（訂閱在線狀態）
```json
{
  "type": "SubscribePresence",
  "payload": {
    "user_ids": ["uuid1", "uuid2"]
  }
}
```

#### SyncUnread（同步未讀）
```json
{
  "type": "SyncUnread"
}
```

---

### 伺服器 → 客戶端訊息

#### pong
```json
{
  "type": "pong"
}
```

#### message（新訊息）
```json
{
  "type": "message",
  "message": {
    "id": "uuid",
    "conversation_id": "uuid",
    "sender_id": "uuid",
    "content": "你好！",
    "content_type": "text",
    "created_at": 1704067200000,
    "mentions": [],
    "reactions": {}
  }
}
```

#### message_sent（訊息已發送）
```json
{
  "type": "message_sent",
  "conversation_id": "uuid",
  "message_id": "uuid",
  "client_message_id": "unique-id",
  "created_at": 1704067200000
}
```

#### read_receipt（已讀回執）
```json
{
  "type": "read_receipt",
  "conversation_id": "uuid",
  "user_id": "uuid",
  "message_id": "uuid",
  "read_at": 1704067200000
}
```

#### typing（輸入中）
```json
{
  "type": "typing",
  "conversation_id": "uuid",
  "user_id": "uuid",
  "is_typing": true
}
```

#### presence（在線狀態）
```json
{
  "type": "presence",
  "user_id": "uuid",
  "status": "online",
  "last_seen": null
}
```

#### error（錯誤）
```json
{
  "type": "error",
  "code": "RATE_LIMITED",
  "message": "已達速率限制，請 60 秒後重試"
}
```

---

## 錯誤碼

| 錯誤碼 | HTTP 狀態碼 | 說明 |
|--------|-------------|------|
| `UNAUTHORIZED` | 401 | 缺少或無效的 token |
| `INVALID_TOKEN` | 401 | Token 驗證失敗 |
| `NOT_PARTICIPANT` | 403 | 用戶不在對話中 |
| `NOT_FOUND` | 404 | 資源未找到 |
| `RATE_LIMITED` | 429 | 已達速率限制 |
| `SERVICE_UNAVAILABLE` | 503 | 內部服務錯誤 |
| `SEND_FAILED` | 500 | 訊息發送失敗 |
| `RECALL_FAILED` | 400 | 訊息撤回失敗 |
| `EDIT_FAILED` | 400 | 訊息編輯失敗 |

---

## 資料類型

### ContentType（內容類型）
```
text | image | file | system
```

### ConversationType（對話類型）
```
direct | group
```

### PresenceStatus（在線狀態）
```
online | away | busy | offline
```

### ParticipantRole（參與者角色）
```
owner | admin | member
```

### ReactionAction（反應動作）
```
add | remove
```
