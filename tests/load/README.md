# Ara Chat Service - Load Testing

K6-based load testing suite for validating billion-scale performance.

## Prerequisites

```bash
# Install K6
brew install k6  # macOS
# or
sudo apt install k6  # Ubuntu/Debian
# or
docker pull grafana/k6
```

## Test Scenarios

### 1. WebSocket Connection Load (`websocket_load.js`)

Tests concurrent WebSocket connection handling capacity.

```bash
# Basic test with 1000 VUs
k6 run --vus 1000 --duration 5m websocket_load.js

# Full ramp-up test (built-in scenario)
k6 run websocket_load.js

# Target custom endpoint
k6 run --env TARGET=ws://chat-prod:8082 websocket_load.js
```

**Target Metrics:**
- 10M+ concurrent connections (scaled across pods)
- 95% connection success rate
- Connection time < 2s (p95)

### 2. Message Throughput (`message_throughput.js`)

Tests message processing capacity and delivery latency.

```bash
k6 run --vus 100 --duration 10m message_throughput.js
```

**Target Metrics:**
- 100K+ messages/second throughput
- 99% delivery rate
- p99 latency < 1s

## Scaling Calculations

For 10M concurrent connections with 100K connections per pod:
- Required pods: 100+
- K6 VUs per test: 10,000 (each VU = 1 connection)
- Distributed K6 setup required for full-scale tests

## Running Distributed Tests

```bash
# Using K6 Cloud
k6 cloud websocket_load.js

# Using K6 Operator on Kubernetes
kubectl apply -f k6-test-job.yaml
```

## Interpreting Results

| Metric | Healthy | Warning | Critical |
|--------|---------|---------|----------|
| Connection Success Rate | > 99% | 95-99% | < 95% |
| Message Latency (p95) | < 100ms | 100-500ms | > 500ms |
| Error Rate | < 0.1% | 0.1-1% | > 1% |

## Environment Variables

| Variable | Default | Description |
|----------|---------|-------------|
| `TARGET` | `ws://localhost:8082` | WebSocket endpoint |
| `JWT_SECRET` | test secret | JWT signing secret |
| `CONVERSATION_COUNT` | 1000 | Number of simulated conversations |
