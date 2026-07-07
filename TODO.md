# TODO

## Phase 0 — Scaffold (done on `rebuild/tokenbar-main`)
- [x] 项目定位文档
- [x] 架构与模块边界文档
- [x] Provider 来源策略文档
- [x] Mac MVP 阶段计划
- [x] SwiftPM scaffold（TokenBarCore / TokenBar / TokenBarCLI / Tests）
- [x] LICENSE + CodexBar/ccusage attribution
- [x] swift build / swift test 通过
- [x] 文档口径修复：orphan/force 表述改为「非 orphan 重建分支 + 标准 PR / squash merge」；新增根目录 `PROJECT_STATUS.md` 作为事实源入口（仅文档，未改架构代码）

## Phase 1 — Mac MVP（待 GPT 审核通过后开工）
- [ ] `DeepSeekProvider`：实现 `Provider` 协议，apiKey 来源，balance 窗口
- [ ] `ApiKeyReader`：实现 `SourceReader`
- [ ] `ConfigStore`：读写 `~/.config/tokenbar/config.json`，0600
- [ ] `SnapshotStore`：持久化上次成功快照，标 fresh/stale
- [ ] `UsageStore.refresh`：注册表 → fetch → MainActor 发布
- [ ] `StatusItemController`：菜单栏图标随余额/状态变化
- [ ] `ProviderCardView`：余额 + 来源 + 上次刷新 + 状态
- [ ] `SettingsView`：填 key、开关、刷新节奏
- [ ] 手动刷新（全局 + 单 Provider）
- [ ] `tokenbar status` / `tokenbar config set-api-key` CLI
- [ ] DeepSeek 解析单测（fixture，不发真请求）
- [ ] MVP 9 条验收（见 docs/MAC_MVP_PLAN.md）

## Phase 2 — Provider 覆盖
- [ ] Codex（CLI config + OAuth）
- [ ] OpenCode Go（Cookie + SQLite）
- [ ] MiniMax / MiMo / Doubao / Volc Agent
- [ ] 多窗口叠加渲染
- [ ] ccswitch 配置导入
- [ ] 脱敏导入导出
- [ ] WidgetKit

## Phase 3 — Windows 原生端口
