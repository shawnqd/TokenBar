# AGENT_HANDOFF

## 背景

把 `model-quota-board` 仓库的 `main` 从旧 Go Web P0 MVP 重建为 TokenBar 原生菜单栏额度管理工具。
旧代码已封存在 `legacy/web-p0-mvp` 分支（commit `713d1ec`，与重建前 `main` 同点），保留不删。

## 本次做了什么

在 `rebuild/tokenbar-main` 分支上完成 Phase 0 脚手架（该分支从 `main` 创建，**非 orphan**，删除全部旧 Web 文件后加入 TokenBar scaffold）：

- 文档：产品计划 / 架构 / Provider 来源策略 / Mac MVP 计划 / 项目状态。
- Scaffold：`TokenBarCore`（类型+协议骨架）+ `TokenBar`（App 占位）+ `TokenBarCLI`（占位）+ `TokenBarTests`（守卫测试）。
- `swift build` 通过，`swift test` 4/4 通过（XCTest 需 Xcode 工具链，用 `DEVELOPER_DIR=/Applications/Xcode.app/Contents/Developer swift test`）。
- LICENSE（MIT）+ CodexBar / ccusage attribution。

## 没做什么（重要）

- 没写任何 Provider 业务逻辑（fetch / parse / 网络 / Cookie 读取 / UI 渲染全部是 TODO(phase1)）。
- 没动 `main`，没动 `legacy/web-p0-mvp`。
- 没删减长期 Provider / 来源 / 窗口范围（守卫测试锁住了清单）。

## 决策与原因

1. **非 orphan 重建分支 + 标准 PR**：从 `main` 创建 `rebuild/tokenbar-main`，`git rm` 全部旧 Web 文件后加入 TokenBar scaffold。新分支树不含旧 Web 代码，但与 `main` 共享历史，可走标准 PR。
2. **合并方式**：标准 PR / squash merge 到 `main`。不 force push `main`，不用 `--allow-unrelated-histories`。
3. **MVP 首选 DeepSeek**：单一 apiKey 来源 + balance 窗口，最快验证全链路；复杂来源（Cookie/CLI）放 Phase 2。
4. **模块边界硬约束**：Core 不依赖 App；UI 不直接采集。这是参考 CodexBar 的核心收获。
5. **`unknown` 是契约**：取不到就 nil/unknown，守卫测试 `testUnknownWindowIsUnknown` 锁住语义。

## 需要确认的假设（GPT 审核重点）

- `PROVIDER_SOURCE_STRATEGY.md` 中各平台来源/窗口的假设是否成立？Phase 1 实现时需逐个核实真实 API/Cookie 行为。
- ccswitch 配置文件实际路径与格式？Phase 2 落地前需确认。
- 模块边界是否足够清晰、Provider 范围有无被偷删？
- 审核通过后：标准 PR / squash merge 到 `main`（不 force push，不用 `--allow-unrelated-histories`）。

## 下一步（审核通过后）

进入 Phase 1（见 `docs/MAC_MVP_PLAN.md`）：DeepSeek 端到端 + 菜单栏 + popover + 手动刷新 + 设置 + 来源状态。

## 技术备忘

- 本地 `xcode-select` 指向 CommandLineTools，跑测试需 `DEVELOPER_DIR` 指向 Xcode.app；纯 `swift build` 不需要。
- 本地工作目录 `/Users/shawnqd/Documents/Man Worker/model-quota-board` 是无 `.git` 的旧代码快照；重建工作在 staging 克隆中进行。审核通过后需把工作目录重新 clone 为新 main。
