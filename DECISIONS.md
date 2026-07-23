# DECISIONS

记录用户确认过、后续终端必须继承的产品、架构、安全、费用和破坏性操作决定。按时间倒序追加。

---

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
