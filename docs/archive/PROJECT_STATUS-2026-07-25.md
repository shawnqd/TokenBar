# PROJECT_STATUS

## Documentation system (2026-07-26)

The project now follows the current cross-tool workflow contract: nine stable
project documents plus the single human-readable `PLATFORM_ACTIVITY_LOG.md`.
Legacy task and handoff history is preserved under `docs/archive/`; it is not a
source of current scope. Code writing is serial; exploration and independent
review may run in parallel.

## 当前阶段

Windows（Tauri/React/Rust）端的截图驱动 UI 迭代 QA，聚焦托盘浮窗与设置页的用量卡片。本轮
（2026-07-23）另外把跨终端开发交接文档体系（`COLLABORATION.md` / `CURRENT_TASK.md` /
`AGENT_HANDOFF.md` / `PROJECT_STATUS.md` / `CODE_REVIEW.md` / `DECISIONS.md` /
`logs/dev_audit.jsonl`）首次引入本仓库的 `platform/windows` 分支。

## 已完成

- 大规模视觉/交互 QA（详见 git log 与历史 `AGENT_HANDOFF.md` checkpoint，涵盖设置窗口宽度统一、
  开关/分段控件重做、浅色主题完整补齐、托盘浮层透明化与圆角修复、Providers 侧栏拖拽等，见仓库根
  任务追踪历史）。
- 本轮（2026-07-23）：Grok 图标与用量条品牌名修复、热门模型按周期计算、Codex 本地用量扫描器
  真实 bug 修复（恢复会话记录归档问题）、中文数量单位、输出速度折线图恢复、会话/周额度视觉拆分、
  显示模式作用域修正（只影响概览，不影响单服务商详情）、"进度 + 用量预测"合并为 Runway 设计、
  卡头品牌图标、浅色主题 CSS 变量缺口修复、`.menu-card__pace` flex-gap 缺失修复、凭据存储区块
  及周边 26 个键补齐汉化。
- 跨终端交接文档体系落地到 `platform/windows` 分支（本次提交）。

## 进行中

- Runway 进度块（.menu-card__pace）样式迭代（两轮：去灰底/减字重 + 配色中性化 chip去色/fill统一蓝/runway去色）：代码与自测完成，reviewer 独立审查 Go（已修复图标去色问题），待用户真机视觉确认（仅改 styles.css，去灰底/减字重/降字号），待用户真机视觉确认。
- **浮窗（tray flyout）全量重新开发**（2026-07-25，Claude Code）：**实现 + 集成 + 真机验证已
  完成**，待用户视觉确认后授权提交。上一轮密度分层 UI 实现出现验证死循环，用户判定为重大
  开发失误，要求完全重新开发。旧改动已 `git stash` 存档，窗口路由全量审计后与用户确认重写
  边界（Rust 层删旧路径 / `flyout_window.rs` 复用已验证修复 / 前端内容层完全重写），用户
  确认浮窗外壳选 B 方案（统一分区）+ 密度分层规格 + 作用域纠正（三档切换只影响概览列表，
  单服务商详情/设置页预览卡始终完整展示）。两个 complex-executor 子 agent 并行实现，主会话
  补齐 4 处遗留项（详细档漏了额外重置次数字段、prearm-gesture 缩放防误关闭机制）并做真机
  截图验证。`cargo test` 322 passed、`vitest` 159 passed、`tsc`/locale-drift 干净。真机截图
  确认：概览列表（详细档）、单服务商详情、设置页预览卡（确认未受影响）。紧凑/极简档因 UI
  自动化导航效率问题未做真机截图，仅代码审查+单测覆盖。详情见 `AGENT_HANDOFF.md` 2026-07-25
  checkpoint (2)。

## 已知问题（待后续处理）

- **Settings 窗口双路径**：与浮窗同构的架构问题，proof-mode 走 `main` 内嵌渲染（原生标题栏），
  真实点击走独立 `settings` 窗口（自定义标题栏），两者视觉冲突。用户明确决定本轮浮窗重写不
  处理，留给后续任务。详情与代码定位见 `DECISIONS.md` 2026-07-25 条目。
- **Vite dev server 前端可靠性**：debug 构建从 `http://localhost:1420`（`pnpm run dev` 起的独立
  node 进程）实时加载前端。该 node 进程长时间存活（跨多轮编辑）后会进入"内存里的模块缓存与磁盘
  文件不一致"的坏状态——2026-07-25 实测：`git stash` 已把源码改回干净版本，同一 Vite 进程仍对
  新启动的 exe 吐出 stash 前的旧内容；杀掉该 node 进程重启后才恢复正常。根因疑似 Windows 上
  chokidar 文件监听在批量文件变更（如 git 操作）时丢事件，或 Vite 长时间运行后模块图状态卡死；
  未定位到一次性代码修复，当前只能靠"改动看似未生效时先重启 Vite 进程"的验证纪律规避（已记入
  Claude Code 侧 memory）。**用户已确认这个问题后续需要真正解决**（而不只是靠重启规避），候选
  方向待评估：`vite.config.ts` 里 `server.watch.usePolling`、每次调试前脚本自动重启 Vite、或
  排查是否有更根本的缓存配置问题。留待后续任务处理，不在本轮浮窗重写范围内。

## 阻塞项

无（浮窗重写已完成实现+验证，等待用户最终视觉确认，非技术阻塞）。

## 下一步

1. 用户对浮窗重写结果做最终真机视觉确认（尤其紧凑/极简两档，本轮未做真机截图）。
2. 确认后授权 commit（浮窗重写这一轮，以及仍待确认的 pace 两轮改动，视用户意愿一并或分开）。
3. 后续任务候选（均已记录，非本轮阻塞）：Settings 窗口双路径问题、Vite dev server 可靠性
   问题、`should_force_tray_panel_reveal`/`hide_to_tray_state` 死代码彻底清理、"本周实际用量"
   locale key 补充（如果用户在意这个具体措辞）。
