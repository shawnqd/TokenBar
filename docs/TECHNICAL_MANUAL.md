# 技术手册

## 1. 技术总决策

继续使用 `onWatch` 作为运行底座，但新增一层独立的“台账 / 看板领域层”，不要把产品实体直接塞进现有 Provider 采集表。

设计原则：

1. 保留 `onWatch` 原有能力
2. 新增功能优先走新表、新页面、新路由
3. 采集层和台账层分离
4. P0 先支持手动台账和导入导出
5. P1 再接 Provider 自动适配

## 2. 参考项目审查结论

### onWatch

可复用：

- 本地 Dashboard
- SQLite
- 后台轮询结构
- Provider 开关
- 告警系统
- API Integrations 本地 JSONL 摄取思路
- Windows 支持路径

缺口：

- 没有平台 / 套餐 / 风险备注等产品级实体
- 现有 UI 以 Provider 监控为中心，不是套餐管理工具
- 现有配置路径偏向真实密钥和认证读取

风险：

- GPL-3.0
- Beta 状态
- 外部接口可能变更

### CodexBar

可参考：

- Provider 目录设计
- Base URL 安全校验
- DeepSeek / Doubao / LiteLLM / Alibaba / Ollama / MiMo / OpenCode Go 的实现思路

不要直接迁：

- macOS 菜单栏 UI
- Keychain / WebKit / 浏览器 Cookie 导入

### OpenCode Bar

可参考：

- `auth.json` 发现逻辑
- `OpenCode Zen` CLI 统计方式
- `OpenCode Go` 的 workspace / auth 发现方式

不要直接迁：

- macOS App 工程
- 状态栏逻辑
- Keychain 和浏览器权限处理

### ccusage

可参考：

- 本地使用记录统计口径
- `daily / weekly / monthly / session` 聚合方式
- `--json` 输出结构
- OpenCode 本地 SQLite / JSON 回退思路

不要直接迁：

- 它不是台账系统，没有套餐、配置状态、风险备注等产品实体

## 3. 仓库改造原则

后续 Code / OpenCode 开始改造时，先审查以下范围：

- `README.md`
- `docs/API_INTEGRATIONS_SETUP.md`
- `internal/store/store.go`
- `internal/agent/*`
- `internal/web/*`
- `internal/config/*`

不要在未审查结构前直接改动：

- 现有 Provider 表的语义
- 现有采集 Agent 的关键路径
- 现有 Dashboard 的原路由行为

## 4. 数据域拆分

建议拆成两层：

### 采集层

沿用 `onWatch` 当前结构，继续承载：

- Provider 快照
- Reset cycle
- API integration event
- Provider account

### 台账层

新增本项目自己的实体：

- Platform
- Plan
- QuotaBucket
- Model
- UsageLog
- CredentialStatus
- RiskNote
- ImportExportSchema

## 5. 推荐数据模型

### Platform

- `id`
- `name`
- `vendor`
- `category`
- `base_url`
- `credential_status`
- `supports_tools_json`
- `default_risk_level`
- `is_active`
- `notes`
- `created_at`
- `updated_at`

### Plan

- `id`
- `platform_id`
- `name`
- `plan_type`
- `starts_at`
- `expires_at`
- `renewal_policy`
- `status`
- `priority`
- `recommended_role`
- `risk_summary`
- `created_at`
- `updated_at`

### QuotaBucket

- `id`
- `plan_id`
- `scope`
- `metric`
- `limit_value`
- `remaining_value`
- `used_value`
- `window_start`
- `window_end`
- `reset_at`
- `source`
- `confidence_level`
- `notes`
- `created_at`
- `updated_at`

### Model

- `id`
- `platform_id`
- `plan_id`
- `model_id`
- `display_name`
- `family`
- `is_current`
- `base_url_override`
- `tool_fit_json`
- `status`
- `created_at`
- `updated_at`

### UsageLog

- `id`
- `plan_id`
- `model_id`
- `bucket_scope`
- `date_key`
- `period_start`
- `period_end`
- `input_tokens`
- `output_tokens`
- `cache_read_tokens`
- `cache_write_tokens`
- `request_count`
- `cost_value`
- `source`
- `source_ref`
- `created_at`

### CredentialStatus

- `id`
- `platform_id`
- `status`
- `detection_method`
- `detected_path`
- `checked_at`
- `message_redacted`
- `base_url`

### RiskNote

- `id`
- `target_type`
- `target_id`
- `level`
- `title`
- `content`
- `source`
- `expires_at`
- `resolved_at`
- `created_at`
- `updated_at`

## 6. 关键派生指标

以下字段建议按查询计算，不强制持久化：

- `days_to_expire`
- `quota_health_status`
- `today_recommendation_score`
- `is_expiring_soon`
- `is_low_quota`
- `is_high_risk`
- `today_usage`
- `week_usage`
- `month_usage`

## 7. 页面信息架构

### 总览页

显示：

- 今日推荐套餐 / 模型
- 快到期套餐
- 低额度套餐
- 风险告警
- 各平台摘要

P0：

- 推荐卡
- 风险卡
- 平台摘要

后置：

- 趋势图
- 推荐规则解释详情

### 平台 / 套餐页

显示：

- 平台列表
- 套餐列表
- 到期时间
- 重置时间
- 当前模型
- Base URL
- 适合工具

P0：

- 新增 / 编辑 / 停用
- 配额桶录入

### Provider 状态页

显示：

- CredentialStatus
- 检测路径
- 最近检查时间
- 可用模型

P0：

- 只读检测
- 人工标记状态

### 使用记录页

显示：

- 今日 / 本周 / 本月消耗
- 按平台 / 套餐 / 模型汇总
- 导入来源

P0：

- 汇总表
- 手动录入
- 批量导入

当前实现备注（2026-07-03 审查）：

- store 层已具备 `UsageLog` CRUD
- 页面当前只有展示和汇总
- `手动录入` 尚未真正落到 handler / 路由 / 表单闭环
- 当前汇总口径偏向最近 30 天，不应替代 `today / week / month`

### 配置状态页

显示：

- Base URL
- API Key 状态
- 工具适配关系
- 本地配置发现结果

P0：

- 状态查看
- 状态刷新

当前实现备注（2026-07-03 审查）：

- `状态查看` 已落地
- `状态刷新` 仍未落地为真实只读检测动作
- 当前 `qb_credential_statuses` 不能视为已接上真实检测链路

### 风险备注页

显示：

- 风险备注
- 接口不稳定说明
- 使用建议

P0：

- 新增备注
- 标记解决

### 导入 / 导出页

显示：

- 导入模板
- 导出结构
- 脱敏预览
- 导入结果

P0：

- 脱敏导出
- 本地导入

P1：

- `ccusage JSON` 导入

## 7.1 截至 2026-07-03 的补缺清单

虽然 P0 主骨架已落地，但以下技术项仍需补齐：

1. `UsageLog` 录入 / 编辑 / 删除路由与表单
2. `today / week / month` 三段聚合口径
3. 配置状态页的只读检测刷新动作
4. 检测结果写回 `qb_credential_statuses`
5. “适合工具”字段在表单、展示和导入导出中的统一口径

## 8. Provider 策略

### P0 只做手动管理

- 火山方舟 Agent Plan / Coding Plan
- MiMo API / MiMo Token Plan
- SenseNova Token Plan Free
- OpenCode Zen Free
- OpenCode Go
- 阿里百炼 / Qwen
- Ollama Cloud Pro

### P1 优先适配

- DeepSeek
- Doubao / Ark
- LiteLLM
- OpenCode Zen

### P1 谨慎适配

- MiMo
- Alibaba Token Plan
- OpenCode Go
- Ollama Cloud

以上谨慎项默认关闭自动抓取，因为普遍依赖 Cookie 或网页结构。

## 9. 安全与隐私边界

必须满足：

1. 不保存真实 API Key
2. 不保存 Cookie
3. 不保存账号密码
4. 日志不输出敏感信息
5. 导出文件不包含真实密钥
6. 只允许保存 API Key 状态
7. Base URL 可以保存
8. 风险备注可以保存
9. 本地迁移必须可控

## 10. 导入导出要求

导出包必须包含：

- `schema_version`
- `exported_at`
- `platforms`
- `plans`
- `quota_buckets`
- `models`
- `usage_logs`
- `credential_statuses`
- `risk_notes`

导出包必须脱敏：

- 不得包含任何真实 key、token、cookie、password
- 只允许导出状态、路径、Base URL、模型、额度、备注

## 11. 最小可运行验证

### 阶段 1

- 新数据表迁移成功
- 原 onWatch 页面和原 Provider 数据不坏

### 阶段 2

- 新页面可打开
- 可录入样例平台 / 套餐 / 配额

### 阶段 3

- 可完成一次导出和导入闭环
- 首页推荐和风险提示可用

### 阶段 4

- 单个 Provider 适配失败不影响整体台账可用
