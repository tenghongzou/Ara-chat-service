# Ara Chat Service Documentation

Welcome to the Ara Chat Service documentation. This service provides real-time messaging capabilities with WebSocket support, designed for 100M DAU and 10M peak concurrent connections.

## Quick Navigation

| Document | Description |
|----------|-------------|
| [Architecture](01-architecture.md) | System design, layers, and domain modules |
| [Installation](02-installation.md) | Docker and local setup, configuration |
| [API Reference](03-api-reference.md) | REST API and WebSocket protocol |
| [Development Guide](04-development-guide.md) | Local development, testing, debugging |
| [Advanced Features](05-advanced-features.md) | Cluster mode, sharding, rate limiting |
| [Observability](06-observability.md) | Metrics, tracing, health checks |

## Features Overview

### Core Messaging
- Private 1:1 conversations
- Group chat with unlimited participants
- Message history with cursor-based pagination
- Message recall (within 2 minutes)
- Message editing (within 15 minutes)

### Engagement
- @mentions with notification support
- Emoji reactions
- Typing indicators
- Read receipts and unread counts

### Real-time
- WebSocket connections with heartbeat
- Presence tracking (online/offline/away/busy)
- Cross-instance message routing (cluster mode)
- Offline message queue (7-day retention)

### Enterprise Features
- Multi-tenant isolation
- Rate limiting (60 msg/min per user)
- Circuit breaker for fault tolerance
- Prometheus metrics
- OpenTelemetry distributed tracing

## Quick Start

```bash
# From project root
docker compose up chat redis postgres -d

# Verify health
curl http://localhost:8082/health
```

## WebSocket Connection

```javascript
const ws = new WebSocket('ws://localhost:8082/ws?token=YOUR_JWT_TOKEN');

ws.onopen = () => {
  // Send a message
  ws.send(JSON.stringify({
    type: 'SendMessage',
    payload: {
      conversation_id: 'uuid',
      content: 'Hello!',
      content_type: 'text'
    }
  }));
};

ws.onmessage = (event) => {
  const message = JSON.parse(event.data);
  console.log('Received:', message);
};
```

## Support

- GitHub Issues: [Ara-infra](https://github.com/tenghongzou/Ara-infra/issues)
- Main Documentation: [Project README](../../README.md)
