# API Versioning Strategy / API 版本策略

This document defines the API versioning strategy for Ara Chat Service.

本文檔定義 Ara Chat Service 的 API 版本策略。

---

## Table of Contents / 目錄

1. [Overview / 概述](#overview--概述)
2. [Versioning Approach / 版本方法](#versioning-approach--版本方法)
3. [Version Numbering / 版本編號](#version-numbering--版本編號)
4. [REST API Versioning / REST API 版本](#rest-api-versioning--rest-api-版本)
5. [WebSocket API Versioning / WebSocket API 版本](#websocket-api-versioning--websocket-api-版本)
6. [Backward Compatibility / 向後兼容](#backward-compatibility--向後兼容)
7. [Deprecation Policy / 棄用政策](#deprecation-policy--棄用政策)
8. [Breaking vs Non-Breaking Changes / 破壞性與非破壞性變更](#breaking-vs-non-breaking-changes--破壞性與非破壞性變更)
9. [Migration Guide Template / 遷移指南模板](#migration-guide-template--遷移指南模板)

---

## Overview / 概述

Ara Chat Service uses **path-based API versioning** for REST endpoints and **capability negotiation** for WebSocket connections. This approach ensures:

- Clear version identification in URLs
- Parallel operation of multiple API versions
- Graceful deprecation of older versions
- Client-side feature detection for WebSocket

Ara Chat Service 對 REST 端點使用**路徑式 API 版本**，對 WebSocket 連接使用**能力協商**。此方法確保：

- URL 中清晰的版本識別
- 多個 API 版本的並行運行
- 舊版本的優雅棄用
- WebSocket 的客戶端功能檢測

---

## Versioning Approach / 版本方法

### REST API: Path-Based Versioning / 路徑式版本

All REST endpoints include the version in the URL path:

```
https://api.example.com/api/v1/conversations
https://api.example.com/api/v2/conversations  (future)
```

**Rationale / 原因:**
- Simple and explicit
- Easy to route at load balancer level
- Cache-friendly (different URLs = different cache entries)
- No header parsing required

### WebSocket API: Capability Negotiation / 能力協商

WebSocket connections use capability negotiation instead of path versioning:

```
ws://api.example.com/ws?token=JWT
```

The client announces capabilities after connection:
```json
{
  "type": "Capabilities",
  "payload": {
    "compression": ["zstd"],
    "protocol_version": "1.12.0",
    "max_message_size": 1048576
  }
}
```

Server responds with acknowledged capabilities:
```json
{
  "type": "capabilities_ack",
  "compression": "zstd",
  "threshold": 1024,
  "protocol_version": "1.12.0"
}
```

### Unversioned Endpoints / 非版本化端點

Operational endpoints do not require versioning:

| Endpoint | Purpose |
|----------|---------|
| `GET /health` | Basic health check |
| `GET /health/live` | Kubernetes liveness probe |
| `GET /health/ready` | Kubernetes readiness probe |
| `GET /health/detailed` | Detailed health with version info |
| `GET /metrics` | Prometheus metrics |
| `WS /ws` | WebSocket connection |

---

## Version Numbering / 版本編號

### Service Version (Semantic Versioning)

The service follows [Semantic Versioning 2.0.0](https://semver.org/):

```
MAJOR.MINOR.PATCH
```

| Component | When to Increment |
|-----------|-------------------|
| **MAJOR** | Breaking API changes requiring new API version (v1 → v2) |
| **MINOR** | New features, backward-compatible additions |
| **PATCH** | Bug fixes, security patches, performance improvements |

**Current Version:** See `Cargo.toml` and `/health/detailed` endpoint.

### API Version

API versions use simple integer numbering:

```
v1, v2, v3, ...
```

A new API version is created **only** when breaking changes are unavoidable.

---

## REST API Versioning / REST API 版本

### Current API: v1

Base URL: `/api/v1/`

#### Endpoint Categories

| Category | Base Path | Description |
|----------|-----------|-------------|
| Conversations | `/api/v1/conversations` | Conversation CRUD |
| Messages | `/api/v1/messages` | Message operations |
| Attachments | `/api/v1/attachments` | File uploads |
| Users | `/api/v1/users` | User management |
| Emojis | `/api/v1/emojis` | Custom emoji |
| Email | `/api/v1/email` | Email preferences |
| GDPR | `/api/v1/gdpr` | Data export/deletion |

### Headers

All API responses include version information:

```http
X-API-Version: v1
X-Service-Version: 1.12.0
```

### Request Version Header (Optional)

Clients may specify desired API version via header:

```http
Accept-Version: v1
```

If not specified, the latest stable version is used.

---

## WebSocket API Versioning / WebSocket API 版本

### Protocol Version

The WebSocket protocol version matches the service MINOR version:

| Service Version | Protocol Version | Changes |
|-----------------|------------------|---------|
| 1.0.x - 1.9.x | 1.0 | Initial protocol |
| 1.10.x | 1.10 | Added compression |
| 1.11.x | 1.11 | Added subscription mode |
| 1.12.x | 1.12 | Added FetchParticipants |

### Message Type Evolution

New message types are **additive** and backward-compatible:

```rust
// v1.11.0 added:
SubscribeConversations { conversation_ids: Vec<Uuid> }
UnsubscribeConversations { conversation_ids: Vec<Uuid> }

// v1.12.0 added:
FetchParticipants { conversation_id, offset, limit }
```

**Rule:** Clients MUST ignore unknown message types gracefully.

### Capability Detection

Clients should check server capabilities before using features:

```javascript
// After receiving capabilities_ack
if (serverCapabilities.compression === 'zstd') {
  enableCompression();
}

if (serverCapabilities.protocol_version >= '1.12.0') {
  // Can use FetchParticipants
}
```

---

## Backward Compatibility / 向後兼容

### Guaranteed Compatibility Window

| Version Type | Support Duration |
|--------------|------------------|
| Current API (vN) | Indefinite |
| Previous API (vN-1) | 12 months after vN release |
| Older APIs (vN-2, ...) | 6 months deprecation notice |

### What We Maintain / 維護承諾

1. **Endpoint URLs** - Will not change within API version
2. **Response structure** - Fields will not be removed or renamed
3. **Required request fields** - Will not add new required fields
4. **Error codes** - Existing codes will not change meaning
5. **HTTP status codes** - Same errors return same status codes

### What May Change / 可能變更

1. **New optional fields** - May be added to requests/responses
2. **New endpoints** - May be added within same version
3. **New WebSocket message types** - May be added
4. **Performance improvements** - Response times, etc.
5. **Default values** - May change (documented in CHANGELOG)

---

## Deprecation Policy / 棄用政策

### Deprecation Timeline / 棄用時程

```
┌─────────────────────────────────────────────────────────────────┐
│ Announce     │ Warning Period    │ Sunset          │ Removed   │
│ Deprecation  │ (6 months)        │ (6 months)      │           │
├──────────────┼───────────────────┼─────────────────┼───────────┤
│ Day 0        │ Day 1 - Day 180   │ Day 181 - 365   │ Day 366+  │
│              │ Deprecation header│ 410 Gone option │ 404/410   │
└─────────────────────────────────────────────────────────────────┘
```

### Deprecation Headers

Deprecated endpoints return warning headers:

```http
Deprecation: true
Sunset: Sat, 01 Jan 2027 00:00:00 GMT
Link: </api/v2/conversations>; rel="successor-version"
X-Deprecation-Notice: This endpoint will be removed on 2027-01-01. Use /api/v2/conversations instead.
```

### CHANGELOG Notices

All deprecations are documented in CHANGELOG.md:

```markdown
## [1.13.0] - 2026-02-01

### Deprecated
- `GET /api/v1/messages/search` - Use `POST /api/v1/messages/search` instead
  - Sunset date: 2026-08-01
  - Reason: GET with body is not HTTP compliant
```

---

## Breaking vs Non-Breaking Changes / 破壞性與非破壞性變更

### Non-Breaking Changes (Minor Version) / 非破壞性變更

These changes are safe within the same API version:

| Change Type | Example |
|-------------|---------|
| Add optional field to request | `{ "content": "...", "priority": 1 }` |
| Add field to response | `{ "id": "...", "created_at": "..." }` |
| Add new endpoint | `POST /api/v1/reactions/bulk` |
| Add new WebSocket message | `FetchParticipants` |
| Add new enum value | `ContentType::Voice` |
| Widen accepted input | Accept both `"true"` and `true` |
| Relax validation | Accept longer strings |

### Breaking Changes (Major Version Required) / 破壞性變更

These changes require a new API version:

| Change Type | Example |
|-------------|---------|
| Remove endpoint | Delete `GET /api/v1/legacy` |
| Remove field from response | Remove `participant_ids` |
| Rename field | `user_id` → `userId` |
| Change field type | `id: string` → `id: number` |
| Add required field | New required `tenant_id` |
| Change URL structure | `/conversations/{id}` → `/chats/{id}` |
| Change authentication | JWT → OAuth2 |
| Change error format | Different error response schema |
| Narrow accepted input | Stricter validation |

---

## Migration Guide Template / 遷移指南模板

When releasing a new API version, include a migration guide:

```markdown
# Migration Guide: v1 → v2

## Overview

API v2 introduces [summary of changes].

## Breaking Changes

### 1. Endpoint Renamed

**Before (v1):**
```http
GET /api/v1/conversations/{id}/members
```

**After (v2):**
```http
GET /api/v2/conversations/{id}/participants
```

### 2. Response Structure Changed

**Before (v1):**
```json
{
  "members": [{"user_id": "..."}]
}
```

**After (v2):**
```json
{
  "participants": [{"id": "...", "role": "member"}]
}
```

## Migration Steps

1. Update all endpoint URLs from `/api/v1/` to `/api/v2/`
2. Update response parsing for changed fields
3. Test thoroughly in staging environment
4. Deploy client updates
5. Monitor error rates

## Timeline

- **v2 Released:** 2026-06-01
- **v1 Deprecated:** 2026-06-01
- **v1 Sunset:** 2026-12-01
- **v1 Removed:** 2027-06-01
```

---

## Client Implementation Guidelines / 客戶端實作指南

### Recommended Practices / 建議做法

1. **Always specify API version explicitly**
   ```javascript
   const API_BASE = 'https://api.example.com/api/v1';
   ```

2. **Handle unknown fields gracefully**
   ```javascript
   // Good: Destructure only needed fields
   const { id, content, sender_id } = message;

   // Avoid: Strict schema validation that rejects unknown fields
   ```

3. **Check for deprecation headers**
   ```javascript
   if (response.headers.get('Deprecation') === 'true') {
     console.warn('API deprecated:', response.headers.get('X-Deprecation-Notice'));
   }
   ```

4. **Implement feature detection for WebSocket**
   ```javascript
   socket.on('capabilities_ack', (caps) => {
     this.serverVersion = caps.protocol_version;
     this.features = detectFeatures(caps);
   });
   ```

5. **Handle version mismatch errors**
   ```javascript
   if (error.code === 'VERSION_NOT_SUPPORTED') {
     promptUserToUpdateApp();
   }
   ```

---

## Version History / 版本歷史

| API Version | Release Date | Status | Sunset Date |
|-------------|--------------|--------|-------------|
| v1 | 2026-01-03 | **Current** | - |

| Protocol Version | Service Version | Key Changes |
|------------------|-----------------|-------------|
| 1.0 | 1.0.0 | Initial WebSocket protocol |
| 1.10 | 1.10.0 | Compression support |
| 1.11 | 1.11.0 | Conversation subscription |
| 1.12 | 1.12.0 | Lazy loading (FetchParticipants) |

---

## References / 參考資料

- [Semantic Versioning 2.0.0](https://semver.org/)
- [RFC 8594 - Sunset HTTP Header](https://datatracker.ietf.org/doc/html/rfc8594)
- [RFC 8288 - Web Linking](https://datatracker.ietf.org/doc/html/rfc8288)
- [Microsoft REST API Guidelines - Versioning](https://github.com/microsoft/api-guidelines/blob/vNext/Guidelines.md#12-versioning)
- [Google Cloud API Versioning](https://cloud.google.com/apis/design/versioning)

---

Last updated: 2026-01-05
