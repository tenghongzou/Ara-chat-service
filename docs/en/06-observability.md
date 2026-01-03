# Observability

This document covers monitoring, metrics, tracing, and health checks.

## Health Checks

### Endpoints

| Endpoint | Purpose | Checks |
|----------|---------|--------|
| `/health` | Basic status | Service running |
| `/health/live` | Liveness probe | Process alive |
| `/health/ready` | Readiness probe | Redis, PostgreSQL, pools |
| `/health/detailed` | Full report | All components |

### Kubernetes Integration

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

### Response Format

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

## Prometheus Metrics

### Endpoint

```
GET /metrics
```

### Available Metrics

#### Connection Metrics

| Metric | Type | Description |
|--------|------|-------------|
| `chat_websocket_connections_total` | Gauge | Active WebSocket connections |
| `chat_websocket_connections_by_user` | Gauge | Connections per user |
| `chat_connection_duration_seconds` | Histogram | Connection lifetime |

#### Message Metrics

| Metric | Type | Description |
|--------|------|-------------|
| `chat_messages_sent_total` | Counter | Total messages sent |
| `chat_messages_received_total` | Counter | Total messages received |
| `chat_message_processing_duration_seconds` | Histogram | Processing latency |
| `chat_message_size_bytes` | Histogram | Message payload size |

#### System Metrics

| Metric | Type | Description |
|--------|------|-------------|
| `chat_db_pool_size` | Gauge | Database pool utilization |
| `chat_redis_pool_size` | Gauge | Redis pool utilization |
| `chat_circuit_breaker_state` | Gauge | Circuit breaker status |
| `chat_rate_limit_rejections_total` | Counter | Rate limit rejections |

### Example Queries

```promql
# Active connections
chat_websocket_connections_total

# Message throughput (per second)
rate(chat_messages_sent_total[5m])

# P95 message latency
histogram_quantile(0.95,
  rate(chat_message_processing_duration_seconds_bucket[5m])
)

# Circuit breaker status (0=closed, 1=half-open, 2=open)
chat_circuit_breaker_state{service="redis"}

# Database pool utilization
chat_db_pool_size{state="active"} / chat_db_pool_size{state="max"}
```

### Grafana Dashboard

```json
{
  "panels": [
    {
      "title": "Active Connections",
      "type": "stat",
      "targets": [{
        "expr": "chat_websocket_connections_total"
      }]
    },
    {
      "title": "Message Throughput",
      "type": "graph",
      "targets": [{
        "expr": "rate(chat_messages_sent_total[5m])",
        "legendFormat": "Messages/s"
      }]
    },
    {
      "title": "P95 Latency",
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

## OpenTelemetry Tracing

### Configuration

```env
CHAT__OTEL__ENABLED=true
CHAT__OTEL__ENDPOINT=http://localhost:4317
CHAT__OTEL__SERVICE_NAME=ara-chat-service
```

### Trace Context

Traces propagate through:
- HTTP headers (W3C Trace Context)
- Redis Pub/Sub messages
- Database queries

### Spans

| Span Name | Attributes |
|-----------|------------|
| `websocket.handle` | `user_id`, `connection_id` |
| `message.send` | `conversation_id`, `content_type` |
| `message.route` | `target_user_id`, `delivery_method` |
| `db.query` | `query_name`, `duration_ms` |
| `redis.command` | `command`, `key_prefix` |

### Jaeger Integration

```yaml
# docker-compose.yml
services:
  jaeger:
    image: jaegertracing/all-in-one:latest
    ports:
      - "16686:16686"  # UI
      - "4317:4317"    # OTLP gRPC
```

View traces at: `http://localhost:16686`

### Example Trace

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

## Logging

### Configuration

```env
RUST_LOG=info
# Or module-specific
RUST_LOG=chat_service=debug,sqlx=warn,tower_http=info
```

### Log Levels

| Level | Usage |
|-------|-------|
| `error` | Failures requiring attention |
| `warn` | Recoverable issues |
| `info` | Key operations |
| `debug` | Detailed flow |
| `trace` | Everything |

### Structured Logging

```rust
tracing::info!(
    user_id = %user_id,
    conversation_id = %conversation_id,
    content_len = content.len(),
    "Message sent"
);
```

Output:
```json
{
  "timestamp": "2026-01-03T12:00:00Z",
  "level": "INFO",
  "target": "chat_service::message::handler",
  "message": "Message sent",
  "user_id": "uuid",
  "conversation_id": "uuid",
  "content_len": 42
}
```

### Log Aggregation

Compatible with:
- Loki (Grafana)
- Elasticsearch
- CloudWatch Logs
- Datadog

---

## Alerting Rules

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
          summary: "High connection count on {{ $labels.instance }}"
          description: "Connections at {{ $value }}, approaching limit"

      - alert: CircuitBreakerOpen
        expr: chat_circuit_breaker_state == 2
        for: 1m
        labels:
          severity: critical
        annotations:
          summary: "Circuit breaker open for {{ $labels.service }}"

      - alert: HighMessageLatency
        expr: |
          histogram_quantile(0.95,
            rate(chat_message_processing_duration_seconds_bucket[5m])
          ) > 0.5
        for: 5m
        labels:
          severity: warning
        annotations:
          summary: "High message latency"
          description: "P95 latency is {{ $value }}s"

      - alert: DatabasePoolExhausted
        expr: |
          chat_db_pool_size{state="active"} /
          chat_db_pool_size{state="max"} > 0.9
        for: 5m
        labels:
          severity: critical
        annotations:
          summary: "Database pool nearly exhausted"

      - alert: RateLimitExceeded
        expr: rate(chat_rate_limit_rejections_total[5m]) > 100
        for: 1m
        labels:
          severity: warning
        annotations:
          summary: "High rate limit rejections"
```

---

## Performance Targets

| Metric | Target | Alert Threshold |
|--------|--------|-----------------|
| Connection Success Rate | > 99% | < 95% |
| Message Latency (P95) | < 100ms | > 500ms |
| Message Throughput | > 100K/s | - |
| Error Rate | < 0.1% | > 1% |
| Connection Pool Usage | < 80% | > 90% |

---

## Debugging Production

### Connection Investigation

```bash
# Check active connections
curl http://chat-service:8082/health/detailed | jq '.checks.connection_pool'

# Monitor Prometheus metrics
curl http://chat-service:8082/metrics | grep chat_websocket
```

### Slow Query Investigation

```sql
-- Enable pg_stat_statements
SELECT query, mean_exec_time, calls
FROM pg_stat_statements
WHERE query LIKE '%messages%'
ORDER BY mean_exec_time DESC
LIMIT 10;
```

### Redis Investigation

```bash
# Check memory usage
redis-cli INFO memory

# Monitor commands
redis-cli MONITOR

# Check slowlog
redis-cli SLOWLOG GET 10
```

### Trace Investigation

1. Find trace ID from logs
2. Search in Jaeger UI
3. Analyze span timeline
4. Identify bottlenecks

---

## SLA Monitoring

### Availability

```promql
# Service availability (based on health checks)
avg_over_time(up{job="chat-service"}[24h]) * 100
```

### Latency SLO

```promql
# % of requests under 100ms
sum(rate(chat_message_processing_duration_seconds_bucket{le="0.1"}[24h]))
/
sum(rate(chat_message_processing_duration_seconds_count[24h]))
* 100
```

### Error Budget

```promql
# Remaining error budget (target: 99.9% availability)
1 - (
  sum(rate(chat_message_errors_total[30d]))
  /
  sum(rate(chat_messages_sent_total[30d]))
) - 0.001
```
