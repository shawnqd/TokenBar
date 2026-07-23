# CODE_REVIEW

## 说明

本文件当前只包含**执行方自查**结果（Claude Code / Sonnet 5，2026-07-23），不是独立 Reviewer
会话的输出。按 `COLLABORATION.md` 的角色模型，审查方应是与执行方不同的会话/模型；本仓库目前
还没有为这批改动跑过那一轮，如实记录，不冒充已完成审查。

## Findings（自查发现，按严重程度排列）

1. **[已修复] 浅色主题下进度条/预测色系不可见** — `--usage-bar-track` 与四组
   `--pace-*-bg/-fg` 变量只在 `[data-theme="dark"]` 定义，`[data-theme="light"]` 从未补齐。
   用户截图暴露；已用无头浏览器计算样式实测确认修复后解析为真实颜色。
2. **[已修复] `.menu-card__pace` 缺少 `display:flex`** — 挂载的 `.menu-card__group` 类在
   样式表里没有任何规则，`gap:9px` 在块级容器上静默失效，实测三行间距为 0px。补
   `display:flex; flex-direction:column;` 后实测恢复为 9px。
3. **[已修复] 一次越界编辑** — 在"用量预测和进度合并"的需求下，误将预测框直接删除而非合并，
   且未经用户确认就改了行为。用户当场纠正，已改为合并方案并撤销原编辑；教训记入用户的
   `feedback-diagnose-before-workaround` 记忆条目。
4. **[已修复] 托盘强制隐藏进度块的旧 CSS 回归** — 进度块合并了用量预测后，托盘里原有的
   `display:none` 规则会导致两者一起从浮窗消失；已移除该规则。
5. **[待独立复核] CSS 级联改动面较广**（`styles.css` 本轮新增/调整约 150+ 行，涉及多个
   `:is(.menu-surface--tray, .provider-detail-live-card)` 覆盖层），建议下一轮由独立
   Reviewer（或另一模型会话）复核是否还有其它选择器因类似"共享类名无规则"的模式静默失效。

## 验证缺口

- 没有独立 Reviewer 会话复核 diff。
- 最后一批修复（浅色主题、flex-gap）只有几何/计算样式实测，没有用户在真机的最终视觉截图确认。
- 没有对深色主题重新做一轮回归截图（本轮改动理论上不影响深色主题取值，但未逐项截图复核）。

## Go / No-Go

**有条件 Go**：代码可以提交（tsc/vitest/cargo check 均干净，且用户已在对话中明确要求"提交到
分支"）。Push/merge 前建议：(a) 用户对最后一批修复做一次真机截图确认，(b) 如时间允许，安排一次
独立 Reviewer 复核 `styles.css` 的级联改动。

## 剩余风险

见 `AGENT_HANDOFF.md` 最新 checkpoint 的"剩余风险"章节，二者保持一致，不重复维护。
