# PROJECT_STATUS

> 当前主线：`rebuild/tokenbar-main`（orphan 重建，等待 GPT 审核后并回 `main`）

## 当前阶段
**Phase 0 — Scaffold（脚手架）**

本分支只交付：
- 项目定位文档
- 架构与模块边界文档
- Provider 来源策略文档
- Mac MVP 阶段计划
- SwiftPM 模块 scaffold（`TokenBarCore` / `TokenBar` / `TokenBarCLI` / 测试）
- MIT LICENSE 与 CodexBar / ccusage attribution

本分支**不包含**任何 Provider 业务逻辑、网络请求、Cookie 读取、UI 渲染实现。

## 分支与历史保护
- `legacy/web-p0-mvp`：保留旧 Go Web P0 MVP，不删除、不合并。
- `main`（远端当前）：仍指向旧 Web P0 commit `713d1ec`。本 PR 审核通过前不动 main。
- `rebuild/tokenbar-main`：orphan 重建分支，干净起点，无旧 Web 文件残留。

## 模块就绪度

| 模块 | 状态 | 说明 |
| --- | --- | --- |
| `TokenBarCore` | scaffold | Provider / Source / Window / Storage 类型与协议已定义，无实现 |
| `TokenBar` (App) | scaffold | App 入口、State、UI 占位，无渲染逻辑 |
| `TokenBarCLI` | scaffold | CLI 入口占位，无子命令实现 |
| `TokenBarTests` | scaffold | 占位测试，无业务断言 |
| docs | done | Phase 0 所需文档齐备 |

## 下一阶段入口
见 `docs/MAC_MVP_PLAN.md` —— Phase 1 将在 `TokenBarCore` + `TokenBar` 内端到端打通一个 Provider。

## 阻塞与风险
- 无当前阻塞。
- 待 GPT 审核：模块边界是否清晰、Provider 范围是否被偷删、source 策略假设是否成立。
- 审核 through 后再决定：直接 force-update `main`，或 merge PR。
