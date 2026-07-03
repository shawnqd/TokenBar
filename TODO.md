# TODO

## P0 启动清单

### 包 0：结构审查 ✅

- [x] 引入 `onWatch` fork / 工作副本到当前仓库（合并上游 onllm-dev/onwatch）
- [x] 阅读 `README.md`（onWatch 原始 README 存档为 docs/ONWATCH_README.md）
- [x] 阅读 `docs/API_INTEGRATIONS_SETUP.md`
- [x] 审查 `internal/store/store.go`
- [x] 审查 `internal/agent/*`
- [x] 审查 `internal/web/*`
- [x] 审查 `internal/config/*`
- [x] 输出改造点清单
- [x] 输出禁改区域清单
- [x] 输出阶段 1 涉及文件清单

验收：
- 能明确“新表放哪、路由放哪、模板放哪、哪些老表和老路由不能动”

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

验收：
- 数据迁移成功
- 原有表和数据不坏
- 样例数据可读可查

### 包 2：P0 页面骨架 ✅

- [x] 新增总览页
- [x] 新增平台/套餐页
- [x] 新增 Provider 状态页
- [x] 新增使用记录页
- [x] 新增配置状态页
- [x] 新增风险备注页
- [x] 新增导入/导出页
- [x] 接入样例数据展示

验收：
- 新页面可打开
- 样例数据能显示
- 原 `onWatch` 页面仍可访问

### 包 3：P0 最小闭环 ✅

- [x] 手动录入平台
- [x] 手动录入套餐
- [x] 手动录入额度桶
- [x] 只读配置检测
- [x] 首页推荐规则最小版
- [x] 快到期提示
- [x] 低额度提示
- [x] 风险提示
- [x] 脱敏导出
- [x] 本地导入

验收：
- 能走通一次录入、查看、导出、导入、再次查看

### 包 3.5：自审缺口修复 ✅

- [x] S1 导入 CSRF 闭环（qb_import_export.html fetch + X-Requested-With）
- [x] S2 当前模型管理 UI（Model 4 handler + qb_model_form.html + PlansPage 模型列表）
- [x] G1 风险备注编辑（RiskEditForm + 路由 + 编辑按钮 + resolved_at）
- [x] G2 ImportData ID 重建（DeleteAllQB + 7 Insert*WithID，防 FK 孤儿）
- [x] G3 导航高亮（Nav → import-export）
- [x] G4 桶编辑入口（qb_plans.html 桶行编辑链接）
- [x] 端到端复验（HOME 隔离）：模型录入 + 导入 CSRF + ID 重建

### 包 3.6：补充审查缺口

- [ ] 新增 `UsageLog` 录入 / 编辑 / 删除路由与表单
- [ ] 将使用记录页拆成 `today / week / month` 明确口径
- [ ] 配置状态页增加真实只读检测刷新动作
- [ ] 将检测结果写回 `qb_credential_statuses`
- [ ] 平台 / 模型补录“适合工具”字段
- [ ] 总览 / 配置 / 套餐页展示“适合工具”
- [ ] 排查 `internal/web` 测试中的 `502` 与静态资源返回异常
- [ ] 同步 `PROJECT_STATUS.md` / `CHANGELOG.md` / `AGENT_HANDOFF.md`

## 非本轮

- [ ] Provider 自动适配
- [ ] Cookie / 网页抓取
- [ ] 趋势图
- [ ] 云同步
- [ ] 多用户
