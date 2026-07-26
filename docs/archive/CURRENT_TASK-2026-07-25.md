# CURRENT_TASK

## 状态：实现完成，待用户视觉确认（2026-07-25）

两个任务包均已执行完成，主会话补齐遗留项并做真机截图验证。详见 `AGENT_HANDOFF.md`
2026-07-25 checkpoint (2)。紧凑/极简两档密度未做真机截图（仅代码+单测覆盖），其余均已
真机确认。以下原始任务包内容保留作为实现依据记录。

## 目标

浮窗（tray flyout）全量重新开发。用户判定上一轮开发出现重大失误（验证死循环），要求完全
重新开发、不在旧代码基础上改。背景、审计发现、边界决定见 `DECISIONS.md` 2026-07-25 条目、
`AGENT_HANDOFF.md` 同日 checkpoint、`PROJECT_STATUS.md`"已知问题"。

用户已确认三件事，本轮按此执行，不再返工讨论：
1. 浮窗整体外壳样式选 **B 方案「统一分区」**——参考文件
   `C:\Users\13701\AppData\Local\Temp\claude\C--Users-13701-Documents-Man-Worker-TokenBar\5f964b95-3687-4ef0-ad5e-6ef8aae618e2\scratchpad\flyout_shell_options.html`
   的 `.b-shell` 那一列（class 前缀 `b-`），是用真实数据和真实 CSS token 做的高保真 HTML，
   直接读取该文件即可拿到精确结构和样式值，不要凭空猜。核心特征：切换栏贴合面板顶部（用
   `border-bottom` 分隔，不是浮动药丸）、整个浮窗是一块连续白色面板（不是"卡中卡"）、
   provider 之间用细分割线分隔（不是每个套一个白底投影卡片）、底部操作条贴合面板底部
   （`border-top`，不是浮动药丸）。
2. 卡片内部密度分层（详细/紧凑/极简）：见下方「密度分层规格」，逐字段照做。
3. **作用域**（关键，纠正上一轮 stash 里的错误实现）：详细/紧凑/极简三态切换**只影响浮窗
   "全部服务商"概览列表**；点开单个服务商的详情视图（`selectedProviderId !== null`）**永远**
   渲染"详细"两分区布局，不随设置切换；设置页预览卡、PopOut 仪表盘**完全不动**，不传
   `densityMode`，维持现状。这是原有 `DECISIONS.md` 2026-07-23 条目就定下的规则，上一轮
   （已 stash）误把详情视图也做成随设置切换，这次要修正。

## 执行方与角色

- 总控：Claude Code 主会话（Sonnet 5）
- 执行：两个 complex-executor subagent 并行（文件不重叠，见下方任务包 1/2）
- 审查：reviewer subagent（两包都完成后）
- 最终真机视觉验证：总控本人（Win32 API 截图），不交给 subagent——上一轮的验证死循环
  就发生在这一步，总控已摸清 Vite dev server 长期运行会导致内容与磁盘不一致的坑
  （见 `AGENT_HANDOFF.md`），会先确认 Vite 进程是新鲜的再截图
- commit / push / merge：仅用户明确授权后

---

## 任务包 1：Rust 窗口路由清理

### 允许修改

- `apps/desktop-tauri/src-tauri/src/surface.rs`
- `apps/desktop-tauri/src-tauri/src/shell/transition.rs`
- `apps/desktop-tauri/src-tauri/src/state.rs`
- `apps/desktop-tauri/src-tauri/src/shell/window.rs`
- `apps/desktop-tauri/src-tauri/src/shell/dwm.rs`
- `apps/desktop-tauri/src-tauri/src/shell/flyout_window.rs`
- `apps/desktop-tauri/src-tauri/src/proof_harness.rs`
- `apps/desktop-tauri/src-tauri/src/commands/surface.rs`
- `apps/desktop-tauri/src/App.tsx`（仅移除下述死分支）

### 禁止修改

- 任务包 2 的文件；`rust/**`；locale/ftl；`platform/macos`；不新建分支；不 commit/push/merge

### 任务

1. **彻底删除旧共享窗口浮窗路径**：`main` 窗口目前仍能渲染 `SurfaceMode::TrayPanel`
   （`surface.rs`）。改到"没有任何路径能让 `main` 渲染 tray-panel 内容"——无论是真实用户
   操作、`commands::set_surface_mode` 这个 IPC 命令、还是 proof-mode，唯一能显示 tray 面板
   内容的只能是 `shell::flyout_window` 这个独立窗口。`commands::set_surface_mode` 收到
   `"trayPanel"` 时应该拒绝/忽略，不能落地成 `main` 的一个可渲染状态。
2. **删除确认的死代码**：`shell/transition.rs::schedule_startup_tray_panel_reveal_fallback` +
   `should_force_tray_panel_reveal`、`state.rs::arm_startup_tray_reveal`、
   `shell/window.rs::hide_to_tray_state`（均带 `#[allow(dead_code)]`，生产无调用方，已确认
   是拆分出独立 flyout 窗口之前的旧实现残留）。
3. **修复 proof_harness 路由**：`CODEXBAR_PROOF_MODE=trayPanel` 必须打开真实的
   `shell::flyout_window::open_or_focus`，不能落到 `main`。可以用
   `git stash show -p stash@{0} -- apps/desktop-tauri/src-tauri/src/proof_harness.rs` 看上一轮
   （已验证工作正常）的修法作参考，但要重新审查、不要盲目照抄——那一轮改动是在旧共享路径
   还存在的前提下打的补丁，这一轮共享路径整个被删掉了，逻辑可能可以更直接。
4. **复用已验证的浮窗缩放/最小最大宽高修复**：Part A（缩放偶发关闭 + 无最小/最大宽高约束
   两个 bug）已经在 `git stash@{0}` 里修好并用真实 Win32 拖拽测试验证过。用
   `git stash show -p stash@{0} -- <文件>` 分别看 `surface.rs`（`WindowProperties` 加
   `max_width`/`max_height`）、`shell/window.rs`（`set_max_size` + clamp）、`shell/dwm.rs`
   （`WM_GETMINMAXINFO` 委托给 `DefSubclassProc`）、`shell/flyout_window.rs`
   （`.max_inner_size()`）这几处的具体修法，重新应用等价逻辑。
5. **清理 `App.tsx` 死分支**：完成第 1 步后，`SurfaceRouter` 里 `case "trayPanel"`
   （渲染 `main` 窗口里的 TrayPanel 那条分支）永远不会再被触发，删除它。

### 验收标准

- `cargo build`（或至少 `cargo check`）在 `apps/desktop-tauri/src-tauri` 下干净
- `cargo test` 全绿（含 Part A 加的 3 个 `prearm_gesture_blur_guard` 测试，如果沿用相同测试名）
- `npx tsc --noEmit`（`apps/desktop-tauri`）干净
- 能用 grep 证明：仓库里已经没有 `SurfaceMode::TrayPanel` 被渲染到 `main` 的路径；上述 4 个
  死函数已不存在
- 完成后写清楚：改了哪些文件、`SurfaceMode::TrayPanel` 具体是怎么处理的（保留枚举值但不可达，
  还是整个删掉）、验证命令的真实输出

---

## 任务包 2：前端浮窗内容重写

### 允许修改

- `apps/desktop-tauri/src/surfaces/TrayPanel.tsx`
- `apps/desktop-tauri/src/components/MenuCard.tsx`
- `apps/desktop-tauri/src/components/MenuSurface.tsx`（仅 tray 变体相关，`--popout` 相关规则/
  分支不动）
- `apps/desktop-tauri/src/styles.css`
- `apps/desktop-tauri/src/surfaces/TrayPanel.test.tsx`（如需要同步更新测试）

### 禁止修改

- `PopOutPanel.tsx`、任何 Settings 相关组件、`ProviderQuotaBlock.tsx`（除非确认必须）、
  任务包 1 的 Rust 文件、locale/ftl、`platform/macos`

### A. 外壳样式（Option B）

参考文件（**直接 Read，不要凭空猜结构**）：
`C:\Users\13701\AppData\Local\Temp\claude\C--Users-13701-Documents-Man-Worker-TokenBar\5f964b95-3687-4ef0-ad5e-6ef8aae618e2\scratchpad\flyout_shell_options.html`
里 class 前缀 `b-` 的那一列（`.b-shell`/`.b-body`/`.b-sep`）。

- 切换栏（`ProviderGrid`）贴合面板顶部，用 `border-bottom` 与下方分隔，不是浮动药丸。
- 整个浮窗是一块连续面板背景（`var(--app-bg)` 或等价），provider 之间用一条细分割线分隔
  （替换现在的 `.menu-stack__sep`），**不是**每个 provider 套一层独立白底+投影的卡片。
- `MenuCard` 在 tray 概览列表场景下渲染时，不带自己的外层白底/投影/圆角框——两个分区
  （额度 zone / 洞察 zone）的浅色 tint 背景直接贴在面板背景上，作为唯一的视觉分组手段。
- 底部操作条（显示窗口/刷新/设置/退出）贴合面板底部，用 `border-top` 分隔，不是浮动药丸。
- **必须只影响 tray 变体**：`.menu-surface--popout` 相关规则一律不动，PopOut 仪表盘的视觉
  不能有任何变化。用 CSS 选择器加 `.menu-surface--tray` 前缀限定作用域。

### B. 密度分层规格（逐字段照做，来自用户确认的最终参考截图）

三档共享的卡头（不受密度影响）：provider 图标 + 名称（左），更新时间"X 分钟前更新"
（详细档）或"X分钟前"（紧凑/极简档，省略"更新"二字、字号略小）（右）。

**详细档（`densityMode="detailed"`）**——两个分区，分区背景 `var(--surface-elevated)`，
圆角 10px，内边距 12px，分区间 gap 8-10px：

- 分区一（额度 zone）：
  - 主额度行：`{窗口标签}`（如"5 小时额度"，左） … `{重置倒计时}`（如"3小时57分后重置"，右，
    次要色）；大号粗体百分比（如"22%"）；蓝色进度条
  - 分区内细分割线
  - 次额度行（如有）：同样式，标签+倒计时 / 百分比+"已使用"标签 / 进度条
- 分区二（洞察 zone），字段间用分区内细分割线分隔：
  - 速度行：⚡ 图标（新建 `BoltIcon`，`path d="M13 3L4 14h6l-1 7 9-11h-6l1-7z"`，复用
    `paceIconProps` 13x13/stroke 风格）+ "最近输出速度"（左） … 数值加粗（如"64.9 t/s"，右）
  - 用量行：📊 图标（新建 `ChartBarIcon`，`path d="M4 20V10M10 20V4M16 20v-7M20 20H4"`）+
    "近7天使用"（左） … token 数加粗 + "≈X万/亿" 约数（右）；下方两行小字：
    "等额 API 价值 ≈ $X · ¥Y"、"热门模型：{model}"
  - 进度行：图标（复用现有 `TrendIcon`）+ "本周实际用量"（左） … 一行文字：
    `{实际}% · 预期 {预期}%（{超前/落后} {差值}%）`（右，"预期…"及括号部分用
    `var(--pace-steady-fg)` 绿色，差值为负时用 `var(--pace-racing-fg)` 橙色）；下方进度条
    （蓝色 fill + 竖线 marker 标预期位置，复用现有 `.menu-card__pace-track` 结构）；下方
    运行时间行：图标（复用现有 `CheckIcon`/`WarnIcon`，取决于 `willLastToReset`）+
    "足以支撑到重置" 或对应的耗尽提示文案（左，绿/橙） … "≈X小时"（右，加粗同色）

**紧凑档（`densityMode="compact"`）**：
- 单个 zone：主额度行（标签+倒计时 / 百分比 / 蓝色进度条）；分区内细分割线；次额度行
  （标签 / 百分比右对齐，**不带**进度条，纯文字行）
- 下方一排 3 个等宽 chip（`background:var(--surface-elevated)`，圆角 8px，居中）：
  速度 chip（⚡ + 数值加粗 + 下方小字"t/s"）、用量 chip（数值加粗（约数如"92.2万"）+ 下方
  小字"近7天"）、进度 delta chip（"+X.X%" 加粗，正值绿色/负值橙色 + 下方小字"略超前"/
  "落后"等状态词）

**极简档（`densityMode="minimal"`）**：
- 主额度行（标签 / 百分比 / 蓝色进度条），`suppressForecast` 效果，不显示次额度
- 下方一行摘要：`{次要标签} {次要百分比}%`（左） … ⚡ `{速度}t/s`（中） … `{delta}%`
  （右，同色规则），字号 caption 级别，次要色

密度作用域（关键，见上方"作用域"总述）：
- `TrayPanel.tsx` 概览列表（`selectedProviderId === null`）：
  `densityMode={settings.menuBarDisplayMode}`
- `TrayPanel.tsx` 单服务商详情（`selectedProviderId !== null`）：`densityMode="detailed"`
  **硬编码**，不读 `settings.menuBarDisplayMode`
- 其它 `MenuCard` 调用方（设置页预览卡、PopOut）：不传 `densityMode`（维持现状 `undefined`，
  完全不受这次改动影响）

### 验证纪律（必须遵守，上一轮就是栽在这里）

1. `npx tsc --noEmit`、`npx vitest run`（`apps/desktop-tauri`）必须干净通过。
2. **不要**用真机截图去反复验证是否生效——那是总控本人收尾时做的事。你只需要保证代码逻辑
   和上面的规格一致、类型检查和单测通过，写清楚改了什么、为什么这样映射到 CSS/JSX 结构。
3. 如果需要临时本地验证渲染是否报错，先确认 `Get-NetTCPConnection -LocalPort 1420 -State
   Listen` 对应进程是不是你自己刚启动的新进程；长期存活的旧 Vite 进程会返回和磁盘不一致的
   陈旧内容（已有真实事故记录，见 `AGENT_HANDOFF.md` 2026-07-25），不要基于旧进程的显示结果
   下任何结论。

---

## 停止条件

- 任务包 1：如果发现 `SurfaceMode::TrayPanel` 枚举值被其它未列出的代码路径依赖，无法干净
  删除，停止并报告依赖点，不要为了"删干净"而破坏其它功能
- 任务包 2：如果参考文件（B 方案 HTML）读不到，或密度分层规格与 `MenuCard.tsx` 现有字段
  结构对不上（例如某个字段现在根本不存在），停止并报告缺口，不要自行发挥
- 两个任务包都不涉及 commit/push/merge，不新建分支
