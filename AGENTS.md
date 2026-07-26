# Repository Guidelines

TokenBar uses the current `cross-tool-dev-workflow` contract. The canonical
document map is in `DOCUMENTATION.md`; the non-negotiable safety baseline is in
`UNIVERSAL_EXECUTION_RULES.md`. Tool-local files under `.claude/`, `.opencode/`
and `.codex/` are launch inputs, not project truth.

## Required reading

Before writing, read in this order:

1. `AGENTS.md`
2. `README.md`
3. `CURRENT_TASK.md`
4. `AGENT_HANDOFF.md`

On first entry or platform switch also read `COLLABORATION.md` and
`PROJECT_STATUS.md`. Read `DECISIONS.md` for product, architecture, security,
cost or destructive decisions. Read domain documents, `CODE_REVIEW.md`,
`PLATFORM_ACTIVITY_LOG.md` and `CHANGELOG.md` only when the task needs them.

## Project state and boundaries

- The default desktop shell is `apps/desktop-tauri/`; shared backend and CLI
  logic lives in `rust/`.
- `platform/windows` is the active Windows product line and uses Tauri/React/Rust.
- Windows provider and browser behavior must be checked against the direct
  upstream `Finesssee/Win-CodexBar`; macOS references are historical only.
- `main` contains cross-line documentation and decisions only. Application
  code stays on its platform branch; see `docs/PROJECT_LINES.md`.
- One executor writes the worktree at a time. Read-only exploration and review
  may run in parallel, but concurrent writing agents are forbidden.

## Build and test

From `apps/desktop-tauri/`:

```powershell
pnpm build
pnpm test -- --run
pnpm tauri:build:debug
```

For backend-only work:

```powershell
cargo test --manifest-path rust/Cargo.toml
cargo test --manifest-path apps/desktop-tauri/src-tauri/Cargo.toml
```

Run a direct debug executable only after the build succeeds. Stop an older
`codexbar-desktop-tauri.exe` before rebuilding so the executable is not locked.
For UI or tray changes, record the actual runtime validation in the handoff and
activity log; do not claim visual success from unit tests alone.

## Coding and security

- Keep provider-specific logic inside provider modules and use the shared
  `codexbar::core::instantiate_provider` factory.
- Prefer small typed changes and deterministic tests; do not add dependencies
  without confirmation.
- Never log or commit tokens, cookies, API keys, passwords, private URLs or
  raw personal data. Use existing redaction and secure-storage helpers.
- Do not commit, push, merge, publish, deploy, migrate, delete or overwrite
  external data without explicit user authorization.

## Handoff completion

Every complete, partial or blocked task must update `AGENT_HANDOFF.md` and add
one factual row to `PLATFORM_ACTIVITY_LOG.md`. Keep `CURRENT_TASK.md` as the
single active task contract; keep historical material under `docs/archive/`.
