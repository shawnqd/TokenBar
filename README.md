# 模型额度看板

本目录当前只放开发前基础文档，不放实现代码。

目标：为后续新的 chat、Code、OpenCode 提供直接可执行的开发依据，避免重复调研和反复传话。

文档入口：

- `docs/PRODUCT_PLAN.md`：产品计划书
- `docs/TECHNICAL_MANUAL.md`：技术手册
- `docs/MAINTENANCE_MANUAL.md`：维护与使用手册
- `docs/CODE_HANDOFF.md`：给 Code / OpenCode 的执行任务书
- `docs/DEVELOPMENT_WORKFLOW.md`：开发流程，包含原 Codex 流程和当前 GPT 临时 PR 审核流程

当前结论：

- 继续使用 `onWatch` 作为底层运行底座。
- 第一阶段不从零开发，不先做全自动抓取。
- 先做 P0：本地套餐台账、额度桶、总览推荐、配置状态、风险备注、导入导出。
- Provider 自动适配放在 P1，且默认关闭高风险的 Cookie / 网页抓取路径。

执行入口：

- `AGENT_HANDOFF.md`：给执行开发方的当日交接单
- `PROJECT_STATUS.md`：当前阶段、阻塞和今日目标
- `TODO.md`：包 0 到包 3 执行清单
- `CHANGELOG.md`：文档与执行状态变更记录
- `AGENTS.md`：执行约束和停手规则

后续执行顺序：

1. 先读 `docs/DEVELOPMENT_WORKFLOW.md`
2. 再读 `docs/CODE_HANDOFF.md`
3. 再读 `docs/PRODUCT_PLAN.md`
4. 再读 `docs/TECHNICAL_MANUAL.md`
5. 严格先做 P0，再做 P1
