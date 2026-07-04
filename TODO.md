# TODO

更新时间：2026-07-04

## 当前执行模式

```text
P0 急用连续开发模式
分支：feature/p0-urgent-mvp
事实源：docs/P0_URGENT_MVP_DEV_PLAN.md + AGENT_HANDOFF.md + TODO.md + PROJECT_STATUS.md + CHANGELOG.md
```

硬规则：

- [ ] 不直接在 `main` 开发
- [ ] 本轮只使用 `feature/p0-urgent-mvp` 一个开发分支
- [ ] 不新开多个 PR
- [ ] 不触发 GitHub Actions
- [ ] 不保存真实敏感凭据
- [ ] 不进入 P1 Provider 自动适配

## P0 启动清单

### 包 0：结构审查 ✅

- [x] 引入 `onWatch` fork / 工作副本到当前仓库
- [x] 阅读 `README.md`
- [x] 阅读 `docs/API_INTEGRATIONS_SETUP.md`
- [x] 审查 `internal/store/store.go`
- [x] 审查 `internal/agent/*`
- [x] 审查 `internal/web/*`
- [x] 审查 `internal/config/*`
- [x] 输出改造点清单
- [x] 输出禁改区域清单
- [x] 输出阶段 1 涉及文件清单

验收：能明确“新表放哪、路由放哪、模板放哪、哪些老表和老路由不能动”。

### 包 1：P0 数据骨架 ✅

- [x] 新增 `Platform`
- [x] 新增 `Plan`
- [x] 新增 `QuotaBucket`
- [x] 新增 `Model`
- [x] 新增 `UsageLog`
- [x] 新增 `CredentialStatus`
- [x] 新增 `RiskNote`
- [x] 增加迁移入口
- [x] 设计脱敏导入导出 schema
- [x] 准备最小样例数据

### 包 2：P0 页面骨架 ✅

- [x] 新增总览页
- [x] 新增平台 / 套餐页
- [x] 新增 Provider 状态页
- [x] 新增使用记录页
- [x] 新增配置状态页
- [x] 新增风险备注页
- [x] 新增导入 / 导出页
- [x] 接入样例数据展示

### 包 3：P0 最小闭环 ✅

- [x] 手动录入平台
- [x] 手动录入套餐
- [x] 手动录入额度桶
- [x] 只读配置检测展示
- [x] 首页推荐规则最小版
- [x] 快到期提示
- [x] 低额度提示
- [x] 风险提示
- [x] 脱敏导出
- [x] 本地导入

### 包 3.5：自审缺口修复 ✅

- [x] S1 导入 CSRF 闭环
- [x] S2 当前模型管理 UI
- [x] G1 风险备注编辑
- [x] G2 ImportData ID 重建
- [x] G3 导航高亮
- [x] G4 桶编辑入口
- [x] 端到端复验（HOME 隔离）：模型录入 + 导入 CSRF + ID 重建

## 包 3.6：P0 急用补缺（本轮）

### P0-1 UsageLog 手动记录闭环

- [ ] 新增 `GET /qb/usage/new`
- [ ] 新增 `GET /qb/usage/edit?id=<id>`
- [ ] 新增 `POST /qb/usage/save`
- [ ] 新增 `POST /qb/usage/delete`
- [ ] 新增或补齐使用记录表单模板
- [ ] 表单支持平台、套餐、额度桶、模型选择
- [ ] 表单支持工具、项目、备注
- [ ] 表单支持输入 token、输出 token、缓存命中输入、缓存未命中输入
- [ ] 表单支持费用、状态、使用时间
- [ ] 使用记录列表增加新增 / 编辑 / 删除入口
- [ ] 删除必须走 POST
- [ ] 保存成功后回到使用记录页

验收：能手动新增、编辑、删除一条 UsageLog。

### P0-2 today / week / month 消耗口径

- [ ] 明确 today 口径：本地当天 00:00 到当前
- [ ] 明确 week 口径：本周一 00:00 到当前
- [ ] 明确 month 口径：本月 1 日 00:00 到当前
- [ ] store / dashboard 层能分别查询三段统计
- [ ] 使用记录页展示今日 / 本周 / 本月
- [ ] 总览页同步展示今日 / 本周 / 本月
- [ ] 统计字段至少包含输入、输出、缓存命中、缓存未命中、费用
- [ ] 移除或降级“最近 30 天冒充本月”的展示口径

验收：页面能明确看到今日、本周、本月三段统计。

### P0-3 适合工具字段录入与展示

- [ ] 平台表单增加“适合工具”输入
- [ ] 模型表单增加“适合工具”输入
- [ ] 保存到 `supports_tools_json` / `tool_fit_json`
- [ ] 支持逗号分隔文本输入
- [ ] 套餐页展示平台 / 模型适合工具
- [ ] 模型列表展示适合工具
- [ ] 总览页至少一处展示推荐相关工具信息

验收：能录入并看到 Codex / OpenCode / Claude Code / API 等工具标签。

### P0-4 配置状态手动刷新

- [ ] 配置状态页增加“刷新检测”按钮
- [ ] 新增只读检测 action
- [ ] 只检测本地环境变量 / 本地配置是否存在
- [ ] 不保存真实敏感凭据
- [ ] 检测结果写回 `qb_credential_statuses`
- [ ] 状态仅保存 configured / missing / unknown
- [ ] 保存 last_checked_at
- [ ] 保存 message
- [ ] 页面刷新后显示最新检测结果

验收：点击刷新后，配置状态页能看到检测时间和 configured / missing / unknown。

### P0-5 internal/web 502 与静态资源修复

- [ ] 复现 `internal/web` 测试失败
- [ ] 定位 502 来源
- [ ] 定位静态资源异常来源
- [ ] 定位 HTML / metrics 异常来源
- [ ] 做最小修复
- [ ] 保证原 onWatch 页面仍可访问
- [ ] 更新相关测试或修正错误预期

验收：`go test ./internal/web` 不再因 502 / 静态资源异常失败；若仍失败，必须写明是否阻塞急用。

### P0-6 状态文档同步

- [ ] 更新 `AGENT_HANDOFF.md`
- [ ] 更新 `PROJECT_STATUS.md`
- [ ] 更新 `TODO.md`
- [ ] 更新 `CHANGELOG.md`
- [ ] 写入本地验证命令和结果
- [ ] 写入未完成项和风险
- [ ] 不把未完成项标记为完成

## 本地验证

优先：

```bash
go test ./internal/store ./internal/dashboard ./internal/web
```

然后：

```bash
go test ./...
go build ./...
```

如失败，必须记录：

- [ ] 失败命令
- [ ] 失败包
- [ ] 失败原因
- [ ] 是否阻塞急用
- [ ] 下一步最小修复建议

## 急用验收标准

- [ ] 可以手动录入平台、套餐、额度桶、模型、使用记录
- [ ] 可以编辑和删除使用记录
- [ ] 首页或使用记录页能看到今日 / 本周 / 本月消耗
- [ ] 能看到平台 / 模型适合哪些工具
- [ ] 配置状态能手动刷新检测
- [ ] 不保存真实敏感凭据
- [ ] Web 页面不出现 502
- [ ] 原 onWatch 页面仍可访问
- [ ] 状态文档与代码状态一致
- [ ] 没有触发 GitHub Actions

## 非本轮

- [ ] Provider 自动适配
- [ ] 第三方网页登录余额抓取
- [ ] `ccusage` 导入
- [ ] 趋势图
- [ ] 云同步
- [ ] 多用户
- [ ] UI 大改版
