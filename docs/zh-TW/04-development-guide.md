# 開發指南

本指南涵蓋本地開發流程、編碼規範和除錯技巧。

## 開發環境

### 必要工具

| 工具 | 用途 | 安裝 |
|------|------|------|
| Rust 1.75+ | 編譯器 | `rustup` |
| cargo-watch | 熱重載 | `cargo install cargo-watch` |
| sqlx-cli | 資料庫遷移 | `cargo install sqlx-cli` |
| Docker | 依賴服務 | docker.com |

### IDE 設置

**VS Code**（推薦）：
```json
{
  "rust-analyzer.cargo.features": "all",
  "rust-analyzer.checkOnSave.command": "clippy"
}
```

**擴充套件：**
- rust-analyzer
- Even Better TOML
- crates

## 專案結構

```
services/chat/
├── Cargo.toml           # 依賴項
├── Cargo.lock           # 鎖定檔
├── .env.example         # 環境範本
├── migrations/          # SQL 遷移
├── src/
│   ├── main.rs          # 入口點
│   ├── lib.rs           # 程式庫匯出
│   ├── api/             # HTTP/WS 處理器
│   ├── domain/          # 業務邏輯
│   ├── infrastructure/  # 外部服務
│   ├── server/          # 應用程式啟動
│   ├── shutdown.rs      # 優雅關閉
│   ├── tasks.rs         # 背景任務
│   └── telemetry.rs     # 追蹤設置
└── tests/
    └── load/            # K6 負載測試
```

## 開發流程

### 1. 啟動依賴服務

```bash
# 從專案根目錄
docker compose up -d postgres redis

# 驗證
docker compose ps
```

### 2. 配置環境

```bash
cd services/chat
cp .env.example .env
# 編輯 .env 設定
```

### 3. 執行遷移

```bash
# 檢查當前狀態
sqlx migrate info

# 套用待處理遷移
sqlx migrate run
```

### 4. 啟動開發伺服器

```bash
# 使用熱重載（推薦）
cargo watch -x run

# 標準執行
cargo run

# 帶除錯日誌
RUST_LOG=debug cargo run
```

### 5. 測試 WebSocket 連線

```bash
# 使用 websocat
websocat "ws://localhost:8082/ws?token=YOUR_JWT"

# 或使用瀏覽器控制台
# new WebSocket('ws://localhost:8082/ws?token=...')
```

## 程式碼風格

### 格式化

```bash
# 格式化程式碼
cargo fmt

# 檢查格式（CI）
cargo fmt --check
```

### 程式碼檢查

```bash
# 執行 clippy
cargo clippy

# 包含所有功能
cargo clippy --all-features

# 嚴格模式（警告視為錯誤）
cargo clippy -- -D warnings
```

### 慣例

1. **錯誤處理**：使用 `thiserror` 定義自訂錯誤
   ```rust
   #[derive(Debug, thiserror::Error)]
   pub enum MyError {
       #[error("操作失敗: {0}")]
       OperationFailed(String),
   }
   ```

2. **非同步函式**：始終使用 `#[tracing::instrument]`
   ```rust
   #[tracing::instrument(skip(state))]
   pub async fn handler(state: &AppState) -> Result<(), Error> {
       // ...
   }
   ```

3. **模組組織**：每個檔案一個概念
   ```
   domain/message/
   ├── mod.rs         # 重新匯出
   ├── handler.rs     # MessageHandler
   ├── storage.rs     # MessageStorage
   ├── router.rs      # MessageRouter
   └── types.rs       # 資料結構
   ```

## 測試

### 單元測試

```bash
# 執行所有測試
cargo test

# 執行特定測試
cargo test test_name

# 帶輸出
cargo test -- --nocapture

# 單執行緒（用於整合測試）
cargo test -- --test-threads=1
```

### 編寫測試

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_message_creation() {
        let storage = MessageStorage::new(pool);
        let message = storage.create_message(...).await.unwrap();
        assert_eq!(message.content, "Hello");
    }
}
```

### 負載測試

```bash
cd tests/load

# WebSocket 連線測試
k6 run websocket_load.js

# 訊息吞吐量測試
k6 run --vus 100 --duration 5m message_throughput.js

# 使用自訂目標
TARGET=ws://staging:8082 k6 run websocket_load.js
```

## 除錯

### 日誌

```bash
# 設定日誌級別
RUST_LOG=debug cargo run

# 模組特定日誌
RUST_LOG=chat_service::domain::message=trace cargo run

# 結構化輸出
RUST_LOG=info,tower_http=debug cargo run
```

### 追蹤

啟用 OpenTelemetry 進行分散式追蹤：

```env
CHAT__OTEL__ENABLED=true
CHAT__OTEL__ENDPOINT=http://localhost:4317
```

在 Jaeger UI 檢視追蹤：`http://localhost:16686`

### 資料庫查詢

```bash
# 啟用 SQL 日誌
RUST_LOG=sqlx=debug cargo run

# 互動式 psql
docker compose exec postgres psql -U ara -d ara_chat

# 常用查詢
SELECT * FROM messages ORDER BY created_at DESC LIMIT 10;
SELECT * FROM conversations WHERE id = 'uuid';
```

### Redis 除錯

```bash
# 互動式 CLI
docker compose exec redis redis-cli

# 監控命令
MONITOR

# 檢查鍵
KEYS chat:*
GET chat:presence:user-uuid
```

## 常見任務

### 新增端點

1. 在 `src/api/rest.rs` 定義處理器：
   ```rust
   pub async fn my_handler(
       State(state): State<AppState>,
       headers: HeaderMap,
   ) -> Result<Json<Response>, (StatusCode, Json<ErrorResponse>)> {
       let user_id = extract_user_id(&headers, &state)?;
       // ...
   }
   ```

2. 在 `src/api/routes.rs` 新增路由：
   ```rust
   .route("/api/v1/my-endpoint", get(my_handler))
   ```

### 新增 WebSocket 訊息

1. 在 `src/domain/message/types.rs` 新增 `ClientMessage` 變體：
   ```rust
   pub enum ClientMessage {
       // ...
       MyNewMessage { field: String },
   }
   ```

2. 在 `src/api/websocket.rs` 處理：
   ```rust
   ClientMessage::MyNewMessage { field } => {
       handle_my_new_message(user_id, field, state).await;
   }
   ```

### 新增遷移

```bash
# 建立遷移檔案
sqlx migrate add add_my_column

# 編輯 SQL 檔案
vim migrations/YYYYMMDD_add_my_column.sql

# 套用
sqlx migrate run
```

## 效能分析

### CPU 分析

```bash
# 使用 flamegraph
cargo install flamegraph
cargo flamegraph --bin chat-service
```

### 記憶體分析

```bash
# 使用 heaptrack
heaptrack ./target/release/chat-service
heaptrack_print heaptrack.chat-service.*.gz
```

### 效能測試

```bash
# 連線吞吐量
k6 run --vus 1000 --duration 30s tests/load/websocket_load.js

# 訊息延遲
k6 run --vus 100 --duration 5m tests/load/message_throughput.js
```

## 故障排除

### 編譯錯誤

```bash
# 清理建置
cargo clean && cargo build

# 更新依賴
cargo update
```

### 執行時 Panic

1. 啟用回溯：`RUST_BACKTRACE=1 cargo run`
2. 檢查 unwrap() 呼叫
3. 驗證環境變數

### 連線問題

1. 檢查服務健康狀態：`curl http://localhost:8082/health`
2. 驗證 JWT token 有效性
3. 檢查 Redis/PostgreSQL 連線
