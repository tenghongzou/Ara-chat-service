# 可觀測性

本文件涵蓋監控、指標、追蹤和健康檢查。

## 健康檢查

### 端點

| 端點 | 用途 | 檢查項目 |
|------|------|----------|
| `/health` | 基本狀態 | 服務運行中 |
| `/health/live` | 存活探針 | 程序存活 |
| `/health/ready` | 就緒探針 | Redis、PostgreSQL、連線池 |
| `/health/detailed` | 完整報告 | 所有元件 |

### Kubernetes 整合

```yaml
apiVersion: apps/v1
kind: Deployment
spec:
  template:
    spec:
      containers:
        - name: chat
          livenessProbe:
            httpGet:
              path: /health/live
              port: 8082
            initialDelaySeconds: 10
            periodSeconds: 10
          readinessProbe:
            httpGet:
              path: /health/ready
              port: 8082
            initialDelaySeconds: 5
            periodSeconds: 5
```

### 回應格式

```json
{
  "status": "healthy",
  "version": "1.0.0",
  "uptime_seconds": 3600,
  "checks": {
    "database": {
      "status": "healthy",
      "latency_ms": 2
    },
    "redis": {
      "status": "healthy",
      "latency_ms": 1
    },
    "connection_pool": {
      "status": "healthy",
      "active": 1500,
      "max": 100000
    }
  }
}
```

---

## Prometheus 指標

### 端點

```
GET /metrics
```

### 可用指標

#### 連線指標

| 指標 | 類型 | 說明 |
|------|------|------|
| `chat_websocket_connections_total` | Gauge | 活躍 WebSocket 連線數 |
| `chat_websocket_connections_by_user` | Gauge | 每用戶連線數 |
| `chat_connection_duration_seconds` | Histogram | 連線持續時間 |

#### 訊息指標

| 指標 | 類型 | 說明 |
|------|------|------|
| `chat_messages_sent_total` | Counter | 發送訊息總數 |
| `chat_messages_received_total` | Counter | 接收訊息總數 |
| `chat_message_processing_duration_seconds` | Histogram | 處理延遲 |
| `chat_message_size_bytes` | Histogram | 訊息大小 |

#### 系統指標

| 指標 | 類型 | 說明 |
|------|------|------|
| `chat_db_pool_size` | Gauge | 資料庫連線池使用率 |
| `chat_redis_pool_size` | Gauge | Redis 連線池使用率 |
| `chat_circuit_breaker_state` | Gauge | 熔斷器狀態 |
| `chat_rate_limit_rejections_total` | Counter | 限流拒絕次數 |

### 範例查詢

```promql
# 活躍連線數
chat_websocket_connections_total

# 訊息吞吐量（每秒）
rate(chat_messages_sent_total[5m])

# P95 訊息延遲
histogram_quantile(0.95,
  rate(chat_message_processing_duration_seconds_bucket[5m])
)

# 熔斷器狀態（0=關閉, 1=半開啟, 2=開啟）
chat_circuit_breaker_state{service="redis"}

# 資料庫連線池使用率
chat_db_pool_size{state="active"} / chat_db_pool_size{state="max"}
```

### Grafana 儀表板

```json
{
  "panels": [
    {
      "title": "活躍連線",
      "type": "stat",
      "targets": [{
        "expr": "chat_websocket_connections_total"
      }]
    },
    {
      "title": "訊息吞吐量",
      "type": "graph",
      "targets": [{
        "expr": "rate(chat_messages_sent_total[5m])",
        "legendFormat": "訊息/秒"
      }]
    },
    {
      "title": "P95 延遲",
      "type": "graph",
      "targets": [{
        "expr": "histogram_quantile(0.95, rate(chat_message_processing_duration_seconds_bucket[5m]))",
        "legendFormat": "P95"
      }]
    }
  ]
}
```

---

## OpenTelemetry 追蹤

### 配置

```env
CHAT__OTEL__ENABLED=true
CHAT__OTEL__ENDPOINT=http://localhost:4317
CHAT__OTEL__SERVICE_NAME=ara-chat-service
```

### 追蹤上下文

追蹤傳播通過：
- HTTP 標頭（W3C Trace Context）
- Redis Pub/Sub 訊息
- 資料庫查詢

### Spans

| Span 名稱 | 屬性 |
|-----------|------|
| `websocket.handle` | `user_id`, `connection_id` |
| `message.send` | `conversation_id`, `content_type` |
| `message.route` | `target_user_id`, `delivery_method` |
| `db.query` | `query_name`, `duration_ms` |
| `redis.command` | `command`, `key_prefix` |

### Jaeger 整合

```yaml
# docker-compose.yml
services:
  jaeger:
    image: jaegertracing/all-in-one:latest
    ports:
      - "16686:16686"  # UI
      - "4317:4317"    # OTLP gRPC
```

在 `http://localhost:16686` 檢視追蹤

### 追蹤範例

```
chat.websocket.handle (150ms)
├── auth.jwt.validate (2ms)
├── connection.register (5ms)
├── presence.mark_online (10ms)
│   └── redis.set (3ms)
└── offline_queue.drain (30ms)
    ├── redis.lrange (5ms)
    └── message.deliver (25ms)
```

---

## 日誌

### 配置

```env
RUST_LOG=info
# 或模組特定
RUST_LOG=chat_service=debug,sqlx=warn,tower_http=info
```

### 日誌級別

| 級別 | 用途 |
|------|------|
| `error` | 需要關注的故障 |
| `warn` | 可恢復的問題 |
| `info` | 關鍵操作 |
| `debug` | 詳細流程 |
| `trace` | 全部記錄 |

### 結構化日誌

```rust
tracing::info!(
    user_id = %user_id,
    conversation_id = %conversation_id,
    content_len = content.len(),
    "訊息已發送"
);
```

輸出：
```json
{
  "timestamp": "2026-01-03T12:00:00Z",
  "level": "INFO",
  "target": "chat_service::message::handler",
  "message": "訊息已發送",
  "user_id": "uuid",
  "conversation_id": "uuid",
  "content_len": 42
}
```

---

## 告警規則

### Prometheus AlertManager

```yaml
groups:
  - name: chat-alerts
    rules:
      - alert: HighConnectionCount
        expr: chat_websocket_connections_total > 90000
        for: 5m
        labels:
          severity: warning
        annotations:
          summary: "{{ $labels.instance }} 連線數過高"
          description: "連線數為 {{ $value }}，接近上限"

      - alert: CircuitBreakerOpen
        expr: chat_circuit_breaker_state == 2
        for: 1m
        labels:
          severity: critical
        annotations:
          summary: "{{ $labels.service }} 熔斷器已開啟"

      - alert: HighMessageLatency
        expr: |
          histogram_quantile(0.95,
            rate(chat_message_processing_duration_seconds_bucket[5m])
          ) > 0.5
        for: 5m
        labels:
          severity: warning
        annotations:
          summary: "訊息延遲過高"
          description: "P95 延遲為 {{ $value }} 秒"

      - alert: DatabasePoolExhausted
        expr: |
          chat_db_pool_size{state="active"} /
          chat_db_pool_size{state="max"} > 0.9
        for: 5m
        labels:
          severity: critical
        annotations:
          summary: "資料庫連線池即將耗盡"

      - alert: RateLimitExceeded
        expr: rate(chat_rate_limit_rejections_total[5m]) > 100
        for: 1m
        labels:
          severity: warning
        annotations:
          summary: "限流拒絕次數過多"
```

---

## 效能目標

| 指標 | 目標 | 告警閾值 |
|------|------|----------|
| 連線成功率 | > 99% | < 95% |
| 訊息延遲（P95） | < 100ms | > 500ms |
| 訊息吞吐量 | > 10 萬/秒 | - |
| 錯誤率 | < 0.1% | > 1% |
| 連線池使用率 | < 80% | > 90% |

---

## 生產環境除錯

### 連線調查

```bash
# 檢查活躍連線
curl http://chat-service:8082/health/detailed | jq '.checks.connection_pool'

# 監控 Prometheus 指標
curl http://chat-service:8082/metrics | grep chat_websocket
```

### 慢查詢調查

```sql
-- 啟用 pg_stat_statements
SELECT query, mean_exec_time, calls
FROM pg_stat_statements
WHERE query LIKE '%messages%'
ORDER BY mean_exec_time DESC
LIMIT 10;
```

### Redis 調查

```bash
# 檢查記憶體使用
redis-cli INFO memory

# 監控命令
redis-cli MONITOR

# 檢查慢日誌
redis-cli SLOWLOG GET 10
```

### 追蹤調查

1. 從日誌中找到 trace ID
2. 在 Jaeger UI 中搜尋
3. 分析 span 時間線
4. 識別瓶頸

---

## SLA 監控

### 可用性

```promql
# 服務可用性（基於健康檢查）
avg_over_time(up{job="chat-service"}[24h]) * 100
```

### 延遲 SLO

```promql
# 100ms 以內的請求百分比
sum(rate(chat_message_processing_duration_seconds_bucket{le="0.1"}[24h]))
/
sum(rate(chat_message_processing_duration_seconds_count[24h]))
* 100
```

### 錯誤預算

```promql
# 剩餘錯誤預算（目標：99.9% 可用性）
1 - (
  sum(rate(chat_message_errors_total[30d]))
  /
  sum(rate(chat_messages_sent_total[30d]))
) - 0.001
```
