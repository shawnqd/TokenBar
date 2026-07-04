# 开发交接单

更新时间：2026-07-04

## 当前结论

当前进入 **P0 急用连续开发模式**。

目标不是进入 P1，而是把模型额度看板从“P0 骨架可看”推进到“个人急用可用”。

执行分支：

```text
feature/p0-urgent-mvp
```

## 先读文件

执行方必须按顺序阅读：

```text
1. docs/P0_URGENT_MVP_DEV_PLAN.md
2. AGENT_HANDOFF.md
3. TODO.md
4. PROJECT_STATUS.md
5. docs/DEVELOPMENT_WORKFLOW.md
```

## 协作规则

| 角色 | 职责 |
|---|---|
| 用户 | 定目标、边界、取舍、最终确认 |
| GPT | 给开发文档、交接文档、审查口径 |
| Code / OpenCode | 具体实现、最小验证、回写状态文档 |
| Codex | 后续可审本地分支 |

PR 只做摘要，不是事实源。本轮先不新开 PR。

## 硬规则

1. 不直接在 `main` 开发。
2. 只使用 `feature/p0-urgent-mvp` 一个开发分支。
3. 不新开多个 PR。
4. 不触发 GitHub Actions；远程提交信息必须带 `[skip ci]`。
5. 不保存真实密钥、会话凭据、账号密码或任何敏感凭据。
6. 不做网页登录余额抓取。
7. 不进入 P1 Provider 自动适配。
8. 不大改 onWatch 原始核心结构。
9. 每完成一个小阶段，必须同步 `AGENT_HANDOFF.md`、`PROJECT_STATUS.md`、`TODO.md`、`CHANGELOG.md`。

## 背景

基于 onWatch 改造的「模型额度看板」P0 主骨架已完成，但补充审查确认仍有若干未完成项。底座 onWatch 已入仓，台账层与采集层隔离。

## 当前状态：P0 补缺阶段

阶段 0-3 和首轮自审修复已完成，但仍未达到“P0 完整交付”。

当前 main 已落地：

1. 开发流程文档。
2. Codex 本地审 / GPT PR 审规则。
3. CI 改为仅手动触发。
4. macOS runner 与 Codecov 自动上传已移除。

## 已交付清单

### 数据层（internal/store/）

- `quota_board_store.go`：7 struct + CRUD / 派生查询方法。
- `store.go` `createTables()` 追加 7 张 `qb_*` 表 DDL。
- 已具备 `UsageLog`、`CredentialStatus`、`supports_tools_json`、`tool_fit_json` 相关基础字段。

### 业务层（internal/dashboard/）

- `import_export.go`：ExportData / ImportData。
- `seed.go`：SeedSampleData。
- `recommend.go`：Recommend。

### Web 层（internal/web/）

- `quota_handlers.go`：P0 页面骨架、录入 handler、导入导出 action。
- 模板：总览、套餐、Provider、使用记录、配置、风险、导入导出、模型表单等。
- `server.go` 已接入 `/qb/*` 路由。

## 自审修复记录

| 项 | 问题 | 修复 |
|---|---|---|
| S1 | 导入原生 form 无 X-Requested-With → 403 | `qb_import_export.html` 改 fetch；ImportAction 返回 JSON |
| S2 | 当前模型管理 UI 缺失 | Model handler + `qb_model_form.html` + PlansPage 模型列表 |
| G1 | 风险备注编辑死代码 | RiskEditForm + 路由 + 编辑按钮 + resolved_at |
| G2 | ImportData 不保留 ID → FK 孤儿 | DeleteAllQB + Insert*WithID |
| G3 | 导航高亮不匹配 | Nav → import-export |
| G4 | 桶列表无编辑入口 | `qb_plans.html` 桶行编辑链接 |

## 本轮必须完成（P0 急用）

### P0-1：UsageLog 录入 / 编辑 / 删除闭环

必须实现：

```text
GET  /qb/usage/new
GET  /qb/usage/edit?id=<id>
POST /qb/usage/save
POST /qb/usage/delete
```

最小字段：平台、套餐、额度桶、模型、工具、项目、输入 token、输出 token、缓存命中输入、缓存未命中输入、费用、状态、备注、时间。

验收：用户能手动新增、编辑、删除一次使用记录。

### P0-2：使用记录页拆分 today / week / month

必须实现：

1. 今日消耗。
2. 本周消耗。
3. 本月消耗。

三段统计必须独立展示，不能用最近 30 天代替本月。

### P0-3：适合工具字段录入与展示

必须实现：

1. 平台表单支持适合工具。
2. 模型表单支持适合工具。
3. 套餐页 / 模型列表 / 总览页至少一个关键位置展示适合工具。

输入先用逗号分隔文本即可。

### P0-4：配置状态手动刷新

必须实现：

1. 配置状态页增加“刷新检测”按钮。
2. 只做本地只读检测。
3. 不保存真实敏感凭据。
4. 结果写回 `qb_credential_statuses`。

检测结果只保存：configured / missing / unknown、last_checked_at、message。

### P0-5：排查 internal/web 502 与静态资源异常

必须实现：

1. 复现失败测试。
2. 定位失败源。
3. 做最小修复。
4. 原 onWatch 页面仍可访问。

### P0-6：文档同步

必须同步：

```text
AGENT_HANDOFF.md
TODO.md
PROJECT_STATUS.md
CHANGELOG.md
```

不得把未完成项写成完成。

## 禁改区

除非 P0 明确阻塞，否则不要改：

```text
internal/agent/*
internal/api/*
internal/config/config.go
main.go
onWatch 原始核心数据结构
原有非 qb_* 业务表 schema
```

`internal/web` 可做最小修复，但不能破坏原 onWatch 页面。

## 技术问题记录

1. 端到端验证需用 `HOME=/tmp/xxx` 隔离，避免污染用户真实 DB。
2. Go 代理如受阻，优先使用 `env -u HTTP_PROXY -u HTTPS_PROXY GOPROXY=https://goproxy.cn,direct`。
3. `internal/web` 曾出现静态资源 / HTML / metrics 返回 `502`，本轮必须修复或写明是否阻塞。
4. GitHub Actions 已改为手动触发，本轮不得主动运行。

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

如失败，必须写明失败包、失败原因、是否阻塞急用。

## 急用验收标准

1. 可以手动录入平台、套餐、额度桶、模型、使用记录。
2. 可以编辑和删除使用记录。
3. 首页或使用记录页能看到今日 / 本周 / 本月消耗。
4. 能看到平台 / 模型适合哪些工具。
5. 配置状态能手动刷新检测。
6. 不保存真实敏感凭据。
7. Web 页面不出现 502。
8. 原 onWatch 页面仍可访问。
9. 状态文档与代码状态一致。
10. 没有触发 GitHub Actions。

## 明确非本轮

- Provider 自动适配。
- 第三方网页登录余额抓取。
- `ccusage` 导入。
- 趋势图。
- 云同步。
- 多用户。
- UI 大改版。

## 下一步给 Code / OpenCode

```text
拉最新 main，切换到 feature/p0-urgent-mvp。
阅读 docs/P0_URGENT_MVP_DEV_PLAN.md、AGENT_HANDOFF.md、TODO.md、PROJECT_STATUS.md。
按 P0-1 到 P0-6 顺序连续实现。
不要新开 PR，不要触发 GitHub Actions，不要扩大到 P1。
每完成一个小阶段，更新四份状态文档。
最后跑本地验证并把结果写进 AGENT_HANDOFF.md。
```
