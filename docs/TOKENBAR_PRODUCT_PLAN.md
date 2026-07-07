# TOKENBAR_PRODUCT_PLAN

## 1. 定位

TokenBar 是一个**原生菜单栏额度管理工具**，在菜单栏显示 AI 编程 / 模型平台的额度、次数、余额、重置窗口和可用状态。

不是网页看板，不是纯手动台账，不是云同步多用户系统。

第一平台 macOS，第二平台 Windows。

## 2. 要解决的核心问题

每个 AI 编程平台暴露额度的方式都不同：

- 有的用 API Key 查余额
- 有的只能读浏览器 Cookie
- 有的只写在本地 CLI 配置文件里
- 有的按「次数」计费（request limit）
- 有的按 token 计费
- 有的按 5 小时滚动窗口
- 有的按周 / 月周期重置
- 有的同时有多个窗口叠加

人脑记不住，散落在各处查不清。TokenBar 把它们收拢到一个菜单栏图标，每个 Provider 一张卡片，每个窗口一个倒计时，让你能在开始长任务前判断「现在跑会不会撞重置」。

## 3. 长期功能范围（不删减，分阶段实现）

### 3.1 Provider 清单

| Provider | 计划支持的来源 |
| --- | --- |
| Codex (OpenAI) | OAuth API、本地 Codex CLI config |
| OpenCode Go | Browser Cookie / 本地 SQLite |
| MiniMax | API token / Cookie header / Browser cookie |
| Xiaomi MiMo | Browser cookie |
| DeepSeek | API key（余额，paid vs granted） |
| Doubao / Volcengine Ark | API key（request-limit probe） |
| Volc Engine Agent / Coding 次数型套餐 | API key / Cookie |
| ccswitch 配置 | 本地 CLI config 导入 |

新增 Provider 走 `Provider` adapter 契约，不硬编码进 UI。

### 3.2 数据来源类型

| Source | 说明 | 是否 opt-in |
| --- | --- | --- |
| `apiKey` | 用户填的 API Key，本地存储，限制文件权限 | 默认开 |
| `browserCookie` | 复用已有浏览器会话 Cookie，不存密码 | opt-in |
| `localFile` | 读本地 CLI / app 配置文件 | 默认开 |
| `cliConfig` | 调用本地 CLI 拿配置 / 状态 | 默认开 |
| `snapshot` | 旧快照兜底（手动导入或上次成功结果） | 默认开 |

### 3.3 额度窗口类型

- `fiveHour` — 5 小时滚动窗口
- `sevenDay` — 7 天滚动窗口
- `weekly` — 自然周
- `monthly` — 自然月
- `balance` — 余额（USD / credit / token）
- `count` — 次数型（已用 / 总量）
- `token` — token 消耗型
- `requestLimit` — 请求速率上限

一个 Provider 可同时有多个窗口；UI 按窗口分别渲染进度条与重置倒计时。

### 3.4 功能点

- 菜单栏总览：一个 status item，图标随最高用量窗口变化
- Provider 详情卡片：每个 Provider 一张卡，列出所有窗口、来源、上次刷新、状态
- 手动刷新：全局 + 单 Provider
- 设置页：Provider 开关、来源配置、刷新节奏、显示选项
- 数据来源状态：每个 Provider 的每个来源是 ok / stale / error / unknown
- `unknown` 语义：取不到真实额度时明确显示 `unknown`，不伪造、不外推、不缓存过期数字当真值
- ccswitch 配置导入：读本地 ccswitch 配置，批量建 Provider 实例
- 脱敏导入 / 导出：导出配置时不带真实 Key / Cookie

## 4. 阶段划分

| 阶段 | 交付 | 验收 |
| --- | --- | --- |
| Phase 0 | Scaffold + 文档（本分支） | 模块边界清晰，GPT 审核通过 |
| Phase 1 | Mac MVP：App 外壳 + 1 个 Provider 端到端 + 菜单栏 + popover + 手动刷新 + 设置 + 来源状态 | 见 `MAC_MVP_PLAN.md` |
| Phase 2 | 全 Provider 覆盖 + 多窗口渲染 + ccswitch 导入 + 脱敏导入导出 | 全清单 Provider 至少 1 来源可用 |
| Phase 3 | Windows 原生端口 | Windows 上等价 MVP |

## 5. 不做的事（长期边界）

- 不保存真实密码
- 不做 Cookie 永久持久化（仅会话内复用 / opt-in 缓存）
- 不做云同步、多用户
- 不做网页看板（旧 Web 项目已封存在 `legacy/web-p0-mvp`）
- 不伪造任何额度数字

## 6. 与旧 onWatch / Web P0 的关系

- 旧 Go Web P0 MVP 封存在 `legacy/web-p0-mvp` 分支，保留不删。
- 新 `main` 用 orphan 重建，不携带旧 Web 代码。
- 旧 onWatch 的 Provider 采集经验作为参考，但代码不迁移；TokenBar 用 Swift 重写。

## 7. 参考

- 架构：`docs/TOKENBAR_ARCHITECTURE.md`
- Provider 来源策略：`docs/PROVIDER_SOURCE_STRATEGY.md`
- Mac MVP 计划：`docs/MAC_MVP_PLAN.md`
- 上游参考：[steipete/CodexBar](https://github.com/steipete/CodexBar)（MIT）
