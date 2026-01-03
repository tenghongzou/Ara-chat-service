# Contributing to Ara Chat Service

感謝您有興趣為 Ara 聊天服務做出貢獻！本文件提供開發環境設置和貢獻指南。

Thank you for your interest in contributing to the Ara Chat Service! This document provides guidelines for development setup and contributions.

## Development Setup / 開發環境設置

### Prerequisites / 前置需求

- Rust 1.75 or later
- Docker and Docker Compose
- PostgreSQL 15+
- Redis 7+

### Getting Started / 開始

```bash
# Clone the repository / 複製儲存庫
git clone https://github.com/tenghongzou/Ara-chat-service.git
cd Ara-chat-service

# Copy environment template / 複製環境範本
cp .env.example .env

# Start dependencies / 啟動依賴服務
docker compose up -d postgres redis

# Run migrations / 執行資料庫遷移
cargo install sqlx-cli --no-default-features --features postgres
sqlx migrate run

# Start development server / 啟動開發伺服器
cargo watch -x run
```

## Code Style / 程式碼風格

### Formatting / 格式化

We use `rustfmt` for code formatting. Run before committing:

```bash
cargo fmt
```

### Linting / 程式碼檢查

We use `clippy` for linting. All warnings must be resolved:

```bash
cargo clippy -- -D warnings
```

### Commit Messages / 提交訊息

We follow the [Conventional Commits](https://www.conventionalcommits.org/) specification:

```
<type>(<scope>): <description>

[optional body]

[optional footer]
```

**Types:**
- `feat`: New feature
- `fix`: Bug fix
- `docs`: Documentation changes
- `style`: Code style changes (formatting, etc.)
- `refactor`: Code refactoring
- `perf`: Performance improvements
- `test`: Adding or updating tests
- `chore`: Build, CI, or tooling changes

**Examples:**
```
feat(message): add message edit functionality
fix(websocket): handle connection timeout properly
docs(api): update WebSocket protocol documentation
```

## Pull Request Process / PR 流程

### Before Submitting / 提交前

1. **Create a feature branch / 建立功能分支**
   ```bash
   git checkout -b feature/my-feature
   ```

2. **Write tests / 編寫測試**
   - Add unit tests for new functionality
   - Ensure all existing tests pass

3. **Run checks / 執行檢查**
   ```bash
   cargo fmt --check
   cargo clippy -- -D warnings
   cargo test
   ```

4. **Update documentation / 更新文檔**
   - Update relevant documentation in `docs/`
   - Update `CHANGELOG.md` if applicable

### Submitting / 提交

1. Push your branch to GitHub
2. Create a Pull Request with a clear description
3. Link any related issues
4. Wait for review and address feedback

### Review Criteria / 審查標準

- Code follows project conventions
- Tests are included and passing
- Documentation is updated
- No security vulnerabilities introduced
- Performance impact is acceptable

## Testing / 測試

### Unit Tests / 單元測試

```bash
# Run all tests / 執行所有測試
cargo test

# Run specific test / 執行特定測試
cargo test test_message_creation

# Run with output / 帶輸出執行
cargo test -- --nocapture
```

### Load Tests / 負載測試

```bash
cd tests/load

# WebSocket connection test / WebSocket 連線測試
k6 run websocket_load.js

# Message throughput test / 訊息吞吐量測試
k6 run --vus 100 --duration 5m message_throughput.js
```

## Project Structure / 專案結構

```
src/
├── api/             # HTTP and WebSocket handlers
├── domain/          # Business logic (core domain)
│   ├── cluster/     # Multi-instance support
│   ├── connection/  # Connection management
│   ├── conversation/# Conversation CRUD
│   ├── mention/     # @mention parsing
│   ├── message/     # Message handling
│   ├── presence/    # Online status
│   ├── reaction/    # Emoji reactions
│   └── receipt/     # Read receipts
├── infrastructure/  # External services
│   ├── auth/        # JWT validation
│   ├── config/      # Configuration
│   ├── postgres/    # Database
│   ├── redis/       # Cache & Pub/Sub
│   └── ratelimit/   # Rate limiting
└── server/          # Application bootstrap
```

## Architecture Guidelines / 架構指南

### Clean Architecture / 清潔架構

- **API Layer**: HTTP handlers, WebSocket handlers
- **Domain Layer**: Business logic, no external dependencies
- **Infrastructure Layer**: Database, Redis, external services

### Error Handling / 錯誤處理

Use `thiserror` for custom error types:

```rust
#[derive(Debug, thiserror::Error)]
pub enum MyError {
    #[error("Operation failed: {0}")]
    OperationFailed(String),

    #[error("Not found")]
    NotFound,
}
```

### Async Code / 非同步程式碼

Use `#[tracing::instrument]` for async functions:

```rust
#[tracing::instrument(skip(state))]
pub async fn my_handler(state: &AppState) -> Result<(), Error> {
    // ...
}
```

## Security / 安全性

### Reporting Vulnerabilities / 回報漏洞

If you discover a security vulnerability, please email security@example.com instead of creating a public issue.

### Security Guidelines / 安全指南

- Never commit secrets or credentials
- Use parameterized queries for database operations
- Validate and sanitize all user input
- Follow the principle of least privilege

## Questions / 問題

- Open an issue for bugs or feature requests
- Check existing issues before creating new ones
- Use discussions for questions and ideas

## License / 授權

By contributing, you agree that your contributions will be licensed under the project's proprietary license.

---

Thank you for contributing! / 感謝您的貢獻！
