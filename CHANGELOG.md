# 变更记录

## 2026-07-04

### P0 Urgent MVP Notes

- 创建 `feature/p0-urgent-mvp` 分支，进入 P0 急用连续开发模式。
- 新增 `docs/P0_URGENT_MVP_DEV_PLAN.md`，作为本轮 Code / OpenCode 的开发任务书。
- 明确本轮只使用一个开发分支，不新开多个 PR，不主动触发 GitHub Actions；远程提交带 `[skip ci]`。
- 事实源为 `docs/P0_URGENT_MVP_DEV_PLAN.md`、`AGENT_HANDOFF.md`、`TODO.md`、`PROJECT_STATUS.md`、`CHANGELOG.md`，PR 描述只做摘要。
- 本轮只补 P0 急用缺口，不进入 Provider 自动适配、网页余额抓取、ccusage 导入、趋势图、云同步、多用户、UI 大改版。

### R1-R5 返工（按 docs/P0_URGENT_MVP_REVIEW.md）

- **R1**：`PROJECT_STATUS.md` 开头改「P0 急用闭环已完成，等待 GPT 复审 / 合并」，删除「不能完整交付」与 `p0_complete` 并存，「下一步」改等待复审。
- **R2（补代码）**：总览页新增 today/week/month 三段消耗卡片。store 新增 `UsageSummaryTotalByDateRange(start,end)`；`OverviewPage` 计算 UTC 今日/本周(周一)/本月；`qb_overview.html` 新增「消耗统计」section。
- **R3（文档降级）**：平台 `supports_tools_json`(bool)=「支持工具调用」；模型 `tool_fit_json`(text)=「适合工具标签」。UI 文案统一（平台列/总览列「支持工具调用」、模型列「适合工具标签」、模型表单「适合工具标签 (JSON)」）。DEV_PLAN P0-3 加「采用口径」。
- **R4（文档）**：DEV_PLAN P0-4 加「采用口径」：Base URL 只读 HTTP 探测，不发凭据、不调付费 API、不网页登录、只存可达状态+脱敏消息；会外联用户录入的 Base URL，非余额抓取非 Provider 自动适配。代码未改。
- **R5**：PR #2 描述改纯摘要，取舍与口径写入事实源文档。
- 同步 AGENT_HANDOFF / PROJECT_STATUS / TODO / CHANGELOG / DEV_PLAN / REVIEW。

返工验证：`go build` 通过；`go test ./internal/store ./internal/dashboard` 通过；`go test ./internal/web -run TestQB|TestServer_ServesHTML` 全 PASS；仅 `TestHandlerTryAutoDetectAdditionalCoverage` 预存失败；总览页冒烟含「消耗统计|今日|本周|本月|支持工具调用」。

### P0-1~P0-6 补缺完成（feature/p0-urgent-mvp，首次交付）

- **P0-1 UsageLog 录入闭环**：新增 `UsageNewForm/UsageEditForm/UsageSave/UsageDelete` 4 handler + `qb_usage_form.html` 模板 + `/qb/usage/new|edit|save|delete` 4 路由；Handler struct 增 `usageFormTmpl`；`qb_usage.html` 增新增/编辑/删除按钮。
- **P0-2 三段消耗口径**：store 新增 `UsageSummaryByDateRange(planID, start, end)`（半开区间 `period_start >= ? AND period_start < ?`）；`UsagePage` 重写为今日/本周（周一起）/本月三段 UTC 汇总，传 `SumSections` 给模板；`qb_usage.html` 渲染三段汇总表；seed 补 3 条样例 log（今日/10天前/40天前）验证过滤。
- **P0-3 适合工具字段**：`PlatformSave` 读 `supports_tools_json`、`ModelSave` 读 `tool_fit_json`；平台表单加 checkbox、模型表单加 textarea；config/plans/overview 三页展示；seed 给火山方舟与豆包 Coding 补字段值。
- **P0-4 配置状态手动刷新**：新增 `ConfigRefreshAction` + `probeBaseURL`，对每平台 Base URL 做 5s 只读 HTTP 探测（不发任何凭据），按状态码分类 healthy/warning/danger，upsert 写回 `qb_credential_statuses`；路由 `/qb/config/refresh`；`qb_config.html` 增刷新按钮与检测方式/消息列。
- **P0-5 web 502 排查**：根因为环境代理（`HTTP_PROXY` 拦截 `0.0.0.0` 请求），非代码 bug；测试前关闭 `HTTP_PROXY/HTTPS_PROXY`（或 `NO_PROXY` 含 `0.0.0.0`）即全部通过；未改生产代码（默认 host 维持 0.0.0.0 以免破坏 Docker）。
- **P0-6 文档同步**：更新 PROJECT_STATUS / TODO / CHANGELOG / AGENT_HANDOFF。

### 验证

- `go build ./...` 通过；`go vet`（改动包）通过。
- `go test ./internal/store ./internal/dashboard` 通过。
- `go test ./internal/web`：502 全部修复，仅剩 `TestHandlerTryAutoDetectAdditionalCoverage`（Windows HOME vs USERPROFILE，stash 证实预存）。
- HOME 隔离端到端冒烟通过：usage 三段汇总 / 用量表单 / 配置刷新写回 / 适合工具展示 均渲染正常。
- reviewer 自审 Go/No-Go = Go，禁改区零触碰。

### Notes

- Go 1.26.4 装到 `~/.local/go`（zip + .NET 解压）。
- 新增 `.gitignore` 忽略 `onwatch-smoke.exe`。
- 台账层 P0 已交付，剩余失败均为 onWatch 原有测试的 Windows 平台不兼容（底座问题）。

## 2026-07-03

### Workflow Notes

- 新增 `docs/DEVELOPMENT_WORKFLOW.md`，同时记录原 Codex 项目经理流程和当前 GPT 临时 PR 审核流程
- 明确当前唯一生效流程：GPT 从 GitHub 分支 / PR / 交接文档审核，Code / OpenCode 总控开发
- 明确原 Codex 项目经理 / 最终审查流程保留，但因 Codex 额度不足临时暂停
- 明确硬规则：所有开发必须在独立分支进行，禁止直接在 `main` 开发；每个分支必须同步更新交接文档
- 明确关键区别：Codex 正式流程审本地分支 / 本地 diff / 测试结果 / 交接文档，不强制 GitHub PR；GPT 临时流程必须通过 GitHub PR 审核
- 明确事实源：`AGENT_HANDOFF.md`、`PROJECT_STATUS.md`、`TODO.md`、`CHANGELOG.md`，PR 描述只作为远程审核入口
- README 增加开发流程文档入口，执行顺序改为先读 `docs/DEVELOPMENT_WORKFLOW.md`
- `PROJECT_STATUS.md` 增加当前开发流程说明

### Audit Notes

- 补充审查确认：当前状态不应标记为 `p0_complete`
- 记录 3 个真实功能缺口：`UsageLog` 无录入闭环、配置状态页无真实检测刷新、`适合工具` 字段未落地
- 记录 1 个管理缺口：状态文档与代码状态漂移
- 补充验证结果：`internal/store` 测试通过，但 `internal/web` 测试当前失败，存在 `502` 与静态资源响应异常
- 将后续工作重点从“进入 P1”改回“补齐 P0 缺口”

### Added

- 新增 `PROJECT_STATUS.md`，固化当前开发状态、阻塞和今日目标
- 新增 `TODO.md`，拆分包 0 到包 3 的执行清单和验收
- 新增 `AGENTS.md`，约束执行模型的读文档顺序、停手规则和状态更新规则
- 新增 `AGENT_HANDOFF.md`，给执行开发方的今日任务书

### 底座入仓与阶段 0

- 合并上游 `onllm-dev/onwatch` 底座代码进工作区（go.mod / internal/ / cmd/ / main.go 等），module `github.com/onllm-dev/onwatch/v2`，Go 1.25.7，modernc.org/sqlite 纯 Go 无 CGO
- onWatch 原始 README 存档为 `docs/ONWATCH_README.md`，本仓库 README 保持看板入口
- 阶段 0 结构审查完成：store / web / 采集层 / 启动入口 四路并行（商汤 SenseNova 子 agent），确认接入点与禁改区
- 台账层表名前缀定为 `qb_`，业务逻辑放 `internal/dashboard/` 子包，与采集层隔离

### 阶段 1 数据骨架

- 新增 `internal/store/quota_board_store.go`：7 个 struct（Platform/Plan/QuotaBucket/Model/UsageLog/CredentialStatus/RiskNote）+ UsageSummary，43 个方法（CRUD + 关联查询 + 派生查询 ListExpiringPlans/ListLowQuotaBuckets/ListActiveRiskNotes/UsageSummaryByPeriod）
- `internal/store/store.go` `createTables()` schema 末尾追加 7 张 `qb_*` 表 DDL（幂等 CREATE TABLE IF NOT EXISTS，外键 REFERENCES）
- 安装 Go 1.26.4 到 `~/.local/go`（环境原先无 Go/brew/docker，用户级安装不污染系统，可回滚）
- Go module 代理换 `goproxy.cn` 直连绕开本地代理断连，`go build ./...` 通过，原 onWatch 功能未破坏

### 阶段 2 页面骨架

- 新增 `internal/dashboard/` 业务包：`import_export.go`（ExportData/ImportData 脱敏导出导入）+ `seed.go`（SeedSampleData 4 平台样例数据）+ `recommend.go`（Recommend 首页推荐）
- 新增 `internal/web/quota_handlers.go`：7 个页面 GET handler + 3 个 action（seed/export/import）
- `internal/web/handlers.go`：Handler struct +7 模板字段，NewHandler +7 模板解析
- `internal/web/server.go`：NewServer +10 路由（/qb/ 等）
- 新增 7 个模板 `qb_*.html` + `layout.html` 追加看板导航（避开 Login 页）
- 修复 `qb_plans.html` 模板作用域 bug（`$.ID` → `$platID`，套餐此前不显示）
- 新增模板语法 + 渲染集成测试（`qb_template_check_test.go` / `qb_render_test.go`）
- `go build` 通过；模板语法/渲染测试通过（总览+套餐页 seed→recommend→渲染字段对齐）；store 现有测试未破坏

### 阶段 3 录入闭环

- 新增 4 个表单模板（platform / plan / bucket / risk）+ 17 个录入 handler + 15 条路由
- CSRF 用 `X-Requested-With` + fetch + JSON redirect 机制
- `qb_plans.html` / `qb_risks.html` 追加新增 / 编辑 / 删除按钮
- 端到端 HTTP 验证通过：登录 → 灌 seed → 总览 → 录入新平台 → 套餐页显示 → 导出 JSON
- 修复技术问题：`--test` 模式不隔离 DB 路径（忽略 `ONWATCH_DB_PATH`），验证时数据写入用户真实 DB，已用临时清理程序清除 qb_* 测试数据
- P0 全部交付：原 onWatch 未破坏、录入 / 推荐 / 风险 / 导出闭环可用、跨平台无 CGO、无真实密钥

### 缺口修复（自审后）

- S1 导入 CSRF 闭环：`qb_import_export.html` 原生 form 改 fetch + X-Requested-With；ImportAction 改返回 JSON
- S2 当前模型管理 UI：新增 Model 4 handler + `qb_model_form.html` + 路由 + PlansPage 模型列表入口
- G1 风险备注编辑：新增 RiskEditForm + 路由 + 编辑按钮 + resolved_at 字段
- G2 ImportData ID 重建：store 加 DeleteAllQB + 7 Insert*WithID（INSERT OR REPLACE 保留原 ID），防 FK 孤儿
- G3 导航高亮：Nav "importexport" → "import-export"
- G4 桶编辑入口：`qb_plans.html` 桶行加编辑链接
- 端到端复验通过（HOME=/tmp 隔离）：模型录入 + 导入 CSRF 闭环 + 导入后 ID 重建数据完整
- 修正：`--test` 不隔离 DB，端到端验证改用 HOME 隔离避免污染用户 DB

### Notes

- P0 主骨架已完成，但仍处于补缺阶段，未正式进入 P1
