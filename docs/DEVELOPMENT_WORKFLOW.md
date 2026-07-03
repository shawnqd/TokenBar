# 开发流程

## 当前结论

本项目同时保留两套流程说明，但同一时间只能启用一种流程。

当前唯一生效流程：**临时流程 B：GPT 从 GitHub 分支 / PR 审核，Code / OpenCode 总控开发。**

硬规则：**无论当前审查方是 Codex 还是 GPT，开发执行方都必须走分支 + PR，禁止直接在 `main` 开发。**

原流程 A 保留为正式流程，等 Codex 额度恢复后再切回；切换前必须更新本文件和 `PROJECT_STATUS.md`。

## 流程 A：正式流程（Codex 可用时）

### 角色分工

| 角色 | 职责 |
|---|---|
| 用户 | 定目标、边界、取舍、最终确认 |
| Codex | 项目经理 / 产品经理 / 最终审查 |
| Code / OpenCode | 总控开发与具体执行 |
| GPT | 辅助审查、提示词、GitHub 状态读取 |

### 工作方式

1. Codex 先审查需求、仓库状态和项目文档。
2. Codex 输出开发任务书，不直接写代码。
3. Code / OpenCode 按任务书创建分支并执行。
4. Code / OpenCode 完成后开 PR。
5. Codex 审查 PR / diff / 测试 / 文档状态。
6. 用户确认后合并。

### 使用条件

仅在 Codex 额度充足、能稳定承担项目经理和最终审查时启用。

## 流程 B：临时流程（当前启用）

### 启用原因

Codex 暂时额度不足，项目不能停滞，因此临时切换为 GPT 审核 GitHub 分支 / PR。

### 角色分工

| 角色 | 职责 |
|---|---|
| 用户 | 定目标、边界、取舍、最终确认 |
| GPT | 从 GitHub 读取分支 / PR / diff，做审查并给 Go / No-Go |
| Code / OpenCode | 总控开发、具体执行、建分支、开 PR |
| Codex | 暂停作为项目经理和最终审查方 |

### 工作方式

1. Code / OpenCode 不得直接在 `main` 开发。
2. 每个任务必须创建独立分支。
3. 分支命名建议：
   - `feature/p0-usage-log`
   - `feature/p0-config-refresh`
   - `feature/p0-tool-fit`
   - `fix/web-502-assets`
   - `docs/*`
4. 完成后开 PR，目标分支为 `main`。
5. PR 描述必须包含：
   - 改动范围
   - 涉及文件
   - 验证命令
   - 测试结果
   - 剩余风险
   - 是否更新状态文档
6. GPT 从 GitHub 审查 PR，不参与本地直接开发。
7. GPT 给出 `Go / No-Go / 需返工`。
8. 用户最终确认是否合并。

### 当前 P0 优先队列

1. `UsageLog` 录入 / 编辑 / 删除闭环。
2. 今日 / 本周 / 本月消耗口径落地。
3. 配置状态页增加真实只读检测刷新。
4. 检测结果写回 `qb_credential_statuses`。
5. 平台 / 模型补录“适合工具”字段。
6. 排查 `internal/web` 测试 `502` 与静态资源异常。
7. 同步 `PROJECT_STATUS.md` / `TODO.md` / `CHANGELOG.md` / `AGENT_HANDOFF.md`。

## 同一时间只允许一种流程生效

| 状态 | 生效流程 |
|---|---|
| Codex 额度充足 | 流程 A |
| Codex 额度不足 | 流程 B |

切换规则：

1. 切换前必须更新本文件。
2. 切换前必须更新 `PROJECT_STATUS.md`。
3. 切换时必须明确“当前唯一生效流程”。
4. 不允许 Codex 和 GPT 同时作为最终审查方。
5. 不允许绕过 PR 直接合并到 `main`，除非用户明确授权紧急修复。

## 之前是否走分支合并

按当前 GitHub 可见状态判断：

1. 仓库默认分支为 `main`。
2. 当前未查到已有 PR。
3. 分支搜索未返回额外分支。

因此，之前开发**不能确认走过 GitHub 分支 + PR 合并流程**；更稳妥的判断是：此前大概率以直接提交 / 本地整理后入仓为主。

后续从本文件加入开始，统一按“分支开发 + PR 审核 + 用户确认合并”执行。

如果本地存在未推送分支或历史记录，以本地执行结果为准：

```bash
git branch -a
git log --oneline --graph --decorate --all --max-count=30
git ls-remote --heads origin
```

## 禁止事项

1. 不在 `main` 直接开发。
2. 不保存真实 API Key。
3. 不保存 Cookie。
4. 不保存账号密码。
5. 不进入 P1 Provider 自动适配，除非 P0 已验收。
6. 不做网页登录余额抓取。
7. 不破坏 onWatch 原有能力。
8. 不把用户当多个 agent 的传话中间层。

## PR 审核清单

每个 PR 至少检查：

1. 是否只改本轮范围。
2. 是否命中对应 P0 缺口。
3. 是否新增或修改敏感信息。
4. 是否碰到禁改区。
5. 是否保留 onWatch 原能力。
6. 是否有最小验证命令和结果。
7. 是否同步状态文档。
8. 是否能明确合并风险。

## 当前执行口径

当前执行口径为：

```text
GPT 从 GitHub 分支 / PR 审核；Code / OpenCode 总控开发；用户最终确认合并。
```
