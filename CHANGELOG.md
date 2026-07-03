# 变更记录

## 2026-07-03

### Workflow Notes

- 新增 `docs/DEVELOPMENT_WORKFLOW.md`，同时记录原 Codex 项目经理流程和当前 GPT 临时 PR 审核流程
- 明确当前唯一生效流程：GPT 从 GitHub 分支 / PR 审核，Code / OpenCode 总控开发
- 明确原 Codex 项目经理 / 最终审查流程保留，但因 Codex 额度不足临时暂停
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
