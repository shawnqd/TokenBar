# PROJECT_STATUS

> 本文件是项目状态的事实源入口。详细模块就绪度见 `docs/PROJECT_STATUS.md`。

## 当前主线
TokenBar **Phase 0 — Scaffold**。

## 旧 Web 代码
旧 Go Web P0 MVP 已封存在 `legacy/web-p0-mvp` 分支（commit `713d1ec`），保留不删、不合并。

## 当前 PR
**#3** `rebuild/tokenbar-main` → `main`
- 该分支从 `main` 创建（**非 orphan**），删除全部旧 Web 文件后加入 TokenBar scaffold。
- 新分支树不含旧 Web 代码，与 `main` 共享历史。

## 合并方式
**标准 PR / squash merge** 到 `main`。
- 不 force push `main`
- 不用 `--allow-unrelated-histories`
- 待 GPT 复审通过后执行

## 下一步
GPT 复审通过 → 进入 **Phase 1**：DeepSeek 端到端（apiKey + balance 窗口）+ 菜单栏 + popover + 手动刷新 + 设置 + 来源状态。详见 `docs/MAC_MVP_PLAN.md`。

## 本轮变更
仅文档口径修复：把所有「orphan 重建 / force-update main」表述改为实际做法（非 orphan 重建分支 + 标准 PR / squash merge）。未改架构代码。
