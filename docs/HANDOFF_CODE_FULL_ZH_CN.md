# Code 交接：Windows 完整简体中文化

## 任务

完成 `platform/windows` 的**用户可见界面简体中文化**，并将本定制分支的首次启动默认界面语言设为 `简体中文`。保留语言选择器和全部既有语言包，不把项目退化为只能显示中文。

## 目标

用户以简体中文使用托盘面板、设置、弹出窗口、右键菜单、状态与错误提示时，不应再看到可翻译的英文残留；Provider 名称、产品名、模型名、API/CLI 参数、路径与代码术语除外。

## 分支与提交

- 基线：`platform/windows`。
- 工作分支：从基线新建 `code/full-zh-cn`。
- 完成后提交、推送，并创建目标为 `platform/windows` 的 PR。
- 禁止自行合并；由 Codex 审核测试、范围与隐私边界后决定是否合并。

## 先读

1. `AGENTS.md`
2. `apps/desktop-tauri/src/i18n/LocaleProvider.tsx`
3. `apps/desktop-tauri/src/surfaces/settings/tabs/GeneralTab.tsx`
4. `rust/src/locale.rs`
5. `rust/src/locale/zh-CN.ftl` 与其他 `rust/src/locale/*.ftl`
6. `apps/desktop-tauri/src/types/bridge.ts`

## 做

1. 审计下列位置的用户可见文本：
   - `apps/desktop-tauri/src/` 的托盘面板、浮动条、设置、Provider 卡与错误提示。
   - `apps/desktop-tauri/src-tauri/src/` 的托盘菜单、窗口标题、命令层用户提示。
   - `rust/src/` 中由桌面 UI 或 CLI 直接展示的状态、错误和动作标签。
2. 所有可翻译文本接入现有 Fluent `LocaleKey` / `LocaleProvider` 体系；不得只在 React 或 Rust 一侧硬编码中文。
3. 补齐 `zh-CN.ftl` 的缺失或英文残留。若新增 key，必须同步全部现有语言包，并通过 locale 一致性检查。
4. 把首次启动默认 `uiLanguage` 调整为 `chinese`；已有用户显式保存的语言设置必须继续优先，不得被覆盖。
5. 增加或扩展测试，至少覆盖：
   - 简体中文语言包包含全部 `LocaleKey`。
   - 设置为 `chinese` 时关键托盘/设置文案为中文。
   - 新用户默认语言为 `chinese`。
6. 在 PR 描述中列出仍保持英文的项及其原因（例如 Provider 名称、CLI 参数或不可本地化的外部服务原文）。

## 不做

- 不改 Provider 获取逻辑、额度算法、网络请求、Cookie 导入、DPAPI/凭证存储或权限。
- 不删除其他语言，不移除语言选择器，不批量重构 UI。
- 不改变 `CodexBar`、`DeepSeek`、`OpenCode`、模型 ID、API 名称、CLI 命令、配置键和文件路径。
- 不做托盘悬浮交互、视觉重设计、Provider 裁剪或上游同步；这些是独立任务包。
- 不合并 PR、不推送 `main` 或 `platform/macos`。

## 验收标准

- 托盘面板、右键菜单、弹出窗口、设置页、常规状态和错误提示在简体中文下没有可翻译英文残留。
- 首次启动默认简体中文；切换为其他语言后重启仍尊重用户选择。
- `pnpm run check-locale` 通过。
- `pnpm test` 通过；相关 Rust locale / settings 测试通过。
- `cargo fmt --all --check` 通过；如运行 `clippy`，记录命令与结果。
- `./scripts/dev.ps1` 能构建并启动；在 Windows 上手动检查托盘、设置和至少一个错误/未配置状态。
- 不记录、不输出、不提交真实 API Key、Cookie、Token 或个人路径。

## 交付回报格式

```text
分支 / PR：
改动文件：
汉化覆盖：
默认语言行为：
保留英文项及原因：
验证命令与结果：
未验证项：
风险：
```
