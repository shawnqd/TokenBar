# P0 急用连续开发任务书

更新时间：2026-07-04

## 结论

本分支目标是把模型额度看板从“P0 骨架可看”推进到“个人急用可用”。

当前只做 P0 补缺，不进入 P1。执行方按本文档连续开发，不能自行扩大范围。

## 当前分支

```text
feature/p0-urgent-mvp
```

## 协作模式

| 角色 | 职责 |
|---|---|
| 用户 | 定目标、边界、取舍、最终确认 |
| GPT | 提供开发任务书、交接文档、审查口径 |
| Code / OpenCode | 具体实现、最小验证、回写状态文档 |
| Codex | 后续额度恢复后可审本地分支 |

## 硬规则

1. 不直接在 `main` 开发。
2. 本轮只使用 `feature/p0-urgent-mvp` 一个开发分支。
3. 不新开多个 PR。
4. 不触发 GitHub Actions；远程提交信息必须带 `[skip ci]`。
5. 不保存真实密钥、会话凭据、账号密码或任何敏感凭据。
6. 不做网页登录余额抓取。
7. 不进入 P1 Provider 自动适配。
8. 不重构 onWatch 原始核心模块。
9. 技术问题自行修复并写回交接文档；产品取舍问题先停下确认。

## 禁改区

除非修复 P0 明确阻塞，否则不要改：

```text
internal/agent/*
internal/api/*
internal/config/config.go
main.go
onWatch 原始核心数据结构
原有非 qb_* 业务表 schema
```

`internal/web` 可做最小修复，但必须保证原 onWatch 页面仍能访问。

## 本轮 P0 任务顺序

### P0-1：UsageLog 手动记录闭环

目标：用户可以手动新增、编辑、删除一次模型使用记录。

建议路由：

```text
GET  /qb/usage/new
GET  /qb/usage/edit?id=<id>
POST /qb/usage/save
POST /qb/usage/delete
```

最小字段：

| 字段 | 说明 |
|---|---|
| platform_id | 平台 |
| plan_id | 套餐，可为空 |
| bucket_id | 额度桶，可为空 |
| model_id | 模型，可为空 |
| tool | 使用工具，如 Codex / OpenCode / Claude Code / API |
| project | 项目名，可为空 |
| input_tokens | 输入 token |
| output_tokens | 输出 token |
| cache_read_tokens | 缓存命中输入 token |
| cache_write_tokens | 缓存未命中输入 token |
| cost_amount | 本次费用，可为空 |
| status | success / failed / unknown |
| note | 备注 |
| used_at | 使用时间 |

要求：

1. 使用记录页必须有“新增记录”入口。
2. 每条记录必须有编辑入口。
3. 删除必须走 POST。
4. 表单提交成功后回到使用记录页。
5. 不要求自动采集，不要求导入 ccusage。

### P0-2：today / week / month 消耗口径

目标：总览页和使用记录页都能看到今日、本周、本月消耗。

口径：

| 指标 | 口径 |
|---|---|
| today | 本地时区当天 00:00 到当前 |
| week | 本地时区本周一 00:00 到当前 |
| month | 本地时区本月 1 日 00:00 到当前 |

要求：

1. 不再用“最近 30 天”冒充本月。
2. 三段统计至少包含：输入、输出、缓存命中、缓存未命中、费用。
3. 今日 / 本周 / 本月在 UI 上必须分开展示。
4. store 层如已有 `UsageSummaryByPeriod`，优先复用；缺口用最小新增方法补齐。

### P0-3：适合工具字段录入与展示

目标：能标记平台 / 模型适合哪些工具，方便用户选择。

已有字段：

```text
platform.supports_tools_json
model.tool_fit_json
```

最小实现：

1. 平台表单增加“适合工具”输入框。
2. 模型表单增加“适合工具”输入框。
3. 输入格式先用逗号分隔文本即可。
4. 保存时可转成 JSON 数组；如果现有代码更适合原样文本，也必须保证前后兼容。
5. 套餐页、模型列表、总览页至少一个关键位置展示适合工具。

建议工具标签：

```text
Codex, OpenCode, Claude Code, API, ChatGPT, Web, Mobile
```

### P0-3 采用口径（R3 返工确认）

P0 不为平台层新增 schema，降级为两层分工：

- 平台层 `supports_tools_json`（bool）：只表示「是否支持工具调用」，UI 文案统一为「支持工具调用」。
- 模型层 `tool_fit_json`（text/JSON）：维护「具体适合的工具标签」，UI 文案统一为「适合工具标签」。
- 不再用「适合工具」泛指平台，避免误导。
- 具体工具标签的逗号分隔→JSON 转换留 P1；P0 接受原样文本/JSON。

### P0-4：配置状态手动刷新

目标：配置状态页从“展示态”变成“可手动刷新检测”。

要求：

1. 页面增加“刷新检测”按钮。
2. 只做本地只读检测。
3. 不保存真实凭据，只保存状态。
4. 检测结果写回 `qb_credential_statuses`。

状态字段建议：

| 字段 | 说明 |
|---|---|
| status | configured / missing / unknown |
| last_checked_at | 检测时间 |
| message | 简短说明 |

检测范围：

1. 环境变量是否存在。
2. 本地配置项是否存在。
3. 不调用付费 API。
4. 不发起网页登录请求。

### P0-4 采用口径（R4 返工确认）

P0 采用「Base URL 只读可达性探测」，不采用环境变量/本地配置存在性检测：

- 对用户录入的每个平台 Base URL 做 `HTTP GET`（5s 超时），**不发送任何 API Key / Cookie / 账号密码**。
- 按状态码分类：2xx/3xx→healthy、401/403→warning(需要鉴权)、404→warning、5xx→danger、连接失败→danger。
- 写回 `qb_credential_statuses`（upsert）：detection_method=`http_probe`、detected_path=BaseURL、message_redacted（如 `HTTP 401 (需要鉴权)`，无敏感信息）。
- 约束：不发凭据、不调用付费 API、不网页登录、只保存可达状态和脱敏消息。
- 这不是余额抓取，也不是 Provider 自动适配。
- 会访问用户录入的 Base URL（外联）；如后续要求完全禁止外联，再改回本地配置存在性检测。

### P0-5：修复 internal/web 502 / 静态资源测试失败

目标：Web 层测试不再因为 502 或静态资源异常阻塞 P0。

要求：

1. 先复现失败测试。
2. 定位是 handler、静态资源、metrics、middleware 还是测试预期问题。
3. 只做最小修复。
4. 不重写 web server。
5. 原 onWatch 页面不能坏。

### P0-6：状态文档同步

每完成一个小阶段，必须同步：

```text
AGENT_HANDOFF.md
TODO.md
PROJECT_STATUS.md
CHANGELOG.md
```

同步要求：

1. 已完成项勾选。
2. 验证命令和结果写清楚。
3. 未完成项明确是否阻塞急用。
4. 不把未完成项写成完成。

## 本地验证命令

优先执行：

```bash
go test ./internal/store ./internal/dashboard ./internal/web
```

然后执行：

```bash
go test ./...
go build ./...
```

如测试失败，必须写入：

1. 失败命令。
2. 失败包。
3. 失败原因。
4. 是否阻塞急用。
5. 下一步最小修复建议。

## 急用验收标准

本轮完成后，必须满足：

1. 能手动录入平台、套餐、额度桶、模型、使用记录。
2. 能编辑和删除使用记录。
3. 首页或使用记录页能看到今日 / 本周 / 本月消耗。
4. 能看到平台 / 模型适合哪些工具。
5. 配置状态页能手动刷新检测。
6. 不保存真实敏感凭据。
7. Web 页面不出现 502。
8. 原 onWatch 页面仍可访问。
9. 状态文档与代码状态一致。
10. 没有触发 GitHub Actions。

## 明确不做

本轮不做：

1. Provider 自动适配。
2. 第三方网页登录余额抓取。
3. ccusage 导入。
4. 趋势图。
5. 多用户。
6. 云同步。
7. UI 大改版。
8. 自动计费换算优化。

## Code / OpenCode 执行指令

```text
拉最新 main，切换到 feature/p0-urgent-mvp。
先阅读 docs/P0_URGENT_MVP_DEV_PLAN.md、AGENT_HANDOFF.md、TODO.md、PROJECT_STATUS.md。
按 P0-1 到 P0-6 顺序连续实现。
不要新开 PR，不要触发 GitHub Actions，不要扩大到 P1。
每完成一个小阶段，更新四份状态文档。
最后跑本地验证并把结果写进 AGENT_HANDOFF.md。
```
