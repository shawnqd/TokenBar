# PROVIDER_SOURCE_STRATEGY

> 每个 Provider 声明自己支持的来源；来源之间正交；取不到就 `unknown`，不伪造。

## 1. 来源类型总览

| Source | 语义 | 存储 | opt-in | 失败兜底 |
| --- | --- | --- | --- | --- |
| `apiKey` | 用户填的 API Key | 本地配置，限制权限 | 否（默认开） | snapshot(stale) |
| `browserCookie` | 复用浏览器会话 Cookie | 会话内 / opt-in 缓存 | 是 | snapshot(stale) |
| `localFile` | 本地 CLI / app 配置文件 | 只读 | 否 | snapshot(stale) |
| `cliConfig` | 调用本地 CLI 取状态 | 只读 | 否 | snapshot(stale) |
| `snapshot` | 上次成功快照 | 本地 | 否 | `unknown` |

`snapshot` 永远是最后兜底，且必须标注 `stale`，绝不冒充 fresh。

## 2. Provider 来源矩阵（计划，Phase 1+ 落地）

> 「主」= Phase 1 优先实现；「次」= Phase 2 补齐。所有数字以平台真实返回为准，不外推。

| Provider | apiKey | browserCookie | localFile | cliConfig | snapshot | 主要窗口 |
| --- | --- | --- | --- | --- | --- | --- |
| Codex (OpenAI) | 次 (OAuth) | 次 | 主 (CLI config) | 主 (Codex CLI) | 兜底 | 5h / weekly / balance |
| OpenCode Go | — | 主 | 主 (SQLite) | — | 兜底 | 5h / 7d |
| MiniMax | 主 (token) | 次 | — | — | 兜底 | count / balance |
| Xiaomi MiMo | — | 主 | — | — | 兜底 | balance / token |
| DeepSeek | 主 | — | — | — | 兜底 | balance (paid/granted) |
| Doubao / Ark | 主 | — | — | — | 兜底 | requestLimit |
| Volc Engine Agent / Coding 次数型 | 主 | 次 | — | — | 兜底 | count / monthly |
| ccswitch 配置导入 | — | — | 主 (批量建实例) | — | — | 取决于被导入的 Provider |

## 3. 各 Provider 策略说明

### Codex (OpenAI)
- **来源优先级**：本地 Codex CLI config / CLI 调用 > OAuth API > 浏览器 Cookie（dashboard 增强）。
- **窗口**：5 小时滚动 + 周窗口 + 余额。
- **诚实边界**：CLI config 缺失时显示 `unknown`，不假设额度。
- **参考**：CodexBar 的 Codex RPC + CLI config 路径。

### OpenCode Go
- **来源优先级**：浏览器 Cookie（工作区订阅用量）> 本地 SQLite。
- **窗口**：5h / 7d。
- **诚实边界**：SQLite 路径未找到 → `unknown`。

### MiniMax
- **来源优先级**：API token > Cookie header > 浏览器 Cookie。
- **窗口**：次数型 + 余额。
- **诚实边界**：三种来源都不可用 → `unknown`，不混用。

### Xiaomi MiMo
- **来源优先级**：浏览器 Cookie。
- **窗口**：余额 + token 计划用量。
- **诚实边界**：未开 Cookie → `unknown`（不静默降级为 0）。

### DeepSeek
- **来源优先级**：API key。
- **窗口**：余额（区分 paid vs granted）。
- **诚实边界**：API 报错 → `unknown` + 错误状态，不缓存旧余额当真值。

### Doubao / Volcengine Ark
- **来源优先级**：API key。
- **窗口**：request-limit 探测。
- **诚实边界**：探测失败 → `unknown`。

### Volc Engine Agent / Coding 次数型套餐
- **来源优先级**：API key > Cookie。
- **窗口**：次数型 + 月窗口。
- **诚实边界**：次数 API 不公开时 → `unknown`。

### ccswitch 配置导入
- **语义**：ccswitch 是配置聚合器，不是额度来源。TokenBar 读其本地配置，**批量创建 Provider 实例**并填入对应来源，之后每个 Provider 走自己的来源策略。
- **诚实边界**：ccswitch 配置只提供「凭证 / base URL」，不提供「额度」；额度仍由各 Provider 来源拉取。

## 4. 来源状态机

每个 Provider 的每个来源有一个独立状态：

```
unknown ──(配置/开启)──► idle ──(刷新)──► fetching ──► ok
                          │                     │
                          └─────────────────────┴──► error
ok / error 在下一次刷新前持续；超过 stale 阈值后 ok 降级为 stale。
```

UI 上：`ok` = 绿、`stale` = 黄、`error` = 红、`unknown` = 灰。窗口值在任何非 ok 状态下都倾向显示 `unknown`（stale 可显式标注「旧值」）。

## 5. 隐私与安全约束

- API Key 本地存储，文件权限 `0600`，导出脱敏。
- Browser Cookie 默认 opt-out；开启后才读；不存主密码；解密走 Keychain（macOS）/ DPAPI（Windows，Phase 3）。
- 任何来源都不把凭证写进日志 / 崩溃报告。
- `snapshot` 不存凭证，只存额度数字 + 时间戳。

## 6. 新增 Provider 流程

1. 在 `ProviderID` 加枚举值。
2. 实现 `Provider` 协议（`supportedSources` + `fetch`）。
3. 在 `ProviderRegistry` 注册。
4. 在来源矩阵文档补一行。
5. 不改 UI —— UI 通过 `Provider` 协议泛化渲染。

## 7. 待核实假设（GPT 审核重点）

- 各平台真实 API / Cookie 字段是否与上表一致，需在 Phase 1 实现时逐个核实。
- ccswitch 配置文件实际路径与格式需在 Phase 2 落地前确认。
- 若某 Provider 实际无法获取任何窗口的真实值，则该窗口永久 `unknown`，不伪造。
