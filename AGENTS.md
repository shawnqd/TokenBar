# AGENTS

## 目标

TokenBar 是原生菜单栏额度管理工具：在菜单栏显示 AI 编程 / 模型平台的额度、次数、余额、重置窗口和可用状态。
第一阶段 Mac，后续 Windows。

## 工作方式

### 只做
- 原生桌面 App（Swift / SwiftUI，macOS 14+）
- 多来源：API Key / Browser Cookie / Local File / CLI config / Snapshot
- 多窗口：5h / 7d / 周 / 月 / 余额 / 次数 / token / request limit
- 菜单栏总览、Provider 详情卡片、手动刷新、设置页、数据来源状态
- 无法获取真实额度时显示 `unknown`，绝不伪造数据
- 分阶段实现，不删减长期 Provider / 功能范围

### 不做
- 不保存真实密码；Cookie / Token 默认 opt-in，复用本地已有会话
- 不做云同步、多用户、网页看板
- 不为 P0 偷偷破坏已落地模块

## 模块边界（不可越界）
- `TokenBarCore`：fetch + parse + provider adapter + source reader + storage。无 UI。
- `TokenBar`：state + UI（status item / popover / provider card / settings）。
- `TokenBarCLI`：bundled CLI。
- 采集层（Core）与展示层（App）严格分离，UI 不得直接做网络/文件采集。

## 先读顺序
1. `docs/TOKENBAR_PRODUCT_PLAN.md`
2. `docs/TOKENBAR_ARCHITECTURE.md`
3. `docs/PROVIDER_SOURCE_STRATEGY.md`
4. `docs/MAC_MVP_PLAN.md`
5. `docs/PROJECT_STATUS.md`

## 停手规则
出现以下情况必须停下并更新交接文件，不要擅自扩大范围：
1. 必须保存真实密码 / Cookie 永久持久化才能继续
2. 必须破坏 Core / App 模块边界才能落地
3. 某个 Provider 无法获取真实额度却被迫伪造数字
4. 真实平台行为与 `PROVIDER_SOURCE_STRATEGY.md` 假设差异过大

## 更新规则
每完成一个包，至少更新：
- `docs/PROJECT_STATUS.md`
- `TODO.md`
- `CHANGELOG.md`
- `AGENT_HANDOFF.md`

纯技术问题直接写进 `AGENT_HANDOFF.md`，不要把用户当传话中间层。
