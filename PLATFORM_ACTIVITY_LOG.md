# PLATFORM_ACTIVITY_LOG

This is the only human-readable history of platform actions in TokenBar. Add
one concise factual entry after each task is complete, partial or blocked.

Format:

```text
时间 | 任务 ID | 平台 / 模型 / Agent | 操作摘要 | 修改范围或提交 | 状态变化 | 验证结果 | 结果 / 风险
```

## Entries

2026-07-26 | TASK-DOC-SYNC-001 | Codex / unknown / project controller | Migrated project documentation to the current cross-tool workflow and preserved the previous handoff/task files under `docs/archive/` | documentation only, uncommitted | legacy docs retained as archive; current contract introduced | GitHub CLI source read passed; canonical document check passed; `git diff --check` passed | complete; no application code or data changes

## Constraints

- Do not guess the model, session, HEAD, commit or test result; use `unknown`.
- Do not record tokens, cookies, passwords, personal data, full sensitive URLs,
  raw screenshots or chat content.
- Detailed task state belongs in `AGENT_HANDOFF.md`; formal changes in
  `CHANGELOG.md`; review conclusions in `CODE_REVIEW.md`.
