# DECISIONS

## 2026-07-26 — Unified workflow document contract

TokenBar now uses the current `cross-tool-dev-workflow` contract. The active
project language is the root document set described in `DOCUMENTATION.md`,
including `UNIVERSAL_EXECUTION_RULES.md` and
`PLATFORM_ACTIVITY_LOG.md`. Legacy append-only task and handoff material is
preserved under `docs/archive/` and must not define current scope, acceptance
criteria or executor assignment. Code writing is serial; only read-only
exploration and independent review may run in parallel.

This is the TokenBar project adaptation of the private
`shawnqd/ai-skills-vault/workflows/cross-tool-dev-workflow` repository. The
Windows/macOS branch boundary remains governed by `docs/PROJECT_LINES.md`.

记录用户确认过、后续终端必须继承的产品、架构、安全、费用和破坏性操作决定。按时间倒序追加。

---

## 2026-07-25 — 浮窗（tray flyout）全量重写启动；Settings 双路径 bug 确认存在但本轮不处理

用户判定上一轮浮窗开发（密度分层 UI）出现"重大开发失误"（验证死循环，详见
`AGENT_HANDOFF.md` 历史 checkpoint），要求对浮窗做完全重新开发，不在原有未验证代码基础上改。

**处理过程**：
1. 未提交改动（Part A 缩放/最小最大宽高修复，已用真实 Win32 拖拽验证 + Part B 密度分层 UI，
   未验证成功）已用 `git stash push -u` 存档（stash message: "pre-flyout-rewrite backup..."），
   工作区回到干净 HEAD，未硬删除，可恢复。
2. 在干净 HEAD 基线上做了一次全量窗口路由审计（explorer 子 agent），确认真实存在旧路径浮窗
   （不只是上一轮已知的 `proof_harness.rs` 路由 bug）：
   - `main` 窗口仍保留渲染 `SurfaceMode::TrayPanel` 的完整能力（`surface.rs`），是拆分出独立
     `flyout` 窗口之前的旧共享窗口实现残留；`commands::set_surface_mode`（`commands/surface.rs`）
     未将 `TrayPanel` 排除在允许列表外，理论上前端可调用出这条旧路径。
   - `App.tsx` 的 `SurfaceRouter` 里 `trayPanel` case（旧路径的挂载点）与正确的 `flyout` 窗口
     分支并存，前者本该是死代码，一旦旧路径被触发就会变成活的。
   - 明确死代码（带 `#[allow(dead_code)]`，生产无调用方，确认为旧共享窗口实现残留，
     可删除）：`shell/transition.rs::schedule_startup_tray_panel_reveal_fallback` /
     `should_force_tray_panel_reveal`、`state.rs::arm_startup_tray_reveal`、
     `shell/window.rs::hide_to_tray_state`。
3. **重写边界（已与用户确认）**：
   - 窗口路由层（上述 `SurfaceMode::TrayPanel` 旧路径 + 死代码 4 处 + `set_surface_mode` 缺
     guard）：**删除，不是重写**——去掉整个错误分支，只留 `flyout` 一条路。
   - `shell/flyout_window.rs` 本身（定位/尺寸/最小最大宽高/DWM 去抖动）：复用 stash 里已验证的
     Part A 修复，不重新发明。
   - 浮窗内容层（`TrayPanel.tsx` + `MenuCard.tsx` 密度分层渲染 + `styles.css` 相关规则）：
     **完全重写**，不在上一轮未验证代码基础上改。

**同源但本轮不处理的问题**：审计中同时发现 Settings 窗口存在与浮窗同构的双路径问题——
`CODEXBAR_PROOF_MODE=settings` 走 `shell::transition_to_target(..., SurfaceMode::Settings, ...)`，
在 `main` 窗口内嵌渲染（`decorations: true`，原生标题栏）；真实点击托盘菜单"设置"/"关于"走
`shell::settings_window::open_or_focus`，打开独立 `settings` 窗口（`decorations(false)` +
自定义标题栏）。两条路径标题栏样式会冲突（原生栏叠加自定义栏）。**用户明确决定本轮只处理浮窗，
Settings 这个问题记录在案、暂不处理，留给后续任务处理**——后续终端接手时审计范围见
`proof_harness.rs`（`activate` 的 `CODEXBAR_PROOF_MODE=settings` 分支）与
`shell/settings_window.rs`。

## 2026-07-23 — 采用跨终端开发交接文档体系，落地在 platform/windows（不是 main）

用户要求按其私有仓库 `shawnqd/ai-skills-vault` 中 `workflows/cross-tool-dev-workflow/` 的通用
开发流程模板改造本项目的交接文档。该模板默认假设单一 `main` 工作分支、不新建分支；但本仓库按
`docs/PROJECT_LINES.md` 同时维护 `platform/macos` 与 `platform/windows` 两条独立产品线，
`main` 按仓库既有规则只存放跨线共享文档，不放应用代码。

**决定**：交接文档集（`COLLABORATION.md`/`CURRENT_TASK.md`/`AGENT_HANDOFF.md`/
`PROJECT_STATUS.md`/`CODE_REVIEW.md`/`DECISIONS.md`/`logs/dev_audit.jsonl`）落地在
`platform/windows` 分支，随当前活跃分支一起维护；"单一写入者"规则的落地方式是"每条平台线的
当前活跃分支只允许一个写入者"，而不是强制整个仓库只用 `main`。`platform/macos` 线尚未采用
这套体系，如后续需要，由用户决定是否同步。

## 2026-07-21（历史，追认） — 授权 Claude Code 在本机自行编译/运行桌面应用

用户明确表示"完成后你直接开始编译"，此后 Claude Code 在本会话中被授权使用
`pnpm tauri:build:debug` 自行构建并启动 `codexbar-desktop-tauri.exe` 进行验证，不再是
只改代码、等待用户手动编译。构建产物、构建命令与关闭正在运行实例的方式（`Stop-Process`，
不使用被拦截的 `taskkill`）记入本项目的 `AGENT_HANDOFF.md` 历史 checkpoint。

## 2026-07-23（本轮确认的产品行为决定）

- **显示模式的作用域**：菜单栏显示模式（详细/紧凑/极简）只影响"全部服务商"概览列表；单独
  打开某个服务商的详情视图（托盘详情、设置页预览卡）必须始终展示完整内容，不受该设置影响。
- **"用量预测"与"进度"合并展示**：不再作为两个独立区块并列展示；在存在"进度"数据的场景下，
  用量预测的关键信息（预计支撑到重置 / 预计耗尽时间的小时数）并入"进度"块的跑道行，避免信息
  重复；没有进度数据的服务商仍保留独立的预测框，不丢信息。该合并的具体视觉方案（跑道条 /
  Runway，含仪表图标、彩色阶段 chip、单条进度条 + 预期标记）由用户从三套 HTML 原型中选定。

## 历史决定（引用自 `docs/PROJECT_LINES.md`，不在此重复维护）

- Windows 线的直接代码来源是 `Finesssee/Win-CodexBar`；`steipete/CodexBar` 仅是历史灵感来源，
  不是任何一条线可直接合并的代码源。
- 不跨线（macOS ↔ Windows）推断窗口、托盘、Cookie、认证、更新或 UI 行为；相同名称不代表相同
  架构或能力。
- Windows Chrome/Edge 的 App-Bound Encryption 是 Windows 线独有的浏览器安全约束，修复方案必须
  先在 `Finesssee/Win-CodexBar` 查证，不得把 macOS Keychain 方案当作可直接移植的解法。
