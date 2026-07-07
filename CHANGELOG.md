# CHANGELOG

All notable changes to TokenBar are documented here.
This project follows [Keep a Changelog](https://keepachangelog.com/) principles.

## [Unreleased] — Phase 0 Scaffold

### Added
- 项目定位为原生菜单栏额度管理工具（TokenBar）。
- `docs/TOKENBAR_PRODUCT_PLAN.md`：产品定位、长期 Provider / 来源 / 窗口范围、阶段划分。
- `docs/TOKENBAR_ARCHITECTURE.md`：Core/App/CLI 模块边界、数据流、并发模型、关键类型契约。
- `docs/PROVIDER_SOURCE_STRATEGY.md`：8 个 Provider 的来源矩阵与诚实边界。
- `docs/MAC_MVP_PLAN.md`：Phase 1 Mac MVP 范围与 9 条验收标准。
- `docs/PROJECT_STATUS.md`：当前阶段与模块就绪度。
- SwiftPM scaffold：
  - `TokenBarCore`：Provider/Source/Window/Storage 类型与协议骨架（无业务实现）。
  - `TokenBar`：App 入口、State、UI 占位（无渲染逻辑）。
  - `TokenBarCLI`：CLI 入口占位。
  - `TokenBarTests`：4 个守卫测试，防止 Provider/Source/Window 范围被偷删。
- `LICENSE`（MIT）、`NOTICE`（CodexBar / ccusage attribution）、`AGENTS.md`、`.gitignore`、`Package.swift`。

### Notes
- 本分支 `rebuild/tokenbar-main` 为 orphan 重建，不含旧 Web 代码。
- 旧 Go Web P0 MVP 封存在 `legacy/web-p0-mvp`，保留不删。
- `swift build` 与 `swift test`（4 测试）均通过。
- 无任何 Provider 业务逻辑；等待 GPT 审核新 main 架构后再进入 Phase 1。
