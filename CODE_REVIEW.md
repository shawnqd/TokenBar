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

## 2026-07-30 — TASK-QUOTA-PRESENTATION-TASKBAR-018 阶段一独立审查

审查范围：共享 `quotaDisplay` 数据/格式化层、B/C/D 语义、三组件设置迁移与桥接；不把阶段二/三的 UI、DirectWrite 或任务栏多条目组合当作本阶段通过条件。

### 自动化证据

- `apps/desktop-tauri`: `pnpm test` — 35 个测试文件、197 个测试通过。
- `apps/desktop-tauri`: `pnpm build` — locale 709 keys、TypeScript、Vite 构建通过。
- `rust`: `cargo test --manifest-path rust/Cargo.toml settings::tests` — 60 个设置测试通过。
- `apps/desktop-tauri/src-tauri`: 设置相关测试 11 个通过，`tray_bridge::tests` 25 个通过。

### Findings

1. **[P1][未关闭] 任务栏重置时间设置没有消费路径。** `taskbarResetTimeRelative` 已写入设置和共享映射，但原生任务栏只从 `selected_tray_percents` 取百分比并在 `tray_bridge.rs:459-465` 写入两行文本；任务栏没有读取该字段，也没有经过 `formatResetDisplay`。因此“任务栏可独立选择相对/绝对重置时间”目前是死设置，不能声称 C 已覆盖三组件。
2. **[P1][未关闭] 信息型服务商仍可能被服务商切换条渲染成百分比。** `quotaDisplay.quotaPercentDisplay` 已标出 `isInformational`，`ProviderQuotaBlock` 会正确提前渲染信息态；但 `components/ProviderGrid.tsx:67-68,157-169` 直接使用 `.percent` 绘制轨道和百分比，没有判断 `isInformational`。没有额度的账号（例如无会话的 Claude）仍可能出现 0%/空轨道，违反“不得把余额/信息值伪装成百分比”的约束。
3. **[P2][需确认] 仪表盘键合并托盘浮窗与 PopOut。** `quotaDisplay` 明确把两者映射为同一 `dashboard` 组件。若产品定义中的“仪表盘”确实包含这两个入口，可以保留；若后续要求两者独立，必须新增第四组键，不能继续复用 `dashboard`。

### 结论

**No-Go（有条件）**。共享层、迁移和自动化质量达标，但在修复 P1-1/P1-2 或明确将其移入后续阶段并从阶段一验收承诺中剔除前，不建议交用户验收为“阶段一完成”。本次审查未修改应用代码；仅将交接文档中 DirectWrite 从阶段二纠正到用户确认的阶段三。

## 2026-07-30 — TASK-QUOTA-PRESENTATION-TASKBAR-018 阶段一复审

本节覆盖 Claude 最新修复，supersede 上一节的 No-Go 结论。

### 修复核对

1. **P1-1 已关闭（移除死设置）。** `taskbar_reset_time_relative` 已从 Rust 设置、Tauri bridge 和 TS 类型移除；任务栏当前不显示重置文本，因此不再保留无效开关。Rust `taskbar_has_no_reset_time_mode_setting` 测试确认不会重新持久化该键。若阶段三让任务栏条目显示重置时间，应在阶段三重新设计并补回对应设置。
2. **P1-2 已关闭。** `primaryQuotaState` 统一识别信息型行和余额提供商的合成窗口；`ProviderGrid` 只为真实 quota 绘制轨道，信息型服务商改为明确的非额度标记，并有 TrayPanel 回归测试覆盖。

### 复测证据

- `pnpm test`: 35 个测试文件、204 个测试通过。
- `pnpm build`: TypeScript/Vite 构建通过，locale drift 710 keys 通过。
- Rust 设置测试：61 个通过；Tauri `tray_bridge::tests`：25 个通过；`git diff --check` 通过。

### 最新结论

**Conditional Go：可以交用户做阶段一验收，但不能自动进入阶段二。** 两个 P1 已闭合；用户已确认托盘浮窗与 PopOut 共用 `dashboard` 设置，P2 决策关闭。没有真机视觉验证，本阶段虽不改布局仍需用户确认实际开关行为。若用户要求任务栏也显示并切换重置时间，当前“移除设置”不能算满足需求，应把它作为阶段三任务栏条目设计的一部分重新立项。

### 执行方回应（Claude Code / claude-opus-5，2026-07-30）

两个 P1 均已修复，不是移入后续阶段。

**P1-1 已修复 —— 删除该设置，而不是补一个消费方。** 复核任务包 H 节后确认：任务栏设置页定义的是“开关、字体、宽度、对齐、条目组合、服务商/额度窗口选择和排序”，**没有重置时间模式**；原生状态条也不渲染任何重置文本。我上一轮为了六个字段对称而加的 `taskbar_reset_time_relative` 属于过度设计，正确处理是删掉它，而不是为一个用户没要求的显示项去改任务栏内容契约（那属于阶段三 G 项，且需要真实截图验收）。

已从 `Settings`、`RawSettings`、`SettingsSnapshot`、`SettingsUpdate`、`types/bridge.ts` 及全部测试夹具中移除。类型层面也做了防复发处理：`QuotaPercentContext`（只有 `showAsUsed` + 阈值）与 `QuotaDisplayContext`（额外带 `resetTimeRelative`）拆开，`ResetAwareComponent` 只包含 `floatBar | dashboard`，因此**向任务栏索取重置模式现在是编译错误**，而不是一个没人消费的设置。另加 Rust 测试 `taskbar_has_no_reset_time_mode_setting` 断言该键不会被持久化。

顺带修掉一个同源的真问题：`TaskbarTab` 的预览把 `Codex 59%` / `周额度 18%` 写死，与 `taskbarShowAsUsed` 无关。现在预览走 `quotaPercentContext(settings, "taskbar")` + `quotaPercentDisplay`，与原生条同语义翻转，使该设置在前端也有可见消费方。

**P1-2 已修复。** 新增共享判定 `quotaDisplay.primaryQuotaState(provider)` → `"quota" | "error" | "informational"`。它同时覆盖审查指出的 `isInformational` 行（sub2api 的 `Subscription active` / `No quota data`）**和**审查未点名但同样会漏的余额载体窗口（DeepSeek/MiMo 用非 informational 的合成 0% 窗口把余额塞进 `resetDescription`，仅判 `isInformational` 会漏掉这批）。判定复用 `getProviderBalance().excludeWindows`，与卡片路径同源。

`ProviderGrid` 现在只对 `"quota"` 绘制轨道与百分比；非额度行改为静音的非额度标记，带 `QuotaNoPercentageForProvider` 悬浮说明（六语言齐全），不再出现 0%/空轨道。回归测试覆盖两侧：`quotaDisplay.test.ts` 覆盖四种判定（含“有套餐的余额服务商仍应算真额度”），`TrayPanel.test.tsx` 断言混合列表中只渲染一条轨道且非额度项不含 `0%`。

**P2 保持开放，交回用户决定**，未擅自处置。

复核后验证（全部真实结果）：`pnpm build` 通过；locale 710 keys 通过；前端 35 files / **204** tests 通过；共享 Rust **613** tests + launcher 通过；Tauri **314/314** 通过；两个 workspace `cargo check` 通过；`git diff --check` exit 0。仍未做视觉验证，也不声称视觉验收。
