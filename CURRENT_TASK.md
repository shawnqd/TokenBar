# CURRENT_TASK

Status: complete — waiting for the user's next task
Task ID: TASK-DOC-SYNC-001
Executor: Codex
Role: project controller and documentation executor

## Goal

Migrate TokenBar to the current `cross-tool-dev-workflow` document contract so
future agents use one synchronized project system instead of the legacy
append-only handoff rules.

## Allowed scope

- Root workflow documents: `AGENTS.md`, `COLLABORATION.md`, `CURRENT_TASK.md`,
  `AGENT_HANDOFF.md`, `DOCUMENTATION.md`, `UNIVERSAL_EXECUTION_RULES.md`, and
  `PLATFORM_ACTIVITY_LOG.md`.
- Documentation archive: `docs/archive/`.
- `.gitignore` exceptions so the preserved archive is versionable.
- No application source, assets, settings, provider code or runtime data.

## Required sources

- GitHub CLI copy of
  `workflows/cross-tool-dev-workflow/SKILL.md`;
- `UNIVERSAL_EXECUTION_RULES.md`, `DOCUMENTATION.md` and the project branch
  boundary in `docs/PROJECT_LINES.md`;
- Existing documents preserved under `docs/archive/`.

## Forbidden operations

- Do not modify `apps/`, `rust/`, `design/` or provider/runtime data.
- Do not commit, push, merge, create a branch, or rewrite external data.
- Do not delete historical evidence; preserve it under `docs/archive/`.

## Acceptance checks

- The root document map names exactly the nine stable project documents plus
  `PLATFORM_ACTIVITY_LOG.md`.
- Required reading order and update timing match the current skill contract.
- `AGENT_HANDOFF.md` and `CURRENT_TASK.md` describe only current work.
- Historical handoff/task material is preserved under `docs/archive/`.
- `git diff --check` passes and no application files are changed by this task.

## Stop conditions

Stop and return to the user if the remote workflow contract conflicts with the
Windows/macOS branch decision in `docs/PROJECT_LINES.md`, or if preserving a
historical document would require deleting data.
