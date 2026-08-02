# 汉化交付汇报 — 2026-07-10

## 分支 / PR

未做 git 操作。所有改动在本地工作区，待审核后创建分支 `code/full-zh-cn` 提交。

## 改动文件（共 10 个）

### Rust 后端（4 个文件）

| 文件 | 变更 |
|------|------|
| `rust/src/locale.rs` | +33 个 LocaleKey 枚举条目（FloatBar×23 + About×7 + SettingsWindowTitle×1 + CodexLocalLogs×4 已预存） |
| `rust/src/settings/types.rs` | `#[default] English` → `#[default] Chinese` |
| `rust/src/settings/tests.rs` | `test_language_defaults_to_english` → `_to_chinese`；legacy 缺字段反序列化测试 `ui_language: "english"` → 空字段默认 Chinese |

### 本地化文件（6 个 FTL）

| 文件 | 变更 |
|------|------|
| `rust/src/locale/zh-CN.ftl` | ① 补齐 26 条 Phase 6d 凭据/Cookie 英文残留 → 中文；② +31 条新键中文翻译 |
| `rust/src/locale/zh-TW.ftl` | ① 补 7 条预存缺失键（CodexLocalLogs×4, FloatBar×3）；② +31 条新键繁体翻译 |
| `rust/src/locale/en-US.ftl` | +31 条新键英文原文 |
| `rust/src/locale/ja-JP.ftl` | +31 条新键日语翻译 |
| `rust/src/locale/ko-KR.ftl` | +31 条新键韩语翻译 |
| `rust/src/locale/es-MX.ftl` | +31 条新键西班牙语翻译 |

### React 前端（3 个文件）

| 文件 | 变更 |
|------|------|
| `apps/desktop-tauri/src/i18n/keys.ts` | +33 条 TypeScript LocaleKey 同步 |
| `apps/desktop-tauri/src/floatbar/SettingsSection.tsx` | **19 处**硬编码英文标签/描述/aria-label → `t()` |
| `apps/desktop-tauri/src/surfaces/settings/tabs/AboutTab.tsx` | **13 处**硬编码英文（更新状态/按钮/版权/加载）→ `t()`，复用已有 BannerDownloadButton/BannerViewRelease/BannerInstallRestart/BannerUpdateFailedPrefix |
| `apps/desktop-tauri/src/surfaces/Settings.tsx` | **3 处**窗口控件 `"Minimize"`/`"Close"`/`"CodexBar Settings"` → 复用已有 `WindowMinimize`/`WindowClose` + 新键 `SettingsWindowTitle` |

## 汉化覆盖

| 维度 | 结果 |
|------|------|
| FTL 总键数 | **587** keys（从 556 → 587） |
| zh-CN 翻译覆盖率 | **100%**（587/587） |
| 所有 6 语言包键一致性 | **100%**（全部 587 keys，零漂移） |
| 浮动栏设置区 | **0 处硬编码英文**（此前 19 处） |
| 关于页面更新 UI | **0 处硬编码英文**（此前 13 处） |
| 设置窗口标题栏 | **0 处硬编码英文**（此前 3 处） |

## 默认语言行为

- 新用户 / 无 `ui_language` 字段的旧配置 → **简体中文**
- 用户显式保存的语言设置 → 不受影响（serde 反序列化优先于 `#[default]`）
- 语言选择器完整保留，支持 6 种语言自由切换

## 保留英文项及原因

| 类别 | 项 | 原因 |
|------|-----|------|
| Provider 名称 | Claude, Codex, Gemini, DeepSeek 等 | 产品名，不翻译 |
| 模型名 | Opus, Sonnet, GPT-4o 等 | 模型名，不翻译 |
| API/CLI 参数 | `__Secure-session`、`gemini auth login` 等 | 命令行参数/标识符 |
| 链接标签 | GitHub、Website、Original Project | 专有名词 |
| 产品名 | CodexBar | 品牌名 |

## 验证命令与结果

```
cargo test --manifest-path rust/Cargo.toml --lib     → 526 passed, 0 failed ✅
cargo fmt --manifest-path rust/Cargo.toml --all --check → OK ✅
pnpm run check-locale                                  → 587 keys 一致 ✅
pnpm test                                              → 29 files, 135 tests ✅
```

## 未验证项

- `./scripts/dev.ps1` 实际构建启动 — 未执行（需 Windows GUI 环境）
- Windows 上手动物理托盘/设置检查 — 未执行
- `cargo clippy` — 未执行

## 剩余待处理（后续迭代）

按优先级排列的已知硬编码英文残留，均不在本轮交付范围内：

| 优先级 | 文件 | 残留量 | 说明 |
|--------|------|--------|------|
| P1 | `PaceDetailsChart.tsx` | 3 处 | 图表图例 "Average so far"/"Ideal pace"/"Projection" |
| P1 | `GeneralTab.tsx` | 6 处 | 刷新频率下拉选项 "Manual"/"1 minute" 等 |
| P1 | `FloatBar.tsx` | 1 处 | 返回 `"now"` 字面量 |
| P2 | `notifications.rs` | ~12 处 | 系统 Toast 通知标题与正文 |
| P2 | `bridge.rs` | ~7 处 | Claude 错误消息 |
| P3 | CLI 输出 | ~40 处 | account/cost/config/usage.rs 的命令行输出 |
| P3 | 窗口标题 | 2 处 | `shell/settings_window.rs`、`floatbar/window.rs` |
