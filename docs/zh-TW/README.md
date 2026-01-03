# Ara 聊天服務文檔

歡迎使用 Ara 聊天服務文檔。本服務提供基於 WebSocket 的即時訊息功能，設計支援 1 億日活用戶和 1000 萬峰值併發連線。

## 快速導覽

| 文檔 | 說明 |
|------|------|
| [架構設計](01-architecture.md) | 系統設計、分層架構、領域模組 |
| [安裝指南](02-installation.md) | Docker 與本地安裝、配置說明 |
| [API 參考](03-api-reference.md) | REST API 與 WebSocket 協議 |
| [開發指南](04-development-guide.md) | 本地開發、測試、除錯 |
| [進階功能](05-advanced-features.md) | 叢集模式、分片、限流 |
| [可觀測性](06-observability.md) | 指標、追蹤、健康檢查 |

## 功能概覽

### 核心訊息
- 一對一私聊
- 群組聊天（無限成員）
- 訊息歷史（游標分頁）
- 訊息撤回（2 分鐘內）
- 訊息編輯（15 分鐘內）

### 互動功能
- @提及與通知
- 表情符號反應
- 輸入中指示器
- 已讀回執與未讀計數

### 即時功能
- WebSocket 連線與心跳
- 在線狀態追蹤（線上/離線/離開/忙碌）
- 跨實例訊息路由（叢集模式）
- 離線訊息佇列（7 天保留）

### 企業功能
- 多租戶隔離
- 限流（每用戶 60 訊息/分鐘）
- 熔斷器容錯
- Prometheus 指標
- OpenTelemetry 分散式追蹤

## 快速開始

```bash
# 從專案根目錄
docker compose up chat redis postgres -d

# 驗證健康狀態
curl http://localhost:8082/health
```

## WebSocket 連線

```javascript
const ws = new WebSocket('ws://localhost:8082/ws?token=YOUR_JWT_TOKEN');

ws.onopen = () => {
  // 發送訊息
  ws.send(JSON.stringify({
    type: 'SendMessage',
    payload: {
      conversation_id: 'uuid',
      content: '你好！',
      content_type: 'text'
    }
  }));
};

ws.onmessage = (event) => {
  const message = JSON.parse(event.data);
  console.log('收到:', message);
};
```

## 支援

- GitHub Issues: [Ara-infra](https://github.com/tenghongzou/Ara-infra/issues)
- 主要文檔: [專案 README](../../README.md)
