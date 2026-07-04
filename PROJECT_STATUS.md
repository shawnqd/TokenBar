# 项目状态

## 当前结论

P0 急用闭环已完成（P0-1~P0-6），等待 GPT 复审 / 合并。

当前代码已经具备：

- `qb_*` 数据表和台账层 store
- 总览 / 套餐 / Provider / 使用记录 / 配置 / 风险 / 导入导出页面
- 平台 / 套餐 / 额度桶 / 风险备注 / 模型 / 使用记录录入闭环
- 推荐、样例数据、脱敏导入导出
- 总览页与使用记录页的 today / week / month 三段消耗
- 配置状态手动刷新（Base URL 只读探测）

经 R1-R5 返工后，事实源文档与代码口径已一致。剩余失败仅为 onWatch 原有测试在 Windows 上的预存平台不兼容（Setsid/systemd/HOME 语义/getCredentialsFilePath），属底座问题，非台账 P0 责任。

## 当前阶段

- 阶段：P0 补缺完成
- 状态：`p0_complete`
- 日期：2026-07-04

## 本轮结论（P0-1~P0-6，feature/p0-urgent-mvp）

P0 台账层全部缺口已补齐并通过验证：

- P0-1 UsageLog 新增/编辑/删除闭环 ✅
- P0-2 today/week/month 三段消耗口径 ✅
- P0-3 适合工具字段录入与展示 ✅
- P0-4 配置状态手动刷新（只读探测 Base URL，不存敏感凭据） ✅
- P0-5 web 502 排查（根因：环境代理；测试前关闭 HTTP_PROXY 即通过） ✅
- P0-6 四份交接文档同步 ✅

代码层面 P0 已可交付。剩余失败仅为 onWatch 原有测试在 Windows 上的预存平台不兼容（Setsid/systemd/HOME 语义/getCredentialsFilePath），属底座问题，非台账 P0 责任，已用 `git stash` 证实与本轮无关。

## 当前开发流程

- 当前唯一生效流程：GPT 从 GitHub 分支 / PR 审核，Code / OpenCode 总控开发。
- 原 Codex 项目经理 / 最终审查流程保留，但因 Codex 额度不足临时暂停。
- 同一时间只允许一种流程生效；恢复 Codex 流程前，必须更新 `docs/DEVELOPMENT_WORKFLOW.md` 和本文件。
- 后续开发默认必须走分支 + PR，不直接在 `main` 开发。

## 已确认缺口（已全部补齐）

### 缺口 1：UsageLog 没有录入闭环 → ✅ 已补
新增 UsageNewForm/UsageEditForm/UsageSave/UsageDelete + qb_usage_form.html + 4 路由 + 明细行编辑/删除按钮。

### 缺口 2：只读配置检测仍是展示态 → ✅ 已补
新增 ConfigRefreshAction：对每平台 Base URL 做 5s 只读 HTTP 探测（不发凭据），分类 healthy/warning/danger，写回 qb_credential_statuses（upsert）。

### 缺口 3：适合工具字段未落地 → ✅ 已补
PlatformSave 读 supports_tools_json、ModelSave 读 tool_fit_json；两个表单加输入；config/plans/overview 三页展示。

### 缺口 4：状态文档与代码状态漂移 → ✅ 已补
PROJECT_STATUS/TODO/CHANGELOG/AGENT_HANDOFF 同步，撤销 p0_complete 错误口径后再正确标记。

## 已完成部分

- 平台管理：已完成
- 套餐管理：已完成
- 额度桶管理：已完成
- 风险备注：已完成
- 模型管理：已完成
- 首页推荐：已完成最小版
- 导入 / 导出：已完成最小闭环
- 样例数据：已完成
- 新页面与新路由：已完成

## 待补后才可宣称 P0 完成

全部已完成 ✅：
1. ✅ UsageLog 录入 / 编辑 / 删除闭环
2. ✅ `today / week / month` 三段消耗口径落地
3. ✅ 配置状态页接入真实只读检测刷新动作
4. ✅ “适合工具”字段进入录入表单和展示页
5. ✅ 状态文档与代码实际状态同步

## 技术问题（已知）

1. Go 1.26.4 已装到 `~/.local/go`（zip 下载 + .NET 解压）。
2. web 测试 502 根因已定位：环境代理拦截 `0.0.0.0` 请求；测试前 `HTTP_PROXY=$null HTTPS_PROXY=$null NO_PROXY` 含 `0.0.0.0` 即通过（详见 AGENT_HANDOFF.md）。
3. onWatch 原有测试在 Windows 有若干预存平台不兼容（Setsid/systemd/HOME 语义/getCredentialsFilePath），与本轮台账改动无关，属底座问题。

## 下一步

等待 GPT 复审 PR #2；复审通过后合并到 `main`，再进入 P1 规划：

- Provider 自动适配
- `ccusage` 导入
- 趋势图
- 推荐增强
