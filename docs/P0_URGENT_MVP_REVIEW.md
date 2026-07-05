# P0 急用 MVP 审核返工交接

更新时间：2026-07-04

## 审核结论

No-Go，小返工后再审。

本次审核只以仓库事实源为准，不以 PR 描述作为需求来源。

事实源顺序：

```text
1. docs/P0_URGENT_MVP_DEV_PLAN.md
2. AGENT_HANDOFF.md
3. TODO.md
4. PROJECT_STATUS.md
5. CHANGELOG.md
```

PR 只做摘要入口，不承载具体需求、实现取舍和验收标准。

## 当前 PR

```text
PR: #2
Head: feature/p0-urgent-mvp
Base: main
状态：open
结论：暂不合并
```

## 总体判断

功能主体已经接近 P0 急用可用，但当前存在“实现口径与事实源文档不一致”的问题。返工目标不是大改功能，而是让代码、事实源文档、交接信息三者一致。

## 必须返工项

### R1：修 PROJECT_STATUS.md 前后冲突

问题：

`PROJECT_STATUS.md` 开头仍写“P0 主骨架已完成，但还不能定义为完整交付”，后面又写阶段为“P0 补缺完成”、状态为 `p0_complete`。同一文件内存在冲突。

要求：

1. 如果当前判断为 P0 已完成待审核，则开头也改成“P0 急用闭环已完成，等待 GPT 审核 / 合并”。
2. 不要同时出现“不能完整交付”和 `p0_complete`。
3. “下一步”不要再写“第一优先级是补齐 P0 缺口”，应改成“等待 GPT 复审；复审通过后合并，再进入 P1 规划”。

验收：

`PROJECT_STATUS.md` 从结论、阶段、缺口、下一步四处读下来不自相矛盾。

### R2：P0-2 today / week / month 口径对齐

事实源要求：

`docs/P0_URGENT_MVP_DEV_PLAN.md` 写的是“总览页和使用记录页都能看到今日、本周、本月消耗”。

当前实现：

使用记录页已经有 today / week / month 三段统计；总览页没有三段消耗展示。

返工二选一：

A. 补代码：在总览页增加 today / week / month 简表或卡片，至少展示输入、输出、缓存读取、缓存写入、费用。

B. 改事实源：如果最终产品口径决定只放使用记录页，则同步修改 `docs/P0_URGENT_MVP_DEV_PLAN.md`、`AGENT_HANDOFF.md`、`TODO.md`、`PROJECT_STATUS.md`、`CHANGELOG.md`，明确 P0 验收降级为“使用记录页展示 today / week / month；总览页留 P1”。

执行建议：优先 A。因为总览页展示三段消耗是急用价值，且已有 UsagePage 逻辑可复用或抽 helper。

### R3：P0-3 适合工具字段口径对齐

事实源要求：

`docs/P0_URGENT_MVP_DEV_PLAN.md` 写的是“平台 / 模型适合哪些工具”，输入格式可先用逗号分隔，如 Codex、OpenCode、Claude Code、API。

当前实现：

模型层 `tool_fit_json` 可录入文本；平台层 `supports_tools_json` 是 bool checkbox，只能表示“是否支持工具调用”，不能表示“适合哪些工具”。

返工二选一：

A. 补代码：平台层也支持具体工具标签。若要改 schema，必须最小迁移并保证导入导出兼容；不要破坏现有数据。

B. 改事实源：如果 P0 决定不改 schema，则文档明确降级：平台层只表示“是否支持工具调用”，具体工具标签只在模型层 `tool_fit_json` 维护。所有页面文案从“适合工具”改成“支持工具调用 / 模型适合工具”，避免误导。

执行建议：优先 B。P0 急用不建议为平台层新增 schema；但必须把文档和 UI 文案说清楚。

### R4：P0-4 配置状态刷新口径对齐

事实源要求：

`docs/P0_URGENT_MVP_DEV_PLAN.md` 写的是本地只读检测，范围包括环境变量是否存在、本地配置项是否存在，不调用付费 API，不网页登录。

当前实现：

`ConfigRefreshAction` 对 Base URL 做 HTTP GET 可达性探测，不发送凭据、不保存敏感信息、不网页登录。

返工二选一：

A. 改代码：改成本地环境变量 / 本地配置存在性检测，不外联。

B. 改事实源：明确 P0 允许采用 Base URL 只读可达性探测，要求为“不发凭据、不调用付费 API、不网页登录、只保存可达状态和脱敏消息”。同时说明这不是余额抓取，也不是 Provider 自动适配。

执行建议：优先 B，但要在文档中明确“会访问用户录入的 Base URL”。如果用户后续要求完全禁止外联，再改 A。

### R5：PR 描述保持摘要，不写细节

要求：

PR #2 描述只保留：

1. 本 PR 目标。
2. 事实源文件列表。
3. 改动摘要。
4. 验证摘要。
5. 风险摘要。

不要把具体需求、取舍、口径变化写在 PR 里；这些内容必须写进事实源文档。

## 本轮不得做

1. 不新开 PR。
2. 不合并 PR。
3. 不触发 GitHub Actions。
4. 不进入 P1。
5. 不做第三方网页登录余额抓取。
6. 不保存真实 API Key、Cookie、账号密码。
7. 不让 PR 描述替代交接文档。

## Code 执行指令

```text
继续在 feature/p0-urgent-mvp 分支返工。
不要新开 PR，不要触发 GitHub Actions，提交带 [skip ci]。

先读：
1. docs/P0_URGENT_MVP_REVIEW.md
2. docs/P0_URGENT_MVP_DEV_PLAN.md
3. AGENT_HANDOFF.md
4. PROJECT_STATUS.md
5. TODO.md
6. CHANGELOG.md

只做 R1-R5：
- R1 修 PROJECT_STATUS.md 前后冲突。
- R2 对齐 P0-2：优先总览页补 today/week/month；如不补，必须同步降级事实源文档。
- R3 对齐 P0-3：优先文档降级为“平台支持工具调用 bool，模型维护具体工具标签”，并修 UI 文案；如改代码/schema，必须最小迁移并说明兼容。
- R4 对齐 P0-4：优先把 Base URL 只读探测写入事实源文档，说明不发凭据、不调用付费 API、不网页登录；如禁止外联，则改成本地配置检测。
- R5 保持 PR 只做摘要，具体取舍全部写进事实源文档。

返工后必须更新：
- AGENT_HANDOFF.md
- PROJECT_STATUS.md
- TODO.md
- CHANGELOG.md
- docs/P0_URGENT_MVP_DEV_PLAN.md（如口径变化）
- docs/P0_URGENT_MVP_REVIEW.md（标记处理结果）

最后本地验证并把结果写回 AGENT_HANDOFF.md。
完成后停下，等待 GPT 复审。
```

## 返工处理结果（R1-R5，2026-07-04）

| 项 | 选择 | 处理 |
|---|---|---|
| R1 | — | `PROJECT_STATUS.md` 开头改「P0 急用闭环已完成，等待 GPT 复审 / 合并」；删除「不能完整交付」与「暂不应标记 p0_complete」并存；「下一步」改「等待 GPT 复审；通过后合并，再进 P1 规划」。结论/阶段/缺口/下一步四处不再自相矛盾。 |
| R2 | A（补代码） | 总览页新增 today/week/month 三段消耗卡片。store 新增 `UsageSummaryTotalByDateRange(start,end)`（跨所有 plan 聚合）；`OverviewPage` 计算 UTC 今日/本周(周一)/本月；`qb_overview.html` 新增「消耗统计」section，展示输入/输出/缓存读取/缓存写入/费用。使用记录页三段保留。 |
| R3 | B（文档降级） | 不改 schema。平台层 `supports_tools_json`(bool)=「支持工具调用」；模型层 `tool_fit_json`(text)=「适合工具标签」。UI 文案已统一：平台列/总览列「支持工具调用」、模型列「适合工具标签」、模型表单「适合工具标签 (JSON)」。`docs/P0_URGENT_MVP_DEV_PLAN.md` P0-3 段加「采用口径」确认。 |
| R4 | B（文档） | `docs/P0_URGENT_MVP_DEV_PLAN.md` P0-4 段加「采用口径」确认：Base URL 只读 HTTP 探测，不发凭据、不调付费 API、不网页登录、只存可达状态+脱敏消息；说明会外联用户录入的 Base URL，非余额抓取非 Provider 自动适配。代码未改（实现本就如此）。 |
| R5 | — | PR #2 描述改纯摘要：目标 / 事实源列表 / 改动摘要 / 验证摘要 / 风险摘要。具体取舍与口径变化全部写入事实源文档（AGENT_HANDOFF / DEV_PLAN）。 |

返工改动文件：`internal/store/quota_board_store.go`、`internal/web/quota_handlers.go`、`internal/web/templates/qb_overview.html`、`internal/web/templates/qb_config.html`、`internal/web/templates/qb_plans.html`、`internal/web/templates/qb_model_form.html` + 6 份文档。

本地验证：`go build ./...` 通过；`go test ./internal/store ./internal/dashboard` 通过；`go test ./internal/web` 仅剩预存 `TestHandlerTryAutoDetectAdditionalCoverage`（Windows HOME，与本轮无关）。详见 `AGENT_HANDOFF.md`。

### 复审补充：P0-2 UTC 口径（已处理）

复审追问 P0-2 时区口径与事实源是否一致。已处理：`docs/P0_URGENT_MVP_DEV_PLAN.md` P0-2 段后新增「P0-2 采用口径（R2 返工确认）」，明确采用 UTC（今日/本周/本月均以 UTC 计算，半开区间），理由为台账层既有 period_start / seed / recommend / ListExpiringPlans 统一 UTC，避免混用本地时区错位；UI 仍展示「今日/本周/本月」但底层为 UTC。`AGENT_HANDOFF.md` 已有对应说明，未重复长改。本轮为纯文档改动，未改功能代码。

## 复审标准

复审时只看：

1. R1-R5 是否处理。
2. 事实源文档是否一致。
3. 代码是否与事实源一致。
4. 是否未触发 CI。
5. 是否仍只有一个 open PR。

满足后才给 Go。