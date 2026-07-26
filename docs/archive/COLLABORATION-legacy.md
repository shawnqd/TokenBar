# COLLABORATION

跨 Codex、Claude Code、OpenCode、Grok 或其他终端在本仓库协作时的稳定规则。完整背景见用户维护的通用模板
[`cross-tool-dev-workflow`](https://github.com/shawnqd/ai-skills-vault/tree/main/workflows/cross-tool-dev-workflow)
（私有仓库）；本文件是该模板在 TokenBar 项目上的落地版本，以本文件和下方各交接文档为准。

## 平台选择

本轮执行平台由用户明确选择，不自动路由、不自动改派。未明确执行方时，停止修改并请求用户指定。

## 分支边界（本项目对模板的调整）

模板默认假设单一 `main` 工作分支。**TokenBar 不适用这条默认规则**——本仓库按
[`docs/PROJECT_LINES.md`](docs/PROJECT_LINES.md) 同时维护两条独立产品线：

| 分支 | 定位 | 应用代码 |
| --- | --- | --- |
| `platform/macos` | macOS 原型（Swift/SwiftUI） | 只属于这条线 |
| `platform/windows` | Windows 端（Tauri/React/Rust），当前默认活跃分支 | 只属于这条线 |
| `main` | 仅存放跨线共享文档、分支说明、决策记录 | 不放应用代码 |

因此本项目的"单一写入者"规则落地为：**每条平台线各自的当前活跃分支只允许一个写入者**，不得跨
Codex/Claude Code/OpenCode/Grok 并行修改同一平台线的同一批文件。本文件与
`CURRENT_TASK.md`/`AGENT_HANDOFF.md`/`PROJECT_STATUS.md`/`CODE_REVIEW.md`/`DECISIONS.md`/`logs/dev_audit.jsonl`
随 `platform/windows` 分支一起提交，不额外新建分支来维护它们。

跨线规则（不可覆盖，见 `docs/PROJECT_LINES.md`）：

1. 处理问题前先确认目标平台线；Windows 的问题只查 Windows 直接上游（`Finesssee/Win-CodexBar`）。
2. 不跨线推断窗口、托盘、Cookie、认证、更新或 UI 行为。
3. 共享产品规则、分支说明和决策记录只写入 `main` 文档；应用代码留在对应平台分支。

## 角色模型

1. **用户**：选择本轮执行平台，拥有产品范围、取舍、破坏性操作和最终 commit/push/merge 授权。
2. **总控/项目维护者**：维护任务包和共享证据；用户选中时也可执行；不得自动改派执行方。
3. **执行方**：同一时刻仅一个工具/模型；只实现任务包范围内的内容，自测并如实汇报。
4. **审查方**：检查 diff、测试、边界、运行时/数据影响和未解决风险，输出 findings + Go/No-Go。

## 标准流程

```text
用户目标
→ 用户选择一个执行方
→ 更新 CURRENT_TASK.md
→ 在 AGENT_HANDOFF.md 写一个任务包（如需要）
→ 执行方开始 checkpoint 并实现、自测
→ 执行方在 AGENT_HANDOFF.md 追加完成 checkpoint
→ （如有审查方）审查方在 CODE_REVIEW.md 输出 findings 与 Go/No-Go
→ 用户测试并明确授权后，才允许 commit / push / merge
```

不让用户充当工具间的传话人；所有阻塞、测试结果、运行时变化和下一步都写进 `AGENT_HANDOFF.md`。

## 安全与外部操作

- 不记录 Token、Cookie、密码、真实个人数据或敏感原始截图。
- 未经用户明确授权，不 commit、push、merge、发布、部署、迁移、删除或覆盖数据；本文件建立当轮的 commit
  已由用户在对话中明确授权，push/merge 仍需另行授权。
- 数据库/生产数据操作、依赖变更、外部服务变更必须暂停并获得授权。
- 技术问题写入交接文档并推进；产品范围、费用、隐私、安全、外部服务和主观体验取舍交回用户。
