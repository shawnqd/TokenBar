# MAC_MVP_PLAN

> Phase 1：Mac MVP。在 Phase 0 scaffold 基础上端到端打通一个 Provider，验证架构，再铺开。

## 1. MVP 目标

在 macOS 菜单栏跑起来一个能用的 TokenBar：
- 一个 Provider 端到端真实拉取额度并渲染
- 菜单栏图标 + popover + Provider 卡片 + 手动刷新 + 设置页 + 来源状态
- 取不到额度时明确 `unknown`
- 单 Provider 失败不影响 App

## 2. MVP Provider 选择

**首选：DeepSeek**（API key 查余额）。

理由：
- 来源最简单（单一 API key，无 Cookie / 无 CLI 依赖）
- 窗口清晰（balance，paid vs granted）
- 能最快验证 `Provider` 协议 + `SourceReader` + `UsageStore` + UI 全链路
- 验证完架构后再接 Codex / OpenCode Go 等复杂来源

> 备选：若 DeepSeek API 在实现期不可用，改 Doubao（API key request-limit probe），同样单一来源。

## 3. MVP 范围

### 做
- `TokenBarCore`
  - `DeepSeekProvider`：实现 `Provider` 协议，`apiKey` 来源，`balance` 窗口
  - `ApiKeyReader`：实现 `SourceReader`，从 `ConfigStore` 读 key
  - `ConfigStore`：读写 `~/.config/tokenbar/config.json`，文件权限 `0600`
  - `SnapshotStore`：持久化上次成功快照，标记 fresh/stale
  - `ProviderRegistry`：注册 DeepSeek
- `TokenBar` (App)
  - `TokenBarApp` + `AppDelegate`：菜单栏 status item、无 Dock 图标
  - `UsageStore`：`@MainActor`，单 Provider 刷新调度
  - `SettingsStore`：Provider 开关、API key 配置、刷新节奏
  - `SourceStatusStore`：DeepSeek apiKey 来源状态
  - `StatusItemController`：图标随余额/状态变化
  - `ProviderCardView`：DeepSeek 卡片（余额 + 来源 + 上次刷新 + 状态）
  - `SettingsView`：填 key、开关、刷新节奏
  - 手动刷新（全局 + 单 Provider）
- `TokenBarCLI`
  - `tokenbar status`：打印 DeepSeek 快照 JSON
  - `tokenbar config set-api-key --provider deepseek --stdin`

### 不做（Phase 2+）
- 其他 Provider（Codex / OpenCode Go / MiniMax / MiMo / Doubao / Volc Agent / ccswitch）
- Browser Cookie / LocalFile / CLIConfig 来源（MVP 只用 apiKey）
- 多窗口叠加渲染（MVP 只 balance）
- WidgetKit
- Windows
- 脱敏导入导出（Phase 2）
- Sparkle 自动更新（Phase 2+）

## 4. MVP 验收标准

1. `swift build` 通过，`swift test` 通过（含 DeepSeek 解析的单测，用 fixture 不发真请求）。
2. 启动 App 后菜单栏出现 status item，无 Dock 图标。
3. 设置页填入 DeepSeek API key → 保存到 `~/.config/tokenbar/config.json`（`0600`）。
4. 手动刷新 → popover 显示 DeepSeek 卡片：余额（paid / granted）、来源 `apiKey`、上次刷新时间、状态 `ok`。
5. 断网 / 错 key → 状态 `error`，余额显示 `unknown`，不崩溃，不伪造数字。
6. 未填 key → 状态 `unknown`，余额 `unknown`，不发请求。
7. 上次成功快照在 error 时显示并标注 `stale`，不冒充 fresh。
8. 刷新节奏可调（manual / 1m / 2m / 5m / 15m）。
9. `tokenbar status` CLI 输出与 popover 一致的快照 JSON。

## 5. MVP 技术约束

- macOS 14+，Swift 6 strict concurrency。
- 网络用 `URLSession`，异步，后台 task。
- 配置文件 JSON，schema 版本化（`schemaVersion` 字段）。
- API key 不进日志 / 崩溃报告。
- UI 用 SwiftUI；status item 用 `NSStatusItem`。
- 单 Provider 刷新失败不得阻塞 UI 或拖死菜单栏。

## 6. MVP 退出条件（触发停手）
- DeepSeek 真实 API 行为与假设差异过大，无法拿到真实余额 → 停，写 `AGENT_HANDOFF.md`，换备选 Provider，不伪造。
- 必须持久化真实 Cookie 才能继续 → 停，与用户确认。
- 必须破坏 Core / App 模块边界 → 停，重新评审架构。

## 7. MVP 之后（Phase 2 预告，不在本 PR 实现）
- 接入 Codex（CLI config + OAuth）、OpenCode Go（Cookie + SQLite）、MiniMax、MiMo、Doubao、Volc Agent
- 多窗口叠加渲染
- ccswitch 配置导入
- 脱敏导入导出
- WidgetKit
