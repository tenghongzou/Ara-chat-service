# 架構設計

Ara 聊天服務遵循清潔架構原則，各層之間有清晰的分離。

## 系統概覽

```
┌─────────────────────────────────────────────────────────────────┐
│                        負載均衡器                                │
└─────────────────────────┬───────────────────────────────────────┘
                          │
        ┌─────────────────┼─────────────────┐
        ▼                 ▼                 ▼
┌───────────────┐ ┌───────────────┐ ┌───────────────┐
│  Chat Pod 1   │ │  Chat Pod 2   │ │  Chat Pod N   │
│  (10萬連線)   │ │  (10萬連線)   │ │  (10萬連線)   │
└───────┬───────┘ └───────┬───────┘ └───────┬───────┘
        │                 │                 │
        └─────────────────┼─────────────────┘
                          │
        ┌─────────────────┼─────────────────┐
        ▼                 ▼                 ▼
┌───────────────┐ ┌───────────────┐ ┌───────────────┐
│ Redis Cluster │ │   PostgreSQL  │ │  Prometheus   │
│  (Pub/Sub)    │ │    (Citus)    │ │   (指標)      │
└───────────────┘ └───────────────┘ └───────────────┘
```

## 分層架構

```
src/
├── api/                    # 展示層
│   ├── health.rs          # 健康檢查端點
│   ├── rest.rs            # REST API 處理器
│   ├── routes.rs          # 路由配置
│   └── websocket.rs       # WebSocket 處理器
│
├── domain/                 # 業務邏輯層
│   ├── cluster/           # 跨實例路由
│   ├── connection/        # WebSocket 連線管理
│   ├── conversation/      # 對話 CRUD
│   ├── mention/           # @提及解析
│   ├── message/           # 訊息處理與儲存
│   ├── presence/          # 在線/離線追蹤
│   ├── reaction/          # 表情反應
│   └── receipt/           # 已讀回執
│
├── infrastructure/         # 外部依賴層
│   ├── auth/              # JWT 驗證
│   ├── config/            # 設定管理
│   ├── postgres/          # 資料庫連線池
│   ├── redis/             # 快取與 Pub/Sub
│   ├── ratelimit/         # 限流
│   ├── sharding/          # 用戶分片
│   ├── circuit_breaker.rs # 熔斷器
│   └── metrics/           # Prometheus
│
└── server/                 # 應用程式啟動
    ├── mod.rs             # 路由設置
    └── state.rs           # AppState 初始化
```

## 領域模組

### 連線管理器
使用 DashMap 實現無鎖連線追蹤。

**功能特點：**
- 每用戶連線限制（預設：5）
- 全域連線限制（預設：10 萬）
- SmallVec 優化（大多數用戶只有 1-2 個連線）

```rust
// 連線註冊流程
ConnectionManager::register(connection)
    -> 檢查全域限制
    -> 檢查用戶限制
    -> 存入 DashMap<UserId, SmallVec<Connection>>
```

### 訊息路由器
跨實例路由訊息給接收者。

**路由策略：**
1. **本地投遞**：用戶有活躍連線時直接傳送
2. **叢集路由**：通過 Redis Pub/Sub 發送到其他實例
3. **離線佇列**：用戶不在線時存入 Redis（7 天 TTL）

```
┌──────────────────────────────────────────────────────────────┐
│                      訊息路由器                              │
├──────────────────────────────────────────────────────────────┤
│  1. 檢查本地連線（DashMap 查詢）                             │
│     ↓ 找到 → 通過 mpsc channel 發送                         │
│                                                              │
│  2. 檢查叢集會話儲存（Redis）                                │
│     ↓ 找到 → 發布到 chat:cluster:route                      │
│                                                              │
│  3. 加入離線佇列                                             │
│     ↓ 存入帶 TTL 的 Redis 列表                               │
└──────────────────────────────────────────────────────────────┘
```

### 對話服務
管理對話生命週期，支援 O(1) 私聊查詢。

**優化：**
- 使用排序後用戶 ID 的 SHA256 雜湊進行私聊查詢
- 消除「依參與者查找對話」的查詢

### 在線狀態追蹤器
基於 Redis 的在線狀態，採用訂閱模式。

**資料流：**
1. 用戶連線 → 標記上線（Redis SET 帶 TTL）
2. 通過 Pub/Sub 通知訂閱者
3. 用戶斷線 → 檢查剩餘連線 → 標記離線

## 資料庫架構

### 資料表
| 資料表 | 用途 | 分區 |
|--------|------|------|
| `conversations` | 對話元資料 | 無 |
| `conversation_participants` | 成員關係 | 無 |
| `direct_message_lookup` | O(1) 私聊查詢 | 無 |
| `messages` | 聊天訊息 | 按日期（pg_partman） |
| `message_reactions` | 表情反應 | 按日期 |
| `read_receipts` | 已讀狀態 | 無 |

### 索引
- `idx_messages_conversation`: (conversation_id, created_at DESC)
- `idx_messages_mentions`: GIN(mentions) 用於 @提及查詢
- `idx_messages_dedup`: UNIQUE(sender_id, client_message_id)

## 擴展考量

### 水平擴展
- 負載均衡器後的無狀態 Pod
- 通過 Redis Pub/Sub 進行跨實例訊息傳遞
- 不需要黏性會話（叢集路由處理）

### 垂直擴展
- 連線池（自適應大小）
- 全程非同步 I/O（Tokio 運行時）
- 無鎖資料結構（DashMap）

### 資料分片
- 1024 用戶分片（一致性雜湊）
- 相容 Citus 架構，支援十億級規模
- 按日期分區的訊息表
