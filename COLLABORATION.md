# Collaboration Contract

This file is TokenBar's project-specific adaptation of the current
[`cross-tool-dev-workflow`](https://github.com/shawnqd/ai-skills-vault/tree/main/workflows/cross-tool-dev-workflow).
The shared safety baseline is `UNIVERSAL_EXECUTION_RULES.md`; the document map
and update timing are in `DOCUMENTATION.md`.

## User-controlled platform selection

The user chooses the executor for each task: Codex, Claude Code, OpenCode,
Grok or another tool. Do not auto-route, auto-switch or silently reassign a
task. If the executor is unclear, stop before writing and ask the user.

## TokenBar branch boundaries

| Branch | Product line | Code allowed |
| --- | --- | --- |
| `platform/windows` | Windows Tauri/React/Rust product; current active line | Windows app code and its tests |
| `platform/macos` | macOS Swift/SwiftUI product line | macOS app code and its tests |
| `main` | Cross-line documentation, decisions and branch explanation | No platform application code |

This is an intentional project adaptation: unlike a generic single-`main`
repository, application code stays on its platform branch. Shared product
rules and branch boundaries are documented on `main` when that branch is being
maintained. Full upstream boundaries are in `docs/PROJECT_LINES.md`.

## Roles and write ownership

1. **User** — owns scope, product tradeoffs, destructive actions and final
   commit/push/merge authorization.
2. **Project controller** — maintains the active task contract, handoff,
   evidence and review state; it may execute when selected by the user.
3. **Executor** — one selected tool/model at a time; implements only the task
   package, tests it and reports facts.
4. **Reviewer** — independently checks diff, tests, boundaries, runtime/data
   effects and unresolved risks, then returns findings and Go/No-Go.

Only one executor may write the shared worktree at a time. Read-only explorers
and independent reviewers may work in parallel. Two writing agents must never
edit the same worktree concurrently, even with disjoint file lists.

## Standard flow

```text
user goal
→ user selects executor
→ rewrite CURRENT_TASK.md
→ write one package in AGENT_HANDOFF.md
→ executor records start checkpoint and implements
→ executor self-tests and records completion checkpoint
→ append one factual PLATFORM_ACTIVITY_LOG.md entry
→ reviewer reports findings and Go/No-Go
→ user tests and explicitly authorizes commit/merge
```

The user must not act as a messenger between tools. Blockers, test results,
runtime changes and next actions belong in `AGENT_HANDOFF.md`; concise platform
history belongs in `PLATFORM_ACTIVITY_LOG.md`.

## Windows-specific boundary

For `platform/windows`, investigate Windows behavior against the direct
upstream `Finesssee/Win-CodexBar`. The macOS project is historical inspiration,
not a Windows implementation source. Do not cross-import window, tray, cookie,
authentication or update behavior without a Windows-specific decision.

## Authorization and history

Do not commit, push, merge, publish, deploy, migrate, delete or overwrite
external data without explicit user authorization. `AGENT_HANDOFF.md` and
`CURRENT_TASK.md` contain only current work; older material is preserved under
`docs/archive/` and is not a source of current scope.
