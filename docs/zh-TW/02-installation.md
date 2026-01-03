# 安裝指南

本指南涵蓋 Docker 部署和本地開發設置。

## 前置需求

| 需求 | 版本 |
|------|------|
| Rust | 1.75+ |
| PostgreSQL | 15+ |
| Redis | 7+ |
| Docker | 24+ |
| Docker Compose | 2.20+ |

## Docker 部署（推薦）

### 快速開始

```bash
# 從專案根目錄
cd /path/to/Ara-infra

# 複製環境配置
cp .env.example .env

# 啟動服務
docker compose up -d chat redis postgres

# 驗證健康狀態
curl http://localhost:8082/health
```

### 服務位址

| 服務 | 位址 |
|------|------|
| WebSocket | `ws://localhost:8082/ws?token=JWT` |
| REST API | `http://localhost:8082/api/v1/` |
| 健康檢查 | `http://localhost:8082/health` |
| 指標 | `http://localhost:8082/metrics` |

### 叢集模式（多實例）

高可用部署，使用多個聊天實例：

```bash
# 啟動 3 節點叢集
docker compose -f docker-compose.yml -f docker-compose.cluster.yml up -d
```

這會啟動：
- `chat-1` 在 8082 埠
- `chat-2` 在 8083 埠
- `chat-3` 在 8084 埠

## 本地開發

### 1. 安裝 Rust 工具鏈

```bash
# 安裝 rustup
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh

# 安裝穩定版工具鏈
rustup default stable

# 添加元件
rustup component add clippy rustfmt
```

### 2. 啟動依賴服務

```bash
# 啟動 PostgreSQL 和 Redis
docker compose up -d postgres redis

# 或使用本地安裝
# PostgreSQL: brew install postgresql@15
# Redis: brew install redis
```

### 3. 配置環境

```bash
cd services/chat

# 複製環境範本
cp .env.example .env

# 編輯配置
vim .env
```

**最低必要設定：**
```env
CHAT__JWT__SECRET=your-jwt-secret-key-minimum-32-characters
CHAT__DATABASE__URL=postgres://ara:ara_password@localhost:5432/ara_chat
CHAT__REDIS__URL=redis://localhost:6379
```

### 4. 執行資料庫遷移

```bash
# 安裝 sqlx-cli
cargo install sqlx-cli --no-default-features --features postgres

# 執行遷移
sqlx migrate run
```

### 5. 啟動服務

```bash
# 開發模式（熱重載）
cargo watch -x run

# 標準執行
cargo run
```

## 配置參考

### 伺服器設定

| 變數 | 預設值 | 說明 |
|------|--------|------|
| `CHAT__HOST` | `0.0.0.0` | 綁定地址 |
| `CHAT__PORT` | `8082` | 監聽埠 |
| `RUN_MODE` | `development` | development / production |
| `RUST_LOG` | `info` | 日誌級別 |

### JWT 認證

| 變數 | 必要 | 說明 |
|------|------|------|
| `CHAT__JWT__SECRET` | 是 | JWT 簽名密鑰（最少 32 字元） |
| `CHAT__JWT__ISSUER` | 否 | 預期 token 發行者 |
| `CHAT__JWT__AUDIENCE` | 否 | 預期 token 受眾 |

### 資料庫

| 變數 | 預設值 | 說明 |
|------|--------|------|
| `CHAT__DATABASE__URL` | - | PostgreSQL 連線 URL |
| `CHAT__DATABASE__MAX_CONNECTIONS` | `20` | 最大連線池大小 |
| `CHAT__DATABASE__MIN_CONNECTIONS` | `5` | 最小連線池大小 |
| `CHAT__DATABASE__RUN_MIGRATIONS` | `false` | 自動執行遷移 |

### Redis

| 變數 | 預設值 | 說明 |
|------|--------|------|
| `CHAT__REDIS__URL` | `redis://localhost:6379` | Redis URL |
| `CHAT__REDIS__POOL_SIZE` | `10` | 連線池大小 |
| `CHAT__REDIS__CLUSTER_ENABLED` | `false` | 啟用叢集模式 |

### WebSocket

| 變數 | 預設值 | 說明 |
|------|--------|------|
| `CHAT__WEBSOCKET__MAX_CONNECTIONS` | `100000` | 全域連線限制 |
| `CHAT__WEBSOCKET__MAX_CONNECTIONS_PER_USER` | `5` | 每用戶限制 |
| `CHAT__WEBSOCKET__HEARTBEAT_INTERVAL_SECONDS` | `30` | Ping 間隔 |

### 叢集模式

| 變數 | 預設值 | 說明 |
|------|--------|------|
| `CHAT__CLUSTER__ENABLED` | `false` | 啟用叢集路由 |
| `CHAT__CLUSTER__SERVER_ID` | 自動生成 | 唯一實例 ID |

## 資料庫遷移

### 建立新遷移

```bash
# 建立遷移檔案
sqlx migrate add <migration_name>

# 編輯生成的 SQL 檔案
vim migrations/<timestamp>_<migration_name>.sql

# 套用遷移
sqlx migrate run

# 回滾（如可逆）
sqlx migrate revert
```

### 現有遷移

| 遷移 | 用途 |
|------|------|
| `001_conversations.sql` | 對話與參與者 |
| `002_messages.sql` | 帶分區的訊息 |
| `003_reactions.sql` | 表情反應 |
| `004_read_receipts.sql` | 已讀追蹤 |

## 故障排除

### 連線被拒絕

```bash
# 檢查服務是否運行
docker compose ps

# 檢查日誌
docker compose logs chat

# 驗證埠綁定
netstat -tlnp | grep 8082
```

### 資料庫連線失敗

```bash
# 測試 PostgreSQL 連線
psql $CHAT__DATABASE__URL -c "SELECT 1"

# 檢查遷移狀態
sqlx migrate info
```

### Redis 連線失敗

```bash
# 測試 Redis 連線
redis-cli -u $CHAT__REDIS__URL ping
```

### JWT 驗證錯誤

1. 確保 `CHAT__JWT__SECRET` 與後端服務一致
2. 檢查 token 過期時間（`exp` claim）
3. 如有配置，驗證 issuer/audience
