# 开发交接单

## 背景

基于 onWatch 改造的「模型额度看板」P0 主骨架已完成，但补充审查确认仍有若干未完成项。底座 onWatch 已入仓，台账层与采集层隔离。

## 当前状态：P0 补缺阶段

阶段 0-3 和首轮自审修复已完成，但仍未达到“P0 完整交付”。

## 已交付清单

### 数据层（internal/store/）
- `quota_board_store.go`：7 struct + 43 方法 + DeleteAllQB + 7 Insert*WithID（导入用 INSERT OR REPLACE 保留 ID）
- `store.go` `createTables()` 追加 7 张 qb_* 表 DDL

### 业务层（internal/dashboard/）
- `import_export.go`：ExportData / ImportData（overwrite 用 DeleteAllQB + WithID；upsert 用 WithID INSERT OR REPLACE）
- `seed.go`：SeedSampleData
- `recommend.go`：Recommend

### Web 层（internal/web/）
- `quota_handlers.go`：7 页面 GET + 3 action + 17 录入 handler + 5 模型/风险编辑 handler
- 模板：7 页面 + 5 表单（platform/plan/bucket/risk/model）+ layout 导航
- `server.go` +30 路由，`handlers.go` +12 模板字段

## 自审修复记录

| 项 | 问题 | 修复 |
|---|---|---|
| S1 | 导入原生 form 无 X-Requested-With → 403 | `qb_import_export.html` 改 fetch；ImportAction 返回 JSON |
| S2 | 当前模型管理 UI 缺失 | Model 4 handler + `qb_model_form.html` + PlansPage 模型列表 |
| G1 | 风险备注编辑死代码 | RiskEditForm + 路由 + 编辑按钮 + resolved_at |
| G2 | ImportData 不保留 ID → FK 孤儿 | DeleteAllQB + 7 Insert*WithID |
| G3 | 导航高亮不匹配 | Nav → import-export |
| G4 | 桶列表无编辑入口 | `qb_plans.html` 桶行编辑链接 |

## 端到端复验（HOME 隔离）

login → seed → 模型录入（JSON redirect）→ 套餐页显示 → 导出 9357B → 导入（X-Requested-With + multipart 返回 JSON result: platforms=4 plans=4 models=5）→ 导入后数据完整（火山方舟 + E2EModel 仍在，ID 重建无孤儿）。

## 禁改区（始终遵守）

`internal/store/store.go` 现有表 DDL / `migrateSchema()` / 现有方法、`internal/agent/*` / `internal/api/*` / `internal/config/config.go`、`internal/web` 现有 handler / 路由 / middleware / app.js / style.css、`main.go`、`*_test.go`

## 技术问题记录

1. 环境无 Go → 装 Go 1.26.4 到 `~/.local/go`，用 `~/.local/go/bin/go`
2. Go 代理 → `env -u HTTP_PROXY -u HTTPS_PROXY GOPROXY=https://goproxy.cn,direct`
3. onWatch `--test` 不隔离 DB（忽略 `ONWATCH_DB_PATH`，用 `~/.onwatch/data/onwatch.db`）→ 端到端验证用 `HOME=/tmp/xxx` 隔离
4. `qb_plans.html` `{{index $.ModelsByPlatform $platID}}` 对 nil 不健壮 → handler 总传 map 即可（测试传空 map）

## 当前必须补齐（仍属 P0）

1. `UsageLog` 录入 / 编辑 / 删除闭环
2. 使用记录页拆分 `today / week / month` 明确口径
3. 配置状态页接入真实只读检测刷新动作
4. `qb_credential_statuses` 写回真实检测结果
5. “适合工具”字段进入平台 / 模型录入和展示
6. 排查 `internal/web` 测试里的 `502` 与静态资源响应异常
7. 更新状态文档，撤销 `p0_complete` 错误口径

## 已知遗留（P1/P2，非阻断）

- G5 `mustParseTime` 静默吞解析错误（健壮性，P2）
- G6 `ListExpiringPlans` 字符串比较依赖时间格式统一（`plan_form` 的 starts_at/expires_at 是文本输入，P2 可改 datetime-local）
- G7 `SeedAction` 是 GET 触发状态变更（有幂等守卫，CSRF 卫生 P2）
- ImportAction 返回 JSON 后模板 `{{if .ImportResult}}` 块成死代码（无害，P2 清理）

## 下一步

先补齐上述 P0 缺口，再进入：

- Provider 自动适配
- `ccusage` 导入
- 趋势图
- 推荐增强

## 技术问题处理规则

纯技术问题自行整改并写回本文件。产品问题先确认用户。
