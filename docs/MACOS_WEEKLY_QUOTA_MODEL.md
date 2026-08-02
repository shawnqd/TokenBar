# macOS CodexBar 周额度模型全解

来源：`steipete/CodexBar`（Peter Steinberger 的 macOS 原版），只读参考。
用途：Windows 线在做周额度/进度/预测相关 UI 前，先读这份，再读对应 Swift 原文，
**不要自己发明**。

对应源文件：

| 主题 | 文件 |
| --- | --- |
| 进度条绘制（含全部标记） | `Sources/CodexBar/UsageProgressBar.swift` |
| 文案 | `Sources/CodexBar/UsagePaceText.swift` |
| pace 数据模型 | `Sources/CodexBarCore/UsagePace.swift` |
| 阈值/工作日标记 | `Sources/CodexBar/MenuCardQuotaWarningMarkers.swift` |
| 5 小时等效预测 | `Sources/CodexBar/SessionEquivalentForecast.swift` |
| 预测式通知 | `Sources/CodexBar/PredictivePaceWarnings.swift` |
| 历史 pace | `Sources/CodexBar/HistoricalUsagePace.swift` |

---

## 1. pace 数据模型：7 个阶段，不是 3 个

`UsagePace` 字段：

```
deltaPercent            实际 − 预期（正=烧得快=超支，负=结余）
expectedUsedPercent     按时钟推进"此刻应该用掉多少"
etaSeconds              预计耗尽还有多久
willLastToReset         能否撑到重置
runOutProbability       耗尽概率（0–1，可空）
speedMultiplierToReset  还能提速多少倍仍撑到重置（可空）
```

`Stage` 有 **7 档**：`onTrack` / `slightlyAhead` / `ahead` / `farAhead` /
`slightlyBehind` / `behind` / `farBehind`。

**两个构造路径，语义不同：**

- `UsagePace.weekly(...)` —— 线性时钟推算，`runOutProbability` 恒为 `nil`。
- `UsagePace.historical(...)` —— 由真实历史用量推算，**带 `runOutProbability`**。

> Windows 现状：只有等价于 `.weekly` 的线性推算，没有历史路径，因此没有耗尽概率。

---

## 2. 进度条上有三类标记，不是一个

`UsageProgressBar` 在**同一根 6pt 条**上画三种东西：

### 2.1 pace tip（"该到的位置"）

- 宽 6pt（`stripeWidth 2 × 3`），居中于 pace 位置
- 用 `destinationOut` 挖穿 track+fill，**透明度 0.9**（保留 10%，不是挖成全透明）
- 缺口正中画 2pt 彩条：`paceOnTop` → 绿；deficit 且未高亮 → 红；高亮 → 白
- 竖条上下超出条体（`stripeTopY = -height*2`）后被裁切，读起来是"切穿"

### 2.2 quota warning 阈值标记（用户配置的多个节点）

`warningMarkerPercents(thresholds:showUsed:)`：

```swift
QuotaWarningThresholds.active(thresholds)
    .map { showUsed ? 100 - Double($0) : Double($0) }
    .filter { $0 > 0 && $0 < 100 }
```

- 用户可配多个阈值（如 50/75/90）
- **会按"显示已用/剩余"镜像**，和 pace 位置同样处理
- 绘制：punch 宽 5pt + 中间 1pt 中性色竖条（比 pace 细，颜色中性，不抢戏）

### 2.3 workday boundary（工作日分隔）

```swift
func workDayMarkerPercents(workDays: Int?, windowMinutes: Int?) -> [Double] {
    guard workDays != nil, windowMinutes == 10080 else { return [] }
    guard let wd = workDays, wd >= 2, wd <= 7 else { return [] }
    return (1..<wd).map { Double($0) * 100.0 / Double(wd) }
}
```

- **只对标准 7 天周（10080 分钟）生效**
- 若用户设定每周工作 5 天，则在 20%/40%/60%/80% 处画分隔
- 绘制：不挖穿，只在条体**下半部**画 1px 细刻度（`height * 0.5`），最弱的视觉层级
- 与 warning 标记去重（`abs(diff) < 0.001` 则丢弃 workday 的那个）

**三者视觉层级：pace（挖穿+彩色）> warning（挖穿+中性）> workday（半高细刻度）。**

---

## 3. 5 小时等效预测（用户记得的"几个 5 小时计算"）

`SessionEquivalentForecast.make(...)` 产出：

```
estimatedWindowsToExhaustWeekly  按当前烧速，周额度还够几个 5 小时窗口
windowsUntilReset                重置前还剩几个整 5 小时窗口
availableWindowsUntilReset       同上但按工作日折算（Double）
sampleCount                      样本数
```

核心算式：

```
estimatedWindows = (100 − weeklyUsedPercent) / burnEstimate.medianWeeklyPercentPerWindow
availableWindows = effectiveRemainingSeconds(考虑 workDays) / 5h
```

**大量前置守卫，任何一条不满足就返回 `nil`（不显示，而不是编造）：**

- session 窗口必须是真实 300 分钟窗口，且非 `isSyntheticPlaceholder`
- weekly 窗口必须是 10080 分钟且有 `resetsAt`
- `weeklyUsedPercent` 有限且在 0–100
- `medianWeeklyPercentPerWindow` 有限且 > 0
- `sampleCount >= SessionEquivalentBurnEstimator.minimumSampleCount`
- session/weekly 剩余时间为正且不超过窗口长度（+2 分钟容差）
- 剩余周额度必须 > 0

文案（`UsagePaceText.sessionEquivalentDetail`）：
左 `Estimated: N session quotas left`，右 `N windows until reset`。

> 这是"按中位烧速，你还能开几次 5 小时会话"，**依赖历史样本**，
> Windows 目前完全没有这套（没有 burn estimator、没有样本存储）。

---

## 4. 右侧文案的完整规则（`detailRightLabel`）

```
若 willLastToReset:
    "Lasts until reset"
    （codex 专属：deltaPercent < −15 且 speedMultiplierToReset >= 1.5
      时追加 " · 1.5× headroom"）
否则若有 etaSeconds:
    weekly 上下文 → "Runs out in <时长>"（时长为 "now" 时 → "Runs out now"）
    session 上下文 → "Projected empty in <时长>"
否则: nil

再叠加 runOutProbability:
    roundedRisk = round(prob*100 / 5) * 5        ← 按 5% 取整
    若 willLastToReset 且 risk > 0 → 只显示 "≈ N% run-out risk"
    否则若有 etaLabel → "<etaLabel> · ≈ N% run-out risk"
```

左侧（`detailLeftLabel`）：`On pace` / `N% in deficit` / `N% in reserve`，
`deltaValue == 0` 时一律 `On pace`。

汇总串（`weeklySummary`）：`Pace: <左> · <右>`。

**关键点：够用到重置时不显示任何时长**——时长只在会耗尽时出现。

---

## 5. 预测式通知（用户记得的"多个节点通知"）

`PredictivePaceWarningNotificationLogic`：

```swift
static func shouldNotify(pace: UsagePace) -> Bool {
    guard !pace.willLastToReset else { return false }
    guard let eta = pace.etaSeconds, eta > 0 else { return false }
    guard (pace.runOutProbability ?? 1) >= 0.5 else { return false }
    return true
}
```

去重键 `PredictivePaceWarningStateKey`：`(provider, accountDiscriminator, window, resetWindow)`。

`PredictivePaceWarningResetWindow.belongsToSameCycle`：同一 `windowMinutes`，
且 `resetsAt` 差值 < `max(windowMinutes*60/2, 300)` 秒即视为同一周期
——**同一个重置周期内只通知一次**。

`recordObservation` 在 `willLastToReset` 恢复时**移除**该键，即重新武装，
下个周期或再次恶化时可再通知。

> 注意这套依赖 `runOutProbability >= 0.5`，而该字段只有 `.historical` 路径才有。
> 换言之：**没有历史 pace，就没有预测式通知**。

---

## 6. Windows 线与之的差距（截至 2026-07-31）

| macOS 能力 | Windows 现状 |
| --- | --- |
| pace tip 挖穿 + 彩条 | ✅ 已移植（本轮） |
| 7 档 stage | ❌ 只有 reserve/deficit/on-pace 三态 |
| 历史 pace + 耗尽概率 | ❌ 无 |
| quota warning 多阈值标记 | ❌ 无 |
| workday 分隔标记 | ❌ 无（也无"每周工作几天"设置） |
| 5 小时等效预测 | ❌ 无（无 burn estimator / 样本库） |
| 预测式通知 + 周期去重 | ❌ 无 |
| "1.5× headroom" 提速提示 | ❌ 无 |
| 够用时不显示时长 | ✅ 已移植（本轮） |
| 按 5% 取整的 run-out risk | ❌ 无（依赖概率） |

**依赖链**：耗尽概率 ← 历史 pace ← 用量样本存储。
5 小时等效预测 ← burn estimator ← 同一套样本存储。
预测式通知 ← 耗尽概率。

所以若要补齐，**先做"用量历史样本存储 + 中位烧速估计"这一层**，
其余三项（概率、5 小时等效、预测通知）才有数据来源；
而多阈值标记和工作日标记不依赖历史，可以独立先做。
