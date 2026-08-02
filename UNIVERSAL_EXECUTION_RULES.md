# Universal Execution Rules

These are the project-level safety and handoff rules for every terminal and
model. They do not choose the executor; the user chooses the platform for each
task.

## 1. Required reading

Before writing, read `AGENTS.md`, `README.md`, `CURRENT_TASK.md`, and
`AGENT_HANDOFF.md`. On first entry or platform switch, also read
`COLLABORATION.md` and `PROJECT_STATUS.md`. Read `DECISIONS.md` for product,
architecture, security, cost or destructive decisions.

## 2. Task contract

Before code changes, `CURRENT_TASK.md` must state the executor, role, goal,
allowed files, forbidden files, acceptance checks, verification commands and
stop conditions. Unclear product, security, privacy, cost or destructive
choices return to the user instead of being guessed.

## 3. Writing and delegation

- One executor writes the shared worktree at a time.
- Read-only exploration and independent review may run in parallel.
- Multiple writing agents must not edit the same worktree concurrently, even
  when their intended file lists do not overlap.
- The executor stays inside the task package and does not opportunistically
  refactor nearby code.

### Windows development runtime

- The single supported interactive entry is `.\scripts\dev-windows.ps1`
  from the repository root; it owns cleanup, Vite, Tauri and Rust watch
  startup.
- Normal launches clear inherited `CODEXBAR_PROOF_MODE`; use the explicit
  `-ProofMode <surface>` switch only for screenshot automation. Proof mode
  intentionally suppresses flyout blur-dismiss and tray-toggle hiding.
- The Windows taskbar usage strip uses the TrafficMonitor native-child path
  documented in `AGENT_HANDOFF.md`: create a native popup, attach it to
  `Shell_TrayWnd`, convert it to `WS_CHILD`, and reassert client geometry.
  Do not silently replace it with a floating overlay; record evidence before
  changing the path on another Windows build.
- Do not run `pnpm dev`, `tauri dev` or a stale `target\debug` executable as a
  separate development session. The first is frontend-only and the latter two
  can create a second runtime that hides whether the current worktree is being
  tested.
- Before handing work to another model, verify one checkout-scoped
  `codexbar-desktop-tauri.exe`, one Vite listener on port 1420 and one launcher
  chain. Record deviations in `AGENT_HANDOFF.md`.
- To hand off a clean runtime, run `.\scripts\dev-windows.ps1 -StopOnly`
  and verify that no checkout-scoped TokenBar process or port 1420 listener
  remains.

## 4. Checkpoints and activity

Every complete, partial or blocked task updates `AGENT_HANDOFF.md` and adds one
fact-only entry to `PLATFORM_ACTIVITY_LOG.md`. `CURRENT_TASK.md` is rewritten
when the active task changes; it is not a history file. `AGENT_HANDOFF.md`
keeps only the current task and its latest checkpoint. Historical checkpoints
belong under `docs/archive/` or in Git history.

## 5. Authorization and safety

Do not commit, push, merge, publish, deploy, migrate, delete, restore or
overwrite external data without explicit user authorization. Never record
tokens, cookies, passwords, private keys, raw personal data or sensitive
screenshots. Unknown attribution, HEAD, model or test facts must be recorded as
`unknown`, never guessed.

## 6. Review gate

Review findings must state severity, evidence, impact and recommendation before
the Go/No-Go conclusion. Passing tests do not prove external-service quality,
data correctness or visual acceptance; remaining risks must be reported.
