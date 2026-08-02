# TASK-018 阶段 1：现有数据/设置/格式化盘点与迁移表

任务 ID：`TASK-QUOTA-PRESENTATION-TASKBAR-018`
分支：`platform/windows`
本文件对应任务包 `docs/CLAUDE_TASK_PACKAGE_018.md` 的「执行顺序」第 1 条。
写作时 HEAD：`b8529fb5`（工作区有大量未提交改动，未提交任何内容）。

阶段划分（已由用户确认）：

- **阶段 1（本轮）**：本盘点 + 共享 `QuotaDisplayModel` 格式化层 + B/C/D 数据语义 + 三组件独立设置键与旧字段迁移。
- 阶段 2：H 设置页拆分、A 周额度与预测合并、E 设置窗口阴影复验。
- 阶段 3：F 任务栏 DirectWrite 可变字重 + 字体族备选、G 任务栏多服务商有序条目。

---

## 1. 现有数据模型（共享 Rust）

| 类型 | 文件 | 关键字段 | 备注 |
|---|---|---|---|
| `RateWindow` | `rust/src/core/rate_window.rs` | `used_percent: f64`（构造时 clamp 0–100）、`window_minutes: Option<u32>`、`resets_at: Option<DateTime<Utc>>`、`reset_description: Option<String>`、`is_informational: bool` | `remaining_percent()` = `100 - used`。`format_countdown()` 是 Rust 侧倒计时，前端另有一套 —— **两套倒计时并存，是本任务要收敛的重复点之一**。过期时 `format_countdown()` 返回 `"now"`（英文硬编码，未本地化）。 |
| `NamedRateWindow` | `rust/src/core/usage_snapshot.rs` | `id`、`title`、`window: RateWindow` | 服务商额外窗口。**阶段 3 的 G 项多条目选择要以 `id` 为持久化键**。 |
| `UsageSnapshot` | 同上 | `primary`、`secondary: Option`、`model_specific: Option`、`tertiary: Option`、`extra_rate_windows: Vec<NamedRateWindow>`、`updated_at` | 窗口是**固定槽位 + 一个数组**，没有统一的「窗口 id → 窗口」映射。G 项需要一层稳定的窗口标识，见第 5 节。 |
| `CostSnapshot` | 同上 | `used: f64`、`limit: Option<f64>`、`currency_code`、`period`、`resets_at: Option`、`updated_at` | 余额/费用语义。`used_percent()` 仅在有 `limit` 时返回 `Some`。**B 项的「已用/剩余」开关不得作用于此**。 |
| `ProviderFetchResult` | 同上 | `usage`、`cost: Option`、`wayfinder_usage: Option`、`source_label` | `source_label` 已存在，可满足任务包第 3 条「保留来源」的一半；**更新时间来自 `usage.updated_at`**。 |

`PaceSnapshot` **不在共享 Rust 的 core 里**，它在 Tauri 桥接层构造（`apps/desktop-tauri/src-tauri/src/commands/bridge.rs`、`provider_detail.rs`、`tray_bridge.rs`）。

## 2. 现有桥接层 DTO（TypeScript）

`apps/desktop-tauri/src/types/bridge.ts`

| 接口 | 关键字段 | 备注 |
|---|---|---|
| `RateWindowSnapshot` | `usedPercent`、`remainingPercent`、`windowMinutes`、`resetsAt`、`resetDescription`、`isExhausted`、`isInformational?`、`reservePercent`、`reserveDescription`、`reserveWillLastToReset?`、`reserveEtaSeconds?` | 已经同时携带 used 和 remaining，**前端不需要自己算 remaining**，但现有多处仍在 `100 - usedPercent`（见第 4 节）。 |
| `PaceSnapshot` | `stage`（7 档）、`deltaPercent`、`willLastToReset`、`etaSeconds`、`expectedUsedPercent`、`actualUsedPercent` | **A 项要用的全部字段已经在这里了**：实际% / 预计% / 差值 / 可支撑到重置 / ETA 秒。A 项是渲染合并问题，不是数据缺失问题。 |
| `CostSnapshotBridge` | `used`、`limit`、`remaining`、`currencyCode`、`period`、`resetsAt`、`formattedUsed`、`formattedLimit` | 余额类通道。 |
| `ProviderUsageSnapshot` | 上述窗口 + `cost`、`planName`、`sourceLabel`、`updatedAt`、`error`、`pace`、`trayStatusLabel`、`fetchDurationMs?` | 没有 `status` 枚举字段——`loading/ready/stale/error/unsupported` 目前靠调用点各自判断 `error != null` 之类，**这是任务包第 2 条要补的**。 |
| `ProviderDetail` | `session`/`weekly`/`modelSpecific`/`tertiary` + `pace` | 设置页用的 DTO，窗口字段名与 tray 卡片的 `primary`/`secondary` **不一致**，`providerBalance.ts` 已经为此写了两份平行解析函数。 |

## 3. 现有格式化函数（前端，分散状态）

| 位置 | 职责 | 与本任务的关系 |
|---|---|---|
| `src/hooks/useFormattedResetTime.ts` | 重置时间。`relative=true` 走「N 天 N 小时 / N 小时 N 分」，30 秒刷新一次；`relative=false` 走 `Intl.DateTimeFormat`（`month/day/hour/minute`）。`resetsAt` 缺失或不可解析时回落到 `resetDescription`。 | **C 项主战场。** 现状问题：过期时返回 `TrayResetsDueNow`（「即将重置」语义）而不是「已到期，等待刷新」；绝对模式恒定带月日，同日场景多余但不违规；`resetDescription` 回落没有「重置时间未知」的兜底。 |
| `src/lib/paceBudget.ts` | `getPaceBudget` / `getPaceEstimate` / `getPaceChartSnapshot`。全部从 `RateWindowSnapshot` 就地推算，**要求 `windowMinutes` 与 `resetsAt` 同时存在**，否则返回 `null`。 | A 项的「可支撑时长」来源。返回 `null` 即「数据不足」，任务包 A.3 要求把这个 `null` 显式呈现，不得渲染成 `0`。 |
| `src/lib/providerBalance.ts` | 解析各服务商塞进 `resetDescription` 的余额字符串（deepseek / mimo / mimoapi），产出 `BalanceView`，并给出要从额度行里排除的窗口。 | B 项「余额类不受百分比开关转换」的既有护栏，**保留不动**。它的 `excludeWindows` 现在还被 `quotaDisplay.primaryQuotaState` 复用，用来识别余额载体窗口 —— 这批窗口**没有** `isInformational` 标记，只判该标记会漏掉 DeepSeek/MiMo（审查 P1-2 的隐藏面）。⚠️ 该文件内有硬编码中文（`"余额"`、`"API 状态"`、`"暂不可用"`、`"含赠送 "`），违反任务包第 4 条本地化要求 —— 记为已知缺口，阶段 2 处理。 |
| `src/lib/relativeTime.ts` | `formatRelativeUpdated`：「N 分钟前更新」。 | 任务包第 3 条「用户能看出是实时还是缓存数据」要复用它。 |
| `src/surfaces/tray/paceCategory.ts` | `PaceSnapshot.stage` → 展示分类。 | A 项渲染合并时复用。 |
| `RateWindow::format_countdown()`（Rust） | 另一套倒计时，`"now"` 未本地化。 | 与前端重复。本轮不删（有 Rust 侧调用方），但**新代码一律走前端统一层**，并在 CHANGELOG 记录该重复。 |
| `ProviderQuotaBlock.tsx` 内的 `levelOf()` | 阈值分级 `normal/high/critical/exhausted`，**入参是 `remainingPercent`**（≤5 critical、≤25 high）。 | ⚠️ **B 项矛盾点的根源**：颜色恒按剩余量分级，而显示的数字按 `showAsUsed` 切换。任务包 B 明确禁止「显示剩余量但颜色仍按已用量报警」这类语义错配 —— 需要在统一层里把「显示语义」和「阈值语义」绑定。另外它忽略 `Settings.high_usage_threshold` / `critical_usage_threshold`（用户可配的 75/90 之类），硬编码 25/5。 |

## 4. `100 - usedPercent` 就地重算的位置（应改为消费统一层）

- `src/components/ProviderQuotaBlock.tsx:74`
- `src/components/MenuCard.tsx:714-715`、`:987`
- `src/components/ProviderGrid.tsx:63`
- `src/floatbar/FloatBar.tsx:183-184`

## 5. 设置字段现状

### 5.1 全局（会同时影响三个组件 —— 本任务要拆掉）

| 字段 | 文件 | 现状 |
|---|---|---|
| `show_as_used: bool`（默认 `true`） | `rust/src/settings.rs:89` | 一个开关同时被 FloatBar、TrayPanel、PopOutPanel、设置页服务商详情读取。 |
| `reset_time_relative: bool`（默认 `true`） | `rust/src/settings.rs:95` | 同上。 |

消费点（`showAsUsed` / `resetTimeRelative`）：
`floatbar/FloatBar.tsx:402,451,454`、`surfaces/TrayPanel.tsx:275-276,328`、
`surfaces/PopOutPanel.tsx:213,237-238`、`components/MenuCard.tsx`（多处透传）、
`components/ProviderQuotaBlock.tsx:46-47`、`components/ProviderGrid.tsx:11,23`、
`surfaces/settings/providers/ProviderDetailPane.tsx:51-52,74-75`。

### 5.2 任务栏（阶段 3 的 F/G 会改写）

`rust/src/settings.rs:217-247`：`taskbar_widget_enabled`、`taskbar_widget_position`（`notification`/`left`）、`taskbar_widget_font_weight: u16`（`normalize_taskbar_widget_font_weight` 只输出 300/400/700）、`taskbar_widget_content: String`（`usage`/`speed`/`usage_speed` —— **G 项要替换为有序数组**）、`taskbar_widget_font_size: u8`、`taskbar_widget_width: u16`、`taskbar_widget_text_align: String`。

`rust/src/settings/raw.rs` 已有 `RawTaskbarWidgetFontWeight` 兼容旧命名字重（`normal`/`medium`/`semibold`/`bold`），**这是本任务「旧字段兼容迁移」可以照抄的模式**。

## 6. 组件边界判定（需用户复核）

任务包 H 点名三个功能区：浮窗 / 仪表盘 / 任务栏。但代码里有四个展示面：

| 代码面 | 文件 | 本任务归属 |
|---|---|---|
| 浮窗 FloatBar | `src/floatbar/FloatBar.tsx` | **浮窗** |
| 托盘飞出面板 | `src/surfaces/TrayPanel.tsx` | **仪表盘** |
| 主弹出窗口 | `src/surfaces/PopOutPanel.tsx` | **仪表盘** |
| 原生任务栏状态条 | `src-tauri/src/taskbar_widget.rs` | **任务栏** |

**判定**：托盘飞出面板与主弹出窗口都渲染同一套 `MenuCard` / `ProviderQuotaBlock`，任务包只给了三个功能区，因此二者合并为「仪表盘」共用一组设置键。设置页的服务商详情（`ProviderDetailPane`）按任务包 B「只作为数据预览，跟随当前所在组件的显示上下文」，跟随仪表盘键。

若用户认为托盘飞出面板应当独立于主弹出窗口，本判定需要改为四组键 —— 已在交接文档标为待确认项。

## 7. 迁移表（阶段 1 实施内容）

新增字段全部为具体 `bool`（不是 `Option`）。`RawSettings` 侧用 `Option<bool>` 承载，从而区分「文件里没有这个键」与「显式写了 false」。旧文件加载时，缺失的组件键**一次性从旧全局字段播种**，之后彼此独立，不再有运行期继承 —— 这样才满足任务包 H「不能让一个旧的 Display 全局开关悄悄覆盖三个组件」。

| 新字段（`Settings`） | 类型 | 缺失时的迁移来源 | 消费方 |
|---|---|---|---|
| `float_bar_show_as_used` | `bool` | 旧 `show_as_used` | `FloatBar.tsx` |
| `float_bar_reset_time_relative` | `bool` | 旧 `reset_time_relative` | `FloatBar.tsx` |
| `dashboard_show_as_used` | `bool` | 旧 `show_as_used` | `TrayPanel.tsx`、`PopOutPanel.tsx`、`ProviderDetailPane.tsx` |
| `dashboard_reset_time_relative` | `bool` | 旧 `reset_time_relative` | 同上 |
| `taskbar_show_as_used` | `bool` | 旧 `show_as_used` | `tray_bridge::selected_tray_percents`（同时喂托盘图标与任务栏状态条）、`TaskbarTab` 预览 |

**任务栏没有 `taskbar_reset_time_relative`，这是刻意的。** 独立审查（`CODE_REVIEW.md` P1-1）指出该字段没有任何消费方；复核任务包 H 节后确认任务栏设置页定义的是「开关、字体、宽度、对齐、条目组合、服务商/额度窗口选择和排序」，**不含重置时间模式**，原生状态条也不渲染任何重置文本。因此该字段已删除，而不是为它补一个用户没要求的显示项。

防复发做了两层：类型上把 `QuotaPercentContext`（`showAsUsed` + 阈值）与 `QuotaDisplayContext`（额外带 `resetTimeRelative`）拆开，`ResetAwareComponent` 只含 `floatBar | dashboard`，所以向任务栏索取重置模式是编译错误；测试上 `settings::tests::taskbar_has_no_reset_time_mode_setting` 断言该键不会被持久化。若将来任务栏确实要显示重置时间，那是阶段三 G 项的改动，且必须有真实截图验收。

旧 `show_as_used` / `reset_time_relative` 保留在 `Settings` 与 `RawSettings` 中**仅供反序列化与迁移播种**，不再被任何展示面读取，也不再出现在设置 UI（UI 清理属阶段 2 的 H 项）。保留而不删除的理由：删除会让已存在的 `settings.json` 在 `RawSettings` 反序列化阶段丢失播种来源，且这两个字段目前是非 `#[serde(default)]` 的必填字段。

## 8. 阶段 1 明确不做

- 不合并周额度与预测的**渲染**（A，阶段 2）。
- 不拆分设置页 IA、不移除旧全局开关的 UI 控件（H，阶段 2）。
- 不动设置窗口阴影（E，阶段 2）。
- 不改任务栏字体渲染器、不改 `taskbar_widget_content` 为数组（F/G，阶段 3）。
- 不修 `providerBalance.ts` 的硬编码中文（阶段 2）。
- 不删 `RateWindow::format_countdown()`。
- 不提交、不推送、不合并。
