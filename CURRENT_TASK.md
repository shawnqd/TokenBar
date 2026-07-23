# CURRENT_TASK

## 目标

pace 块（`.menu-card__pace`）样式迭代，两步合并为一轮，未提交：

1. **已完成（上一轮，general subagent 执行）**：去掉 pace 块灰底/边框、降低字号字重，
   解决"底部灰色块 + 字体偏大拥挤"。改动已在工作区（`styles.css` 未提交）。
2. **待执行（本轮，已派 executor 完成，待 reviewer 审查）**：配色中性化。pace 块是卡片里唯一用四色药丸
   （slow 蓝 / steady 绿 / racing 橙 / burning 红）+ 实色 fill 的元素，去灰底后花哨感
   放大，与卡片整体克制调性（hero 配额条蓝色渐变、cost/charts 中性灰文字）不协调。
   用户选定方向：**chip 去色 + fill 统一蓝**。

## 执行方与角色

- 总控 / 任务包撰写：opencode 主会话（glm-5.2）
- 执行方：**executor subagent**（sensenova/deepseek-v4-flash）-- 用户明确要求重启后派
  executor；当前会话因 agent 配置启动后注入、Task 工具未识别 executor，需重启 opencode
  生效后由新会话主会话派发
- 审查：执行方自测 -> 用户真机视觉确认
- commit / push / merge：仅用户明确授权后

## 范围

- `apps/desktop-tauri/src/styles.css`，仅 `.menu-card__pace*` 配色相关规则
- 组件 JSX（MenuCard.tsx）不动（data-pace 属性保留，只改 CSS 呈现）

## 配色中性化方案（executor 照此执行）

### A. chip 去色（用户明确要求）
- `.menu-card__pace-chip[data-pace="slow/steady/racing/burning"]` 4 条规则
  （`styles.css` 约 5467-5474）：删除其 `background`/`color` 覆盖，让 chip 回落到
  统一中性样式：`background: var(--menu-popover-divider)`（或等价中性半透明）、
  `color: var(--text-secondary)`。chip 内文字（远落后/正常/超前/超前过多 + Δ%）保留，
  状态语义靠文字传达，不靠颜色。
- 若 chip 无默认背景规则，需在 `.menu-card__pace-chip` base 规则（约 5457）补中性背景。

### B. fill 统一蓝（用户明确要求）
- `.menu-card__pace-fill[data-pace="slow/steady/racing/burning"]` 4 条规则
  （约 5503-5506）：删除，让 fill 回落到默认 `background: var(--usage-bar-normal)`
  （蓝），与 hero 配额条（`provider-quota__fill` 同色）视觉统一。

### C. runway-status 文字去色（为一致性一并处理，"符合整体样式"的合理延伸）
- `.menu-card__pace-runway[data-state="ok"] .menu-card__pace-runway-status`
  （约 5527）：color 从 `var(--pace-steady-fg)` 改 `var(--text-secondary)`
- `.menu-card__pace-runway[data-state="warn"] .menu-card__pace-runway-status`
  （约 5531）：color 从 `var(--pace-racing-fg)` 改 `var(--text-secondary)`
- 状态图标（✓/⚠）保留原语义色作为唯一彩色锚点（不动 svg color 继承之外的处理）；
  若图标也跟着文字去色，则 runway 行完全中性，仅靠图标形状区分状态

### 不动
- track（`--usage-bar-track`）、marker（`--text-primary`）保持
- `--pace-*` 主题变量定义不删（其他地方可能引用，且不破坏向后兼容）
- 浅色/深色主题变量不改动
- 上一轮已完成的去灰底/字号字重改动保留（已在工作区）

## 允许修改

- `apps/desktop-tauri/src/styles.css`（仅 `.menu-card__pace*` 配色规则）

## 禁止修改

- `MenuCard.tsx`、`MenuCard.test.tsx`（本轮不动 JSX/测试）
- `rust/**`、locale / ftl
- `--pace-*` / `--usage-bar-*` 主题变量定义
- pace 块以外的任何样式
- `platform/macos`、不新建分支、不 commit/push/merge

## 验收标准

- `npx tsc --noEmit` clean
- `npx vitest run` 全绿
- `node scripts/check-locale-drift.mjs` OK
- tray 浮窗 + settings 预览卡：pace 块 chip 为中性灰、fill 为蓝色、与 hero 配额条/cost
  块视觉协调，不再花哨
- 浅色 + 深色主题都不破
- PopOut 面板不被破坏

## 验证命令

- `npx tsc --noEmit`（在 `apps/desktop-tauri`）
- `npx vitest run`（在 `apps/desktop-tauri`）
- `node scripts/check-locale-drift.mjs`（在 `apps/desktop-tauri`）

## 停止条件

- 若发现配色受 JSX data-pace 结构约束无法纯 CSS 解决，停止回报
- 具体 hex 取值交用户确认；executor 给方案不擅自定终稿
- 主观体验取舍不明确时停止并交回用户
