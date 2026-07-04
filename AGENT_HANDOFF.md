# 开发交接单

## 背景

基于 onWatch 改造的「模型额度看板」P0 补缺已在本轮完成（P0-1~P0-6），底座 onWatch 未破坏，台账层与采集层隔离。

## 先读文件

执行方按顺序阅读：`docs/P0_URGENT_MVP_DEV_PLAN.md` → 本文件 → `TODO.md` → `PROJECT_STATUS.md` → `docs/DEVELOPMENT_WORKFLOW.md`。

## 当前状态：P0 补缺完成

阶段 0-3、首轮自审修复、以及本轮 P0-1~P0-6 补缺均已完成。代码层面 P0 已可交付，仅剩 onWatch 原有测试在 Windows 上的预存平台不兼容（非台账责任）。

## 本轮交付（feature/p0-urgent-mvp，P0-1~P0-6）

### P0-1 UsageLog 录入闭环
- `internal/web/quota_handlers.go`：新增 `UsageNewForm / UsageEditForm / UsageSave / UsageDelete` 4 个 handler。
- `internal/web/templates/qb_usage_form.html`：新建表单模板（plan/model 下拉 + bucket_scope/date_key/period_start/period_end/tokens/request_count/cost/source/source_ref）。
- `internal/web/server.go`：新增路由 `/qb/usage/new|edit|save|delete`。
- `internal/web/handlers.go`：Handler struct 增 `usageFormTmpl`，NewHandler 解析该模板。
- `qb_usage.html`：增「新增用量记录」按钮 + 明细行编辑/删除按钮。
- CSRF 走 `X-Requested-With` + fetch + JSON redirect，与既有表单一致。

### P0-2 today/week/month 三段消耗口径
- `internal/store/quota_board_store.go`：新增 `UsageSummaryByDateRange(planID, startISO, endISO)`，SQL `WHERE plan_id=? AND period_start >= ? AND period_start < ?`（半开区间）。
- `internal/web/quota_handlers.go` `UsagePage` 重写：基于 UTC 计算 今日 / 本周（周一起算）/ 本月 三个半开区间，逐 plan 汇总，传 `SumSections`（每段含 Title/Range/Summary）给模板。
- `qb_usage.html`：渲染三段汇总表（套餐名 + 各 token + 请求数 + 费用）。
- `internal/dashboard/seed.go`：补 3 条样例 usage log（今日 / 10 天前 / 40 天前），验证三段口径与日期过滤（40 天前的不计入本月）。

### P0-3 适合工具字段录入与展示
- `PlatformSave` 读取 `supports_tools_json`（checkbox == "1"）；`ModelSave` 读取 `tool_fit_json`（textarea）。
- `qb_platform_form.html` 增「支持工具调用 (Tools/JSON)」checkbox；`qb_model_form.html` 增「适合工具 (JSON)」textarea。
- 展示：`qb_config.html`（平台行「支持工具」列）、`qb_plans.html`（模型行「适合工具」列）、`qb_overview.html`（平台摘要「支持工具」列）。colspan 已同步。
- seed：火山方舟 `SupportsToolsJSON=true`、豆包 Coding `ToolFitJSON={"tools":true,"json_mode":true}`。

### P0-4 配置状态手动刷新（只读检测，不存敏感凭据）
- `internal/web/quota_handlers.go`：新增 `ConfigRefreshAction` + `probeBaseURL`。
- 对每个平台的 Base URL 做 `http.Client{Timeout:5s}.Get`，**不发送任何 API Key / Cookie / 账号密码**，仅按 HTTP 状态分类：2xx/3xx→healthy、401/403→warning(需要鉴权)、404→warning、5xx→danger、连接失败→danger。
- 写回 `qb_credential_statuses`：`GetCredentialStatusByPlatform` 有则 `UpdateCredentialStatus`、无则 `InsertCredentialStatus`。记录 detection_method=`http_probe`、detected_path=BaseURL、checked_at、message_redacted（如 `HTTP 401 (需要鉴权)`，无敏感信息）。
- 路由 `/qb/config/refresh`（POST，需 X-Requested-With）；`qb_config.html` 增「刷新检测」按钮 + 凭证记录表增「检测方式/消息」列。

### P0-5 web 502 / 静态资源异常排查
- **根因（已确认）**：非代码 bug，是测试环境代理。`server.go:42` 默认 host `0.0.0.0`，测试用 `http.Get("http://0.0.0.0:PORT/...")`；环境 `HTTP_PROXY=http://127.0.0.1:7897`，`NO_PROXY` 不含 `0.0.0.0` → Go transport 把请求转发给本地代理 → 代理连不上 0.0.0.0 → 返回 502。
- **修复方式**：运行 web 测试前关闭代理：`HTTP_PROXY=` `HTTPS_PROXY=`（或 `NO_PROXY` 含 `0.0.0.0`）。所有 `TestServer_*` 用例（ServesHTML/ServesStaticCSS/ServesStaticJS/EmbeddedAssets/GracefulShutdown/CSRF/Metrics）随后全部通过。
- **未改生产代码**：默认 host 维持 `0.0.0.0`（Docker 未设 ONWATCH_HOST，改默认会破坏容器可达性；config.go/main.go 在禁改区）。
- `qb_*` 测试不走真实 HTTP（用 template.ParseFS + buffer），原本就不受 502 影响。

### P0-6 文档同步
- 更新 `PROJECT_STATUS.md` / `TODO.md` / `CHANGELOG.md` / 本文件。

## 与 `docs/P0_URGENT_MVP_DEV_PLAN.md` 的对齐说明

本轮实现满足计划全部「急用验收标准」1-10。下列 3 处与计划「建议/最小字段」字面口径不同，属工程取舍，已满足核心要求：

1. **P0-1 字段集**：用既有 `UsageLog` schema（PlanID/ModelID/BucketScope/DateKey/PeriodStart/PeriodEnd/Input/Output/CacheRead/CacheWrite Tokens/RequestCount/CostValue/Source/SourceRef），未新增计划建议的 tool/project/status/note/bucket_id 列。理由：既有 schema 已覆盖核心录入与三段统计；`Source` 可承载工具来源；新增列属 schema 变更，P0 急用不必要，留 P1。
2. **P0-2 时区**：用 UTC 而非计划建议的「本地时区」。理由：台账层既有代码（seed/recommend/ListExpiringPlans）全用 UTC，混用本地时区会与 UTC period_start 产生区间错位。UTC 口径对「今日/本周/本月」语义一致且可复现。
3. **P0-4 检测方式**：用 HTTP 只读探测 Base URL（healthy/warning/danger），而非计划建议的「检测环境变量/配置项存在性」。理由：台账平台是用户手填 Base URL（火山方舟/MiMo 等），与 onWatch 采集层 env 变量无映射；HTTP 探测对台账场景更有意义，且不发凭据、不调付费 API、不网页登录，满足「只读检测 + 不存敏感凭据」核心要求。状态值用 healthy/warning/danger 而非 configured/missing，因探测的是可达性而非凭据存在性。

## R1-R5 返工（2026-07-04，按 docs/P0_URGENT_MVP_REVIEW.md）

GPT 首审 No-Go，要求 R1-R5 返工使代码与事实源一致。处理如下：

- **R1 PROJECT_STATUS 冲突**：开头改「P0 急用闭环已完成，等待 GPT 复审 / 合并」；删除「不能完整交付」与 `p0_complete` 并存；「下一步」改「等待 GPT 复审；通过后合并，再进 P1」。结论/阶段/缺口/下一步不再矛盾。
- **R2 P0-2 总览页三段（选 A 补代码）**：store 新增 `UsageSummaryTotalByDateRange(start,end)`（跨所有 plan 聚合，半开区间）；`OverviewPage` 计算 UTC 今日/本周(周一)/本月；`qb_overview.html` 新增「消耗统计」section，三卡片展示输入/输出/缓存读取/缓存写入/费用。使用记录页三段保留不变。
- **R3 P0-3 文案降级（选 B 不改 schema）**：平台 `supports_tools_json`(bool)=「支持工具调用」；模型 `tool_fit_json`(text)=「适合工具标签」。UI 文案统一：平台列/总览列「支持工具调用」、模型列「适合工具标签」、模型表单「适合工具标签 (JSON)」。DEV_PLAN P0-3 段加「采用口径」。
- **R4 P0-4 文档（选 B）**：DEV_PLAN P0-4 段加「采用口径」：Base URL 只读 HTTP 探测，不发凭据、不调付费 API、不网页登录、只存可达状态+脱敏消息；说明会外联用户录入的 Base URL，非余额抓取非 Provider 自动适配。代码未改。
- **R5 PR 摘要**：PR #2 描述改纯摘要（目标/事实源/改动摘要/验证摘要/风险摘要），取舍与口径写入事实源文档。

返工改动文件：`internal/store/quota_board_store.go`、`internal/web/quota_handlers.go`、`qb_overview.html`、`qb_config.html`、`qb_plans.html`、`qb_model_form.html` + 6 份文档（AGENT_HANDOFF/PROJECT_STATUS/TODO/CHANGELOG/DEV_PLAN/REVIEW）。

## 验证结果（本地，Go 1.26.4，Windows）

环境准备：Go 1.26.4 解压到 `~/.local/go`（zip 用 .NET ZipFile 解压，Expand-Archive 过慢会超时致 src 残缺）；`GOPROXY=https://goproxy.cn,direct`；web 测试前 `HTTP_PROXY=$null HTTPS_PROXY=$null NO_PROXY=localhost,127.0.0.1,::1,0.0.0.0`。

```
go build ./...                                          EXIT=0 ✅
go vet  ./internal/store ./internal/dashboard ./internal/web   通过 ✅
go test ./internal/store ./internal/dashboard          ok (store cached, dashboard 无测试) ✅
go test ./internal/web                                  仅 1 个预存失败（见下），502 全部修复 ✅
```

`go test ./internal/web` 唯一失败：`TestHandlerTryAutoDetectAdditionalCoverage/anthropic_success_from_credentials_file`（`handlers_coverage_test.go:286`）。**已用 `git stash` 证实为预存问题**：测试 `t.Setenv("HOME", tmp)` 写 `~/.claude/.credentials.json`，但 Windows 上 `os.UserHomeDir()` 读 `USERPROFILE` 而非 `HOME` → 探测不到 → 失败。与台账改动无关，属 onWatch 原有测试的 Windows 平台不兼容。

`go test ./...` 其余失败均为 onWatch 原有测试的 Windows/Unix 平台不兼容，均经 stash 证实预存：
- `internal/api` [build failed]：`extra_coverage_test.go` 引用未定义 `getCredentialsFilePath`。
- 根包 [build failed]：`root_more_coverage_test.go:183` 用 `attr.Setsid`（Unix-only，Windows 的 syscall.SysProcAttr 无此字段）。
- `internal/update`：systemd 相关（Restart/FallbackSystemctlRestart/MigrateSystemdUnit）。
- `internal/config`：日志轮转 / env 加载（HOME/路径语义）。
- `internal/agent`：statusline bridge / codex agent manager / anthropic OAuth。
- `internal/testutil/cmd/mockserver`：信号启停。

通过包：`internal/store`、`internal/metrics`、`internal/notify`、`internal/menubar`、`internal/tracker`、`internal/api_integrations`、`internal/testutil`。

### 端到端冒烟（HOME 隔离，admin/changeme，curl.exe 直连 127.0.0.1）
```
login:           200 (21720B)
seed:            200 (9356B, 跳总览)
/qb/usage:       200 (16465B)  含「今日消耗/本周消耗/本月消耗/用量明细/新增用量记录」
/qb/usage/new:   200 (9679B)   表单渲染
/qb/config:      200 (9733B)
/qb/config/refresh (POST): 200 (26B = {"redirect":"/qb/config"})
/qb/config (刷新后): 200 (10167B, +434B = 检测记录写回 qb_credential_statuses)
/qb/plans:       200 (34886B) 含「适合工具」列
```
rg 验证：usage.html 含 `今日消耗|本周消耗|本月消耗|用量明细|新增用量记录`；plans.html 含 `适合工具`（3 处）。

### 返工复验（R1-R5 后）
```
go build ./...                                   EXIT=0 ✅
go test ./internal/store ./internal/dashboard   ok ✅
go test ./internal/web -run 'TestQB|TestServer_ServesHTML'   全 PASS ✅
```
仅 `TestHandlerTryAutoDetectAdditionalCoverage` 仍失败（Windows HOME 预存，与本轮无关）。`TestQBOverviewRendersWithSeed` PASS（R2 模板加 `UsagePeriods` 后对 nil 安全）。
HOME 隔离冒烟 `/qb/`（12741B）rg 含 `消耗统计|今日|本周|本月|支持工具调用` —— 总览页三段消耗与 R3 文案运行时确认。

### 自审（reviewer agent，Go/No-Go = Go）
禁改区零触碰（store.go 现有 DDL/agent/api/config.go/main.go/app.js/style.css/现有 *_test.go 均未改）。P0-1 表单 name 与 handler 读取完全一致；*int64 先判 nil 再解引用；P0-2 周一起算 `(Weekday()+6)%7` 正确、半开区间 SQL 正确；P0-4 不发凭据、upsert 正确。findings：onwatch-smoke.exe 未忽略（已删 + 已加 .gitignore）、P0-6 文档（本次补齐）；建议项（probeBaseURL 加 CheckRedirect/MaxBytesReader、UsagePage 边界亚秒、qb_usage_form 加入 TestQBTemplatesParse 清单）非阻断，留 P2。

## 禁改区（始终遵守，本轮零触碰）

`internal/store/store.go` 现有表 DDL / `migrateSchema()` / 现有方法、`internal/agent/*` / `internal/api/*` / `internal/config/config.go`、`internal/web` 现有 handler / 路由 / middleware / app.js / style.css、`main.go`、现有 `*_test.go`。

## 技术问题记录

1. 环境无 Go → 装 Go 1.26.4 到 `~/.local/go`（winget 下载被代理干扰失败，改用 `Invoke-WebRequest -Proxy http://127.0.0.1:7897` 下载 zip，.NET `ZipFile.ExtractToDirectory` 解压）。
2. Go 代理 → `GOPROXY=https://goproxy.cn,direct`，`HTTP_PROXY/HTTPS_PROXY` 置空。
3. web 测试 502 根因见 P0-5：测试前必须 `HTTP_PROXY=$null HTTPS_PROXY=$null NO_PROXY` 含 `0.0.0.0`。
4. `--test` 不隔离 DB（忽略 `ONWATCH_DB_PATH`）→ 端到端用 `HOME=/tmp/xxx` 隔离；onWatch 会 daemonize（父进程立即退出，真正服务在子进程 PID），Stop-Process 要按进程名杀全部。
5. PS 5.1 `Invoke-WebRequest` 走系统代理不走 NO_PROXY 环境变量 → 冒烟测试用 `curl.exe`（尊重 NO_PROXY）。
6. PowerShell 中 `$name:` 会被解析为驱动器作用域，字符串插值用 `${name}:`。

## 已知遗留（P1/P2，非阻断）

- onWatch 原有测试在 Windows 的平台不兼容（Setsid/systemd/HOME 语义/getCredentialsFilePath），属底座问题，非台账 P0 责任。
- G5 `mustParseTime` 静默吞解析错误（P2）。
- G6 `ListExpiringPlans` 字符串比较依赖时间格式统一（P2 可改 datetime-local）。
- probeBaseURL 默认跟随重定向、未限流（P2 加 CheckRedirect + MaxBytesReader）。
- UsagePage 边界为整秒，亚秒 period_start 在边界处有极低概率漏入上一区间（与既有 UsageSummaryByPeriod 同模式，P2）。
- qb_usage_form.html 未加入 `TestQBTemplatesParse` 清单（NewHandler 启动已解析，P2 补清单）。

## 下一步

P0 台账层已交付，可进入：
- Provider 自动适配（P1）
- `ccusage` 导入
- 趋势图
- 推荐增强

## 技术问题处理规则

纯技术问题自行整改并写回本文件。产品问题先确认用户。
