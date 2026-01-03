# 進階功能

本文件涵蓋生產環境部署的企業級功能。

## 叢集模式

啟用多實例部署以實現水平擴展。

### 配置

```env
CHAT__CLUSTER__ENABLED=true
CHAT__CLUSTER__SERVER_ID=chat-node-1
CHAT__CLUSTER__SESSION_PREFIX=chat:cluster:sessions
CHAT__CLUSTER__ROUTING_CHANNEL=chat:cluster:route
```

### 架構

```
┌─────────────────────────────────────────────────────────────┐
│                     負載均衡器                              │
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
                    │   (路由)      │
                    └───────────────┘
```

### 訊息路由流程

1. **用戶 A** 連線到 **Chat-1**
2. **用戶 B** 連線到 **Chat-2**
3. **用戶 A** 發送訊息給 **用戶 B**
4. **Chat-1** 檢查本地連線 → 未找到
5. **Chat-1** 發布到 `chat:cluster:route`
6. **Chat-2** 通過訂閱接收
7. **Chat-2** 投遞給 **用戶 B**

### 會話儲存

會話存儲在 Redis 中用於跨實例查詢：

```
chat:cluster:sessions:{user_id} → {server_id}
TTL: 120 秒（心跳時更新）
```

### Docker Compose 叢集

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

## 資料庫分片

使用 Citus 進行十億級部署。

### 配置

```env
CHAT__DATABASE__SHARDING_ENABLED=true
CHAT__DATABASE__SHARD_COUNT=1024
CHAT__DATABASE__COORDINATOR_URL=postgres://citus-coordinator:5432/ara_chat
CHAT__DATABASE__WORKER_NODES=node1=postgres://worker1:5432,node2=postgres://worker2:5432
```

### 分片策略

- **1024 分片**，使用一致性雜湊
- 分片鍵：`user_id`（CRC16 雜湊）
- 分佈：輪詢到各工作節點

### 架構分佈

| 資料表 | 分佈類型 | 分片欄位 |
|--------|----------|----------|
| `messages` | 分散式 | `sender_id` |
| `conversations` | 參考表 | - |
| `conversation_participants` | 分散式 | `user_id` |

### 分片計算

```rust
fn calculate_shard(user_id: Uuid, shard_count: u32) -> u32 {
    let hash = crc16::State::<crc16::XMODEM>::calculate(user_id.as_bytes());
    hash as u32 % shard_count
}
```

---

## 限流

使用 Redis 實現分散式限流。

### 配置

預設：每用戶每 60 秒 60 條訊息。

### 演算法

使用 Redis 的滑動視窗：

```
Key: chat:ratelimit:user:{user_id}
TTL: 60 秒
Value: 訊息計數
```

### 行為

1. 檢查當前計數
2. 如果 `count >= limit` → 拒絕並返回 `RATE_LIMITED` 錯誤
3. 如果允許 → 遞增並繼續

### 限制時回應

```json
{
  "type": "error",
  "code": "RATE_LIMITED",
  "message": "已達速率限制，請 60 秒後重試"
}
```

---

## 熔斷器

外部服務呼叫的容錯機制。

### 狀態

```
┌────────┐     失敗 >= 閾值         ┌────────┐
│  關閉  │ ─────────────────────▶  │  開啟  │
└────────┘                         └────────┘
    ▲                                   │
    │                                   │ 超時
    │         ┌───────────┐             │
    └─────────│  半開啟   │◀────────────┘
   成功       └───────────┘
```

### 配置

| 參數 | 預設值 | 說明 |
|------|--------|------|
| 失敗閾值 | 5 | 連續失敗次數後開啟 |
| 成功閾值 | 2 | 半開啟時成功次數後關閉 |
| 重置超時 | 30 秒 | 再次測試前等待時間 |

### 監控

```promql
# 熔斷器狀態（0=關閉, 1=半開啟, 2=開啟）
chat_circuit_breaker_state{service="redis"}
```

---

## 離線訊息佇列

為斷線用戶儲存訊息。

### 配置

- 最大佇列大小：每用戶 1000 條訊息
- TTL：7 天
- 儲存：Redis 列表

### Redis 結構

```
Key: chat:offline:queue:{user_id}
Type: List（LPUSH/RPOP）
TTL: 604800 秒（7 天）
```

### 流程

1. 用戶斷線
2. 訊息路由到離線佇列
3. 用戶重新連線
4. 佇列排空並投遞訊息
5. 清空佇列

---

## 多租戶模式

隔離不同租戶的資料。

### 配置

租戶 ID 從 JWT 的 `tenant_id` claim 中提取。

### 資料隔離

所有資料庫查詢都包含租戶過濾：

```sql
SELECT * FROM messages
WHERE tenant_id = $1 AND conversation_id = $2
```

### 索引

```sql
CREATE INDEX idx_conversations_tenant
ON conversations (tenant_id, updated_at DESC);

CREATE INDEX idx_messages_tenant
ON messages (tenant_id, conversation_id, created_at DESC);
```

---

## 連線限制

### 全域限制

```env
CHAT__WEBSOCKET__MAX_CONNECTIONS=100000
```

達到限制時，新連線收到：
```json
{
  "type": "error",
  "code": "CONNECTION_LIMIT",
  "message": "已達最大連線數"
}
```

### 每用戶限制

```env
CHAT__WEBSOCKET__MAX_CONNECTIONS_PER_USER=5
```

防止單一用戶佔用過多連線。

---

## 訊息去重

使用 `client_message_id` 進行客戶端去重。

### 流程

1. 客戶端發送前生成唯一 ID
2. 伺服器檢查是否存在相同 ID 的訊息
3. 如果找到 → 返回現有訊息（冪等）
4. 如果未找到 → 建立新訊息

### 實作

```sql
CREATE UNIQUE INDEX idx_messages_dedup
ON messages (sender_id, client_message_id)
WHERE client_message_id IS NOT NULL;
```

### 客戶端使用

```javascript
const clientMessageId = crypto.randomUUID();

ws.send(JSON.stringify({
  type: 'SendMessage',
  payload: {
    conversation_id: convId,
    content: '你好',
    client_message_id: clientMessageId
  }
}));

// 網路故障時可安全重試
// 伺服器將返回相同的 message_id
```

---

## 在線狀態訂閱

訂閱其他用戶的在線狀態。

### 訂閱

```json
{
  "type": "SubscribePresence",
  "payload": {
    "user_ids": ["uuid1", "uuid2", "uuid3"]
  }
}
```

**限制：** 每次請求最多 100 個訂閱

### 初始狀態

伺服器立即發送當前狀態：

```json
{
  "type": "presence",
  "user_id": "uuid1",
  "status": "online",
  "last_seen": null
}
```

### 更新

訂閱用戶狀態變更時自動推送：

```json
{
  "type": "presence",
  "user_id": "uuid1",
  "status": "offline",
  "last_seen": 1704067200000
}
```

### 取消訂閱

```json
{
  "type": "UnsubscribePresence",
  "payload": {
    "user_ids": ["uuid1"]
  }
}
```
