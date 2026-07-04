# 开发流程

## 当前结论

本项目同时保留两套流程说明，但同一时间只能启用一种流程。

当前唯一生效流程：**临时流程 B：GPT 从 GitHub 分支 / PR 审核，Code / OpenCode 总控开发。**

硬规则：**无论当前审查方是 Codex 还是 GPT，开发执行方都必须在独立分支工作，禁止直接在 `main` 开发；每个分支必须同步更新交接文档。**

关键区分：**PR 是 GPT 临时远程审查机制，不是 Codex 本地审查的必要条件。**

原流程 A 保留为正式流程，等 Codex 额度恢复后再切回；切换前必须更新本文件和 `PROJECT_STATUS.md`。

## 事实源规则

PR 描述不是事实源，只是远程审核入口。

| 文件 | 定位 |
|---|---|
| `AGENT_HANDOFF.md` | 本轮执行事实源 |
| `PROJECT_STATUS.md` | 项目阶段事实源 |
| `TODO.md` | 任务完成状态事实源 |
| `CHANGELOG.md` | 变更记录事实源 |
| PR 描述 | 摘要和入口，不替代仓库内交接文档 |

冲突处理：

1. PR 描述和交接文档冲突，以交接文档为准。
2. PR 描述写完成，但 `TODO.md` 未同步，视为未完成。
3. PR 写测试通过，但 `AGENT_HANDOFF.md` 未记录命令和结果，视为未验收。
4. 代码改动未同步状态文档，GPT 流程下直接 No-Go；Codex 流程下要求 Code / OpenCode 补齐后再审。

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
3. Code / OpenCode 按任务书创建本地分支并执行。
4. Code / OpenCode 必须在本地分支内同步更新交接文档。
5. Codex 审本地分支、本地 diff、测试结果和交接文档。
6. Codex 给出 `Go / No-Go / 需返工`。
7. 用户确认后，再决定是否本地合并、推送远程、或开 PR 归档。

### 使用条件

仅在 Codex 额度充足、能稳定承担项目经理和最终审查时启用。

### PR 要求

Codex 正式流程下，PR 不强制。  
PR 只作为远程归档、多人协作或用户明确要求时使用。

## 流程 B：临时流程（当前启用）

### 启用原因

Codex 暂时额度不足，项目不能停滞，因此临时切换为 GPT 审核 GitHub 分支 / PR。

### 角色分工

| 角色 | 职责 |
|---|---|
| 用户 | 定目标、边界、取舍、最终确认 |
| GPT | 从 GitHub 读取分支 / PR / diff / 交接文档，做审查并给 Go / No-Go |
| Code / OpenCode | 总控开发、具体执行、建分支、更新交接文档、开 PR |
| Codex | 暂停作为项目经理和最终审查方 |

### 工作方式

1. Code / OpenCode 不得直接在 `main` 开发。
2. 每个任务必须创建独立分支。
3. 每个分支必须更新交接文档：`AGENT_HANDOFF.md`、`PROJECT_STATUS.md`、`TODO.md`、`CHANGELOG.md`。
4. 分支命名建议：
   - `feature/p0-usage-log`
   - `feature/p0-config-refresh`
   - `feature/p0-tool-fit`
   - `fix/web-502-assets`
   - `docs/*`
5. 完成后推送分支并开 PR，目标分支为 `main`。
6. PR 描述只写摘要和入口，不替代交接文档。
7. GPT 从 GitHub PR diff + 分支内交接文档审查。
8. GPT 给出 `Go / No-Go / 需返工`。
9. 用户最终确认是否合并。

### PR 描述模板

```md
## 本 PR 目标
一句话说明本轮解决什么。

## 事实源
- AGENT_HANDOFF.md
- PROJECT_STATUS.md
- TODO.md
- CHANGELOG.md

## 验证
以 AGENT_HANDOFF.md 中记录为准。

## 风险
以 AGENT_HANDOFF.md 中记录为准。
```

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
5. 不允许直接在 `main` 开发，除非用户明确授权紧急修复。
6. 从流程 B 切回流程 A 后，继续保留“独立分支 + 交接文档”规则，但不强制 GitHub PR。

## 之前是否走分支合并

按当前 GitHub 可见状态判断：

1. 仓库默认分支为 `main`。
2. 当前未查到已有 PR。
3. 分支搜索未返回额外分支。

因此，之前开发**不能确认走过 GitHub 分支 + PR 合并流程**；更稳妥的判断是：此前大概率以直接提交 / 本地整理后入仓为主。

后续从本文件加入开始，统一按“独立分支 + 交接文档 + 用户确认”执行；GPT 临时流程额外要求 GitHub PR。

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

## GPT 流程 PR 审核清单

每个 PR 至少检查：

1. 是否只改本轮范围。
2. 是否命中对应 P0 缺口。
3. 是否新增或修改敏感信息。
4. 是否碰到禁改区。
5. 是否保留 onWatch 原能力。
6. 是否有最小验证命令和结果。
7. 是否同步状态文档。
8. 是否能明确合并风险。
9. PR 描述是否与交接文档冲突。

## 当前执行口径

当前执行口径为：

```text
GPT 从 GitHub 分支 / PR / 交接文档审核；Code / OpenCode 总控开发；用户最终确认合并。
```
