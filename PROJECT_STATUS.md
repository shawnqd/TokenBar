# 项目状态

更新时间：2026-07-04

## 当前结论

项目进入 **P0 急用连续开发模式**。

P0 主骨架已完成，但还不能定义为“完整交付”。当前第一目标是补齐急用闭环，让用户可以真实记录、查看、判断模型套餐消耗。

当前执行分支：

```text
feature/p0-urgent-mvp
```

## 当前阶段

| 项 | 状态 |
|---|---|
| 阶段 | P0 补缺阶段 |
| 状态 | `p0_urgent_mvp_in_progress` |
| 开发方式 | 单一开发分支连续开发 |
| PR 策略 | 暂不新开 PR；最终 PR 只做摘要 |
| CI 策略 | GitHub Actions 仅手动触发；本轮不主动运行 |
| 日期 | 2026-07-04 |

## 当前开发流程

- 当前唯一生效流程：GPT 提供开发文档 / 交接文档，Code / OpenCode 按文档在本地分支执行。
- 分支：`feature/p0-urgent-mvp`。
- 同一时间只允许一个开发分支 / 一个 open PR。
- PR 描述只做摘要，事实源是仓库内文档。
- Codex 正式流程保留；后续可审本地分支、本地 diff、测试结果和交接文档。
- 不直接在 `main` 开发。

## 当前代码已经具备

- `qb_*` 数据表和台账层 store。
- 总览 / 套餐 / Provider / 使用记录 / 配置 / 风险 / 导入导出页面。
- 平台 / 套餐 / 额度桶 / 风险备注 / 模型录入闭环。
- 推荐、样例数据、脱敏导入导出。
- CI 已改成手动触发，自动 PR / push CI 已关闭。
- macOS runner 和 Codecov 自动上传已移除。

## 当前未完成缺口

### 缺口 1：UsageLog 没有录入闭环

现状：

- store 层已有使用记录基础能力。
- 使用记录页主要是展示和历史汇总。
- 没有完整 `usage/new`、`usage/edit`、`usage/save`、`usage/delete` 路由和表单。

影响：

- 用户无法手动补录或修正消耗记录。
- 看板不能作为急用台账。

### 缺口 2：today / week / month 口径未落地

现状：

- 使用记录页存在历史统计能力。
- “本月”不能用最近 30 天替代。

影响：

- 用户无法快速判断今天、本周、本月消耗。

### 缺口 3：只读配置检测仍是展示态

现状：

- `qb/config` 和 `qb/providers` 主要读取 `qb_credential_statuses`。
- 页面没有明确“刷新检测”闭环。
- 非测试代码中检测结果写回不足。

影响：

- 当前更接近“状态展示页”，不是“可刷新检测页”。

### 缺口 4：适合工具字段未落地

现状：

- schema 已有 `supports_tools_json` / `tool_fit_json`。
- 平台表单、模型表单、详情展示未完整接入。

影响：

- 无法快速判断某个平台 / 模型适合 Codex、OpenCode、Claude Code、API 等工具。

### 缺口 5：internal/web 测试存在 502 / 静态资源异常

现状：

- `internal/store` 曾验证通过。
- `internal/web` 曾出现 502、静态资源、HTML、metrics 返回异常。

影响：

- Web 层不能视为稳定验收。

## 本轮必须完成后才可宣称 P0 急用可用

1. UsageLog 录入 / 编辑 / 删除闭环。
2. 使用记录页和总览页展示 today / week / month。
3. 配置状态页接入真实只读检测刷新动作，并写回状态。
4. “适合工具”字段进入平台 / 模型录入和展示。
5. 修复或明确处置 `internal/web` 502 / 静态资源异常。
6. 状态文档与代码实际状态同步。

## 急用验收标准

- 可以手动录入平台、套餐、额度桶、模型、使用记录。
- 可以编辑和删除使用记录。
- 首页或使用记录页能看到今日 / 本周 / 本月消耗。
- 能看到平台 / 模型适合哪些工具。
- 配置状态能手动刷新检测。
- 不保存真实敏感凭据。
- Web 页面不出现 502。
- 原 onWatch 页面仍可访问。
- 状态文档与代码状态一致。
- 没有触发 GitHub Actions。

## 技术约束

1. 不进入 P1 Provider 自动适配。
2. 不做第三方网页登录余额抓取。
3. 不保存真实敏感凭据。
4. 不大改 onWatch 原始核心结构。
5. `internal/web` 只做最小修复。
6. 端到端验证必须用 `HOME=/tmp/xxx` 隔离，避免污染真实 DB。

## 本地验证命令

优先：

```bash
go test ./internal/store ./internal/dashboard ./internal/web
```

然后：

```bash
go test ./...
go build ./...
```

如测试失败，必须在 `AGENT_HANDOFF.md` 记录失败原因、失败范围、是否阻塞急用。

## 下一步

Code / OpenCode 直接按以下文档连续开发：

```text
docs/P0_URGENT_MVP_DEV_PLAN.md
AGENT_HANDOFF.md
TODO.md
```

完成 P0 急用闭环后，再考虑：

- Provider 自动适配
- `ccusage` 导入
- 趋势图
- 推荐增强
- UI 细化
