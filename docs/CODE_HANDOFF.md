# 给 Code / OpenCode 的执行任务书

## 1. 任务目标

基于 `onWatch` 改造成一个个人本地使用的双端通用「模型额度看板」。

重点不是重写底座，而是在不破坏 `onWatch` 原有能力的前提下，先交付 P0：

- 平台管理
- 套餐管理
- 额度桶管理
- 当前模型
- 到期 / 重置时间
- 今日 / 本周 / 本月消耗
- Base URL
- API Key 状态
- 适合工具
- 风险备注
- 首页推荐
- 导入 / 导出

## 2. 先读哪些文档

执行前必须先读：

1. `README.md`
2. `docs/PRODUCT_PLAN.md`
3. `docs/TECHNICAL_MANUAL.md`
4. 当前 fork 的 `onWatch` 仓库结构和以下文件：
   - `README.md`
   - `docs/API_INTEGRATIONS_SETUP.md`
   - `internal/store/store.go`
   - `internal/agent/*`
   - `internal/web/*`
   - `internal/config/*`

## 3. 范围边界

### 当前只做

1. P0
2. 本地个人工具
3. 手动管理为主
4. 只读配置检测
5. 脱敏导入导出

### 当前不做

1. P1 Provider 自动适配
2. SaaS
3. 多用户
4. 云同步
5. 保存真实密钥
6. Cookie 持久化
7. 网页余额抓取

## 4. 改造原则

1. 不从零开发
2. 不破坏 onWatch 原有 Provider 和 Dashboard 能力
3. 优先新增新表、新页面、新路由
4. 采集层和台账层分离
5. 自动适配失败不能拖垮手动台账

## 5. 推荐执行阶段

### 阶段 0：结构审查

目标：

- 先确认 `onWatch` 的可改造点
- 输出实际改造方案

交付物：

- 改造点清单
- 涉及文件清单
- 风险点清单

验收：

- 明确哪些地方新增，哪些地方不能动

### 阶段 1：P0 数据骨架

目标：

- 新增台账层数据结构
- 建立迁移方案

做什么：

- 新增 `Platform / Plan / QuotaBucket / Model / UsageLog / CredentialStatus / RiskNote`
- 设计脱敏导入导出 schema

不做什么：

- 不做 Provider 自动适配
- 不做趋势图

验收：

- 能完成数据库迁移
- 原 onWatch 旧表和旧页面不坏

### 阶段 2：P0 页面骨架

目标：

- 搭起新页面信息架构

做什么：

- 总览页
- 平台 / 套餐页
- Provider 状态页
- 使用记录页
- 配置状态页
- 风险备注页
- 导入 / 导出页

不做什么：

- 不做复杂图表
- 不做自动抓取

验收：

- 页面能打开
- 样例数据能显示

### 阶段 3：P0 核心闭环

目标：

- 形成可用的本地台账工具

做什么：

- 手动录入
- 只读配置检测
- 首页推荐
- 风险提示
- 导入 / 导出

验收：

- 能完成一次录入、导出、导入、查看推荐的完整闭环

### 阶段 4：P1 Provider 适配

建议顺序：

1. DeepSeek
2. Doubao / Ark
3. LiteLLM
4. OpenCode Zen
5. OpenCode Go
6. MiMo
7. Alibaba
8. Ollama

要求：

- 默认关闭高风险自动化
- Cookie / 网页方案必须显式开启
- 单个适配失败不影响主流程

## 6. Provider 策略要求

### P0 只做手动管理

- 火山方舟
- MiMo
- SenseNova
- OpenCode Zen
- OpenCode Go
- 阿里百炼 / Qwen
- Ollama Cloud Pro

### P1 可参考 CodexBar

- DeepSeek
- Doubao / Ark
- LiteLLM
- Alibaba
- Ollama
- MiMo

### P1 可参考 OpenCode Bar

- OpenCode auth.json 发现
- OpenCode Zen CLI 统计
- OpenCode Go workspace / auth 发现

## 7. 文件检查范围

开始编码前必须先检查：

- `internal/store/store.go`
- `internal/web/server.go`
- `internal/web/handlers.go`
- `internal/web/templates/*`
- `internal/agent/*`
- `internal/config/config.go`

如果需要加新模块，优先新建独立目录，不要把台账逻辑散落在 Provider 采集逻辑中。

## 8. 安全要求

硬性要求：

1. 不保存真实 API Key
2. 不保存 Cookie
3. 不保存账号密码
4. 不打印敏感日志
5. 导出文件不能包含真实密钥
6. 只能保存配置状态和脱敏说明

## 9. 遇到冲突时的处理规则

如果出现以下冲突，必须停下并先说明：

1. 现有 onWatch 结构无法无损新增
2. 必须改坏旧 Provider 才能完成 P0
3. 某项需求需要保存真实密钥
4. 某项需求只能依赖网页登录抓取

默认处理：

- 优先保住 P0 台账能力
- 把自动化降级为后续可选项

## 10. 最小验收标准

P0 完成后必须满足：

1. 原 onWatch 功能未被破坏
2. 新增平台 / 套餐 / 配额录入可用
3. 首页推荐可用
4. 快到期 / 低额度 / 风险提示可用
5. 导入 / 导出闭环可用
6. Mac / Windows 可迁移
7. 数据库、日志、导出文件不含真实密钥

## 10.1 截至 2026-07-03 的已知未完成项

以下事项未补齐前，不应宣称 P0 完整交付：

1. `UsageLog` 仍缺少录入 / 编辑 / 删除闭环
2. 使用记录页当前是最近 30 天汇总，不等于 `today / week / month` 三段口径完整落地
3. 配置状态页当前以展示为主，尚未接入真实只读检测刷新动作
4. `qb_credential_statuses` 当前主要由 seed / import 写入，不应误判为真实检测链路已完成
5. “适合工具”字段已存在于 schema，但尚未进入平台 / 模型录入表单和展示层

处理优先级：

- 先补齐以上 5 项
- 再进入 P1 Provider 适配

## 11. 当前推荐结论

继续使用 `onWatch`，但只把它当底层运行底座。

不要把第一阶段目标定义成“自动抓所有平台余额”。

第一阶段应该交付：

- 稳定的本地套餐台账
- 明确的额度桶管理
- 可靠的导入导出
- 可解释的首页推荐
