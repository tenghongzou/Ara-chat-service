# TODO

Project status and planned features for Ara Chat Service.

## Completed Features / 已完成功能

### Core Messaging / 核心訊息
- [x] Private 1:1 conversations / 一對一私聊
- [x] Group chat / 群組聊天
- [x] Message history with pagination / 訊息歷史（分頁）
- [x] Message recall (2-minute window) / 訊息撤回
- [x] Message editing (15-minute window) / 訊息編輯
- [x] Message deduplication / 訊息去重

### Engagement / 互動功能
- [x] @mentions with validation / @提及與驗證
- [x] Emoji reactions / 表情反應
- [x] Typing indicators / 輸入中指示
- [x] Read receipts / 已讀回執
- [x] Unread counts / 未讀計數

### Real-time / 即時功能
- [x] WebSocket with heartbeat / WebSocket 心跳
- [x] Presence tracking / 在線狀態追蹤
- [x] Presence subscriptions / 在線狀態訂閱
- [x] Offline message queue / 離線訊息佇列

### Infrastructure / 基礎設施
- [x] Cluster mode (Redis Pub/Sub) / 叢集模式
- [x] Rate limiting / 限流
- [x] Circuit breaker / 熔斷器
- [x] Connection limits / 連線限制
- [x] Multi-tenant support / 多租戶支援

### Database / 資料庫
- [x] PostgreSQL integration / PostgreSQL 整合
- [x] Connection pooling / 連線池
- [x] Date-partitioned messages / 日期分區訊息
- [x] Full-text search / 全文搜尋
- [x] Citus sharding support / Citus 分片支援

### Observability / 可觀測性
- [x] Prometheus metrics / Prometheus 指標
- [x] OpenTelemetry tracing / OpenTelemetry 追蹤
- [x] Health check endpoints / 健康檢查端點
- [x] Structured logging / 結構化日誌

### Testing / 測試 *(v1.0.5)*
- [x] Unit tests: 253 tests passing / 單元測試：253 項通過
  - validation (sanitizer, limits, error): 33 tests
  - mention/parser: 14 tests
  - message (types, handler, router, storage, offline_queue): 89 tests
  - connection/manager: 18 tests
  - conversation/direct_lookup: 7 tests
  - presence/tracker: 22 tests
  - receipt/unread: 9 tests
  - cluster/session_store: 10 tests
  - infrastructure (ratelimit, circuit_breaker, sharding): 5 tests
  - api (error, middleware): 20 tests
  - redis/fallback: 8 tests
  - notification (types, publisher): 15 tests
- [x] Integration tests: 24 tests passing / 整合測試：24 項通過
  - health endpoints: 6 tests
  - authentication/authorization: 12 tests
  - request ID middleware: 2 tests
  - WebSocket protocol: 4 tests
- [x] Database migration tests: 11 tests passing / 資料庫遷移測試：11 項通過

### Documentation / 文檔
- [x] README.md
- [x] .env.example
- [x] OpenAPI 3.0 specification / OpenAPI 規範
- [x] Bilingual docs (en/zh-TW) / 雙語文檔
- [x] CONTRIBUTING.md
- [x] CHANGELOG.md

### File Handling / 檔案處理 *(v1.1.0)*
- [x] File upload with multipart support / 多部分上傳
- [x] Local filesystem storage / 本地檔案儲存
- [x] S3/MinIO integration / S3/MinIO 整合
- [x] Image thumbnail generation / 圖片縮圖生成
- [x] Content hash deduplication / 內容雜湊去重
- [x] MIME type validation / MIME 類型驗證
- [x] 50MB file size limit / 50MB 檔案限制

### Message Threading / 訊息串 *(v1.2.0)*
- [x] Reply to specific messages / 回覆特定訊息
- [x] Reply context preview (100 chars) / 回覆內容預覽
- [x] Thread queries with pagination / 串列查詢分頁
- [x] Reply count tracking / 回覆計數追蹤
- [x] ThreadUpdated WebSocket events / 即時串更新事件
- [x] Reply target validation / 回覆目標驗證

### Notification Integration / 通知整合 *(v1.2.0)*
- [x] Redis Pub/Sub integration / Redis Pub/Sub 整合
- [x] New message notifications (offline users) / 新訊息通知（離線用戶）
- [x] @mention notifications / @提及通知
- [x] Emoji reaction notifications / 表情反應通知
- [x] Configurable notification types / 可配置通知類型

### GDPR Compliance / GDPR 合規 *(v1.2.0)*
- [x] Data export (Art. 20 - Data Portability) / 資料匯出
- [x] Data deletion (Art. 17 - Right to Erasure) / 資料刪除
- [x] Message anonymization / 訊息匿名化
- [x] Audit logging (Art. 30 - Records of Processing) / 審計日誌
- [x] 7-year audit log retention / 7 年審計日誌保留
- [x] REST API endpoints / REST API 端點

### Message Pinning / 訊息置頂 *(v1.3.0)*
- [x] Pin/unpin messages (Owner/Admin only) / 置頂/取消置頂訊息
- [x] Get pinned messages list / 獲取置頂訊息列表
- [x] Real-time pin/unpin notifications / 即時置頂通知
- [x] WebSocket: PinMessage, UnpinMessage / WebSocket 訊息
- [x] REST API endpoints / REST API 端點
  - `POST /api/v1/conversations/{id}/messages/{msg_id}/pin`
  - `DELETE /api/v1/conversations/{id}/messages/{msg_id}/pin`
  - `GET /api/v1/conversations/{id}/pinned`

### Conversation Muting / 對話靜音 *(v1.3.0)*
- [x] Mute/unmute conversations / 靜音/取消靜音對話
- [x] Skip push notifications for muted conversations / 靜音對話跳過推送通知
- [x] @mentions override mute status / @提及覆蓋靜音狀態
- [x] Muted conversations list / 靜音對話列表
- [x] WebSocket: MuteConversation, UnmuteConversation / WebSocket 訊息
- [x] REST API endpoints / REST API 端點
  - `POST /api/v1/conversations/{id}/mute`
  - `DELETE /api/v1/conversations/{id}/mute`
  - `GET /api/v1/conversations/muted`

### User Blocking / 用戶封鎖 *(v1.4.0)*
- [x] Block/unblock users / 封鎖/解除封鎖用戶
- [x] Bidirectional DM blocking / 雙向私訊封鎖
- [x] Message filtering for blocked users / 封鎖用戶訊息過濾
- [x] Presence hiding for blocked users / 封鎖用戶在線狀態隱藏
- [x] Blocked users list / 封鎖用戶列表
- [x] WebSocket: BlockUser, UnblockUser, GetBlockedUsers / WebSocket 訊息
- [x] REST API endpoints / REST API 端點
  - `POST /api/v1/users/{id}/block`
  - `DELETE /api/v1/users/{id}/block`
  - `GET /api/v1/blocked-users`

### Message Forwarding / 訊息轉發 *(v1.5.0)*
- [x] Forward messages to one or more conversations / 轉發訊息到一個或多個對話
- [x] Batch forwarding (max 10 targets) / 批次轉發（最多 10 個目標）
- [x] Original message metadata preservation / 保留原始訊息元資料
- [x] Block check for DM conversations / 私訊對話封鎖檢查
- [x] WebSocket: ForwardMessage / WebSocket 訊息
- [x] REST API endpoint / REST API 端點
  - `POST /api/v1/messages/{id}/forward`
- [x] Database migration: `012_message_forwarding.sql`

---

## Planned Features / 計劃中功能

### High Priority / 高優先級

#### Testing / 測試
- [x] Unit tests for domain layer (211 tests) / 領域層單元測試 *(Completed in v1.0.4)*
- [x] Integration tests (24 tests) / 整合測試 *(Completed in v1.0.5)*
- [x] WebSocket protocol tests (4 tests) / WebSocket 協議測試 *(Completed in v1.0.5)*
- [x] Database migration tests (9 tests) / 資料庫遷移測試 *(Completed in v1.0.5)*

#### Security Enhancements / 安全增強
- [x] Input content sanitization (XSS prevention) / 輸入內容消毒
- [x] Message content length validation / 訊息長度驗證
- [x] JWT secret minimum length enforcement / JWT 密鑰長度驗證
- [x] CORS configuration for REST API / REST API CORS 配置

### Medium Priority / 中優先級

#### File Handling / 檔案處理 *(v1.1.0)*
- [x] File upload support / 檔案上傳支援
- [x] Image upload and thumbnails / 圖片上傳與縮圖
- [x] File storage integration (S3/MinIO) / 檔案儲存整合

#### Advanced Features / 進階功能
- [x] Message threading (replies) / 訊息串（回覆） *(v1.2.0)*
- [x] Message pinning / 訊息置頂 *(v1.3.0)*
- [x] Conversation muting / 對話靜音 *(v1.3.0)*
- [x] User blocking / 用戶封鎖 *(v1.4.0)*
- [x] Message forwarding / 訊息轉發 *(v1.5.0)*

#### Integration / 整合
- [x] Notification service integration (Redis Pub/Sub) / 通知服務整合 *(v1.2.0)*
- [ ] Email notification for offline users / 離線用戶郵件通知

### Low Priority / 低優先級

#### Data Management / 資料管理
- [x] Message export (GDPR) / 訊息匯出 *(v1.2.0)*
- [x] User data deletion (GDPR) / 用戶資料刪除 *(v1.2.0)*
- [x] Audit logging (GDPR) / 審計日誌 *(v1.2.0)*
- [ ] Data backup automation / 資料備份自動化

#### Performance / 效能
- [ ] Message compression / 訊息壓縮
- [ ] Connection multiplexing / 連線多工
- [ ] Lazy loading for large groups / 大群組懶載入

#### UI/UX Support / UI/UX 支援
- [ ] Link preview generation / 連結預覽
- [ ] Markdown rendering hints / Markdown 渲染提示
- [ ] Custom emoji support / 自訂表情支援

---

## Known Issues / 已知問題

### Security / 安全性
- [x] Message content not sanitized before storage / 訊息內容未消毒 *(Fixed in v1.0.1)*
- [x] Search query only escapes single quotes / 搜尋查詢轉義不完整 *(Fixed in v1.0.3)*

### Performance / 效能
- [x] Full-text search lacks dedicated index / 全文搜尋缺少專用索引 *(Fixed in v1.0.3)*
- [ ] pg_partman not auto-initialized / pg_partman 未自動初始化

### Documentation / 文檔
- [ ] Missing API versioning strategy / 缺少 API 版本策略

---

## Technical Debt / 技術債務

- [x] Consolidate error handling across modules / 統一錯誤處理 *(Unified ApiError with IntoResponse)*
- [x] Add request ID propagation / 添加請求 ID 傳播 *(X-Request-ID middleware)*
- [x] Improve configuration validation / 改進配置驗證 *(JWT secret length, input validation)*
- [x] Add graceful degradation for Redis failures / Redis 故障優雅降級 *(RedisFallback with backoff)*

---

## Milestones / 里程碑

### v1.1.0 (Completed)
- ~~Unit tests coverage > 80%~~ *(Completed: 211 tests)*
- ~~Input sanitization~~ *(Completed)*
- ~~Search query security~~ *(Completed)*
- ~~Full-text search index~~ *(Completed)*
- ~~Technical debt cleanup~~ *(Completed: unified errors, request ID, Redis fallback)*
- ~~Integration tests~~ *(Completed: 24 tests)*
- ~~Database migration tests~~ *(Completed: 10 tests)*
- ~~File upload support~~ *(Completed: Local + S3/MinIO, thumbnails)*

### v1.2.0 (Completed)
- ~~Message threading~~ *(Completed: thread queries, reply context, ThreadUpdated events)*
- ~~Notification service integration~~ *(Completed: Redis Pub/Sub, new message/mention/reaction notifications)*
- ~~GDPR compliance~~ *(Completed: data export, data deletion/anonymization, audit logging)*

### v1.3.0 (Completed)
- ~~Message pinning~~ *(Completed: pin/unpin, pinned list, real-time notifications, REST API)*
- ~~Conversation muting~~ *(Completed: mute/unmute, skip push notifications, @mentions override, REST API)*

### v1.4.0 (Completed)
- ~~User blocking~~ *(Completed: block/unblock, bidirectional DM blocking, message filtering, REST API)*

### v1.5.0 (Completed)
- ~~Message forwarding~~ *(Completed: forward to multiple conversations, batch forwarding, metadata preservation, block check)*

### v2.0.0 (Future)
- End-to-end encryption
- Voice messages
- Video call integration

---

Last updated: 2026-01-04

---

## Database Migrations / 資料庫遷移

1. `001_conversations.sql` - Conversations and participants
2. `002_messages.sql` - Messages with partitioning
3. `003_reactions.sql` - Emoji reactions
4. `004_read_receipts.sql` - Read receipt tracking
5. `009_message_pins.sql` - Message pinning support
6. `010_conversation_muting.sql` - Conversation muting support
7. `011_user_blocking.sql` - User blocking support
8. `012_message_forwarding.sql` - Message forwarding support
