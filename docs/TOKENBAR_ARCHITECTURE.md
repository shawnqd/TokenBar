# TOKENBAR_ARCHITECTURE

## 1. 设计原则

1. **采集与展示分离**：`TokenBarCore` 只做 fetch + parse + storage，`TokenBar` 只做 state + UI。UI 不直接发网络 / 读文件。
2. **Provider adapter 契约**：每个 Provider 实现同一协议，UI 不认具体平台。
3. **来源正交**：API Key / Cookie / LocalFile / CLIConfig / Snapshot 是正交来源，Provider 声明自己支持哪几种。
4. **窗口是一等公民**：5h / 7d / 周 / 月 / 余额 / 次数 / token / request-limit 统一为 `UsageWindow`，UI 泛化渲染。
5. **unknown 是合法状态**：取不到就标 `unknown`，不伪造、不外推、不把过期快照当真值。
6. **Swift 6 并发**：Sendable 状态，显式 MainActor 跳转，后台 refresh 不阻塞 UI。
7. **不破坏即稳**：单个 Provider / 来源失败不得影响其他 Provider 或拖死菜单栏。

## 2. 模块划分

```
Sources/
├── TokenBarCore/          # 采集 + 解析 + 存储（无 UI）
│   ├── Provider/          # Provider 协议、注册表、ProviderID
│   ├── Source/            # 来源类型枚举与 reader 契约
│   ├── Window/            # UsageWindow 模型
│   └── Storage/           # ConfigStore / SnapshotStore
├── TokenBar/              # App：state + UI
│   ├── App/               # TokenBarApp / AppDelegate
│   ├── State/             # UsageStore / SettingsStore / SourceStatusStore
│   └── UI/                # StatusItemController / ProviderCardView / SettingsView
└── TokenBarCLI/           # bundled CLI（scripts / CI）
```

模块边界是硬约束：`TokenBar` 依赖 `TokenBarCore`，`TokenBarCore` 不反向依赖 `TokenBar`。

### 2.1 TokenBarCore

| 子模块 | 职责 | Phase 0 状态 |
| --- | --- | --- |
| `Provider/Provider.swift` | `Provider` 协议、`ProviderRegistry` | 协议已定义，无实现 |
| `Provider/ProviderID.swift` | `ProviderID` 枚举（全清单） | 枚举已定义 |
| `Source/UsageSource.swift` | `UsageSource` 枚举 + `SourceReader` 协议 | 已定义，无实现 |
| `Window/UsageWindow.swift` | `UsageWindow` / `WindowKind` / `Snapshot` | 已定义，无实现 |
| `Storage/ConfigStore.swift` | 配置读写（脱敏） | 占位 |
| `Storage/SnapshotStore.swift` | 上次成功快照（兜底，标记 stale） | 占位 |

### 2.2 TokenBar (App)

| 子模块 | 职责 | Phase 0 状态 |
| --- | --- | --- |
| `App/TokenBarApp.swift` | SwiftUI 入口、Settings scene | 占位 |
| `App/AppDelegate.swift` | 接线 status controller、菜单栏 | 占位 |
| `State/UsageStore.swift` | 持有所有 Provider 快照、刷新调度 | 占位 |
| `State/SettingsStore.swift` | Provider 开关 / 来源配置 / 刷新节奏 | 占位 |
| `State/SourceStatusStore.swift` | 每来源 ok/stale/error/unknown | 占位 |
| `UI/StatusItemController.swift` | 菜单栏 status item + 图标 | 占位 |
| `UI/ProviderCardView.swift` | Provider 详情卡（窗口进度 + 倒计时） | 占位 |
| `UI/SettingsView.swift` | 设置页 | 占位 |

### 2.3 TokenBarCLI

bundled CLI，Phase 1 起逐步加 `tokenbar status` / `tokenbar refresh` / `tokenbar config` 子命令。Phase 0 仅入口占位。

## 3. 数据流

```
┌─────────────────────────────────────────────────────────────┐
│  定时器 / 手动刷新                                            │
│         │                                                    │
│         ▼                                                    │
│  UsageStore.refresh(providerIDs)                             │
│         │                                                    │
│         ▼                                                    │
│  ProviderRegistry[id].fetch(sources)   ──► SourceReader      │
│         │                              (apiKey/cookie/file/  │
│         │                               cli/snapshot)        │
│         ▼                                                    │
│  parse → [UsageWindow]  +  SourceStatus                      │
│         │                                                    │
│         ▼                                                    │
│  SnapshotStore.persist(快照, 标记 fresh)                      │
│         │                                                    │
│         ▼                                                    │
│  UsageStore @MainActor 发布                                  │
│         │                                                    │
│         ▼                                                    │
│  StatusItemController (图标) / ProviderCardView (卡片)        │
└─────────────────────────────────────────────────────────────┘
```

- 失败：`SourceStatus = .error`，窗口值 `nil` → UI 显示 `unknown`，并用 `SnapshotStore` 里上次成功快照（明确标 `stale`）兜底，**不**把 stale 当 fresh。
- 无来源配置：`SourceStatus = .unknown`，UI 显示 `unknown`，不发请求。

## 4. 关键类型契约（Phase 0 已定义骨架）

```swift
// Provider 契约
public protocol Provider: Sendable {
    var id: ProviderID { get }
    var displayName: String { get }
    var supportedSources: Set<UsageSource> { get }
    func fetch(using readers: [UsageSource: any SourceReader]) async -> ProviderSnapshot
}

// 来源
public enum UsageSource: String, Sendable {
    case apiKey, browserCookie, localFile, cliConfig, snapshot
}

// 窗口
public enum WindowKind: String, Sendable {
    case fiveHour, sevenDay, weekly, monthly, balance, count, token, requestLimit
}
public struct UsageWindow: Sendable {
    public let kind: WindowKind
    public let used: Double?      // nil = unknown
    public let total: Double?     // nil = unknown
    public let resetsAt: Date?    // nil = 无固定重置 / unknown
}
```

> 完整定义见 `Sources/TokenBarCore/`。Phase 0 只定义类型，不实现 `fetch` / `read`。

## 5. 并发模型

- Swift 6 strict concurrency。
- `Provider.fetch` 与 `SourceReader.read` 是 `async`，跑在后台 task。
- `UsageStore` 是 `@MainActor`，发布给 UI。
- Provider 之间并行刷新，单 Provider 失败不阻塞其他。
- 刷新节奏：manual / 1m / 2m / 5m / 15m（设置可调）。

## 6. 配置与隐私

- 配置文件：`~/.config/tokenbar/config.json`（默认），限制文件权限。
- API Key 本地存储，不联网上传，导出时脱敏。
- Browser Cookie 默认 opt-out；用户开启才读；不存密码。
- 详见 `PROVIDER_SOURCE_STRATEGY.md`。

## 7. 上游参考

- 模块分离（core fetch+parse vs app state+UI vs CLI）参考 [CodexBar](https://github.com/steipete/CodexBar)（MIT）。
- 本地 cost-usage 扫描概念参考 [ccusage](https://github.com/ryoppippi/ccusage)（MIT）。
- TokenBar 是独立实现，不直接 vend 上游源码；如引入则保留文件头 MIT notice。
