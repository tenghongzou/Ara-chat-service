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

### Documentation / 文檔
- [x] README.md
- [x] .env.example
- [x] OpenAPI 3.0 specification / OpenAPI 規範
- [x] Bilingual docs (en/zh-TW) / 雙語文檔
- [x] CONTRIBUTING.md
- [x] CHANGELOG.md

---

## Planned Features / 計劃中功能

### High Priority / 高優先級

#### Testing / 測試
- [ ] Unit tests for domain layer / 領域層單元測試
- [ ] Integration tests / 整合測試
- [ ] WebSocket protocol tests / WebSocket 協議測試
- [ ] Database migration tests / 資料庫遷移測試

#### Security Enhancements / 安全增強
- [x] Input content sanitization (XSS prevention) / 輸入內容消毒
- [x] Message content length validation / 訊息長度驗證
- [x] JWT secret minimum length enforcement / JWT 密鑰長度驗證
- [x] CORS configuration for REST API / REST API CORS 配置

### Medium Priority / 中優先級

#### File Handling / 檔案處理
- [ ] File upload support / 檔案上傳支援
- [ ] Image upload and thumbnails / 圖片上傳與縮圖
- [ ] File storage integration (S3/MinIO) / 檔案儲存整合

#### Advanced Features / 進階功能
- [ ] Message threading (replies) / 訊息串（回覆）
- [ ] Message pinning / 訊息置頂
- [ ] Conversation muting / 對話靜音
- [ ] User blocking / 用戶封鎖
- [ ] Message forwarding / 訊息轉發

#### Integration / 整合
- [ ] Notification service webhook / 通知服務 Webhook
- [ ] Push notification support / 推送通知支援
- [ ] Email notification for offline users / 離線用戶郵件通知

### Low Priority / 低優先級

#### Data Management / 資料管理
- [ ] Message export / 訊息匯出
- [ ] User data deletion (GDPR) / 用戶資料刪除
- [ ] Audit logging / 審計日誌
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
- [ ] Search query only escapes single quotes / 搜尋查詢轉義不完整

### Performance / 效能
- [ ] Full-text search lacks dedicated index / 全文搜尋缺少專用索引
- [ ] pg_partman not auto-initialized / pg_partman 未自動初始化

### Documentation / 文檔
- [ ] Missing API versioning strategy / 缺少 API 版本策略

---

## Technical Debt / 技術債務

- [ ] Consolidate error handling across modules / 統一錯誤處理
- [ ] Add request ID propagation / 添加請求 ID 傳播
- [x] Improve configuration validation / 改進配置驗證 *(JWT secret length, input validation)*
- [ ] Add graceful degradation for Redis failures / Redis 故障優雅降級

---

## Milestones / 里程碑

### v1.1.0 (Planned)
- Unit tests coverage > 80%
- ~~Input sanitization~~ *(Completed)*
- File upload support

### v1.2.0 (Planned)
- Message threading
- Notification service integration
- GDPR compliance

### v2.0.0 (Future)
- End-to-end encryption
- Voice messages
- Video call integration

---

Last updated: 2026-01-03
