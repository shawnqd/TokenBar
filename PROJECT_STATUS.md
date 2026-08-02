# PROJECT_STATUS

## Active handoff

`TASK-QUOTA-PRESENTATION-TASKBAR-018` is being implemented by Claude Code as
sole writer, from the task package in `docs/CLAUDE_TASK_PACKAGE_018.md`. The user
split it into three phases and accepts each one before the next starts.

**Phase 1 is implemented and fully tested (uncommitted), awaiting user
acceptance.** It delivered the inventory/migration table
(`docs/QUOTA_PRESENTATION_INVENTORY_018.md`), the shared quota presentation layer
(`apps/desktop-tauri/src/lib/quotaDisplay.ts`), items B/C/D data semantics, and
three independent per-component settings pairs replacing the single global
`show_as_used` / `reset_time_relative`. See `CURRENT_TASK.md` for the detail and
the one deferred product decision.

Phase 2 (item H settings split, item A weekly/forecast merge, item E Settings
shadow) and phase 3 (item F DirectWrite variable weight, item G ordered
multi-provider taskbar entries) have not been started. Codex reviews and performs
final verification. Nothing is committed.

## Current phase

Windows Tauri/React/Rust product line, branch `platform/windows`. The dedicated
tray flyout is rebuilt around a fixed 328 x 776 logical-pixel native surface and
the reference HTML's three density modes. The selected option-D surface styling,
canonical tray naming, dead-code cleanup and Windows hot-reload entry are in the
worktree. Application changes remain uncommitted pending user review.

## Completed

- Tray flyout routing and fixed geometry were rebuilt and verified with a debug
  build and direct Windows launch.
- Detailed, compact and minimal provider-card structures were aligned with
  `design/floatbar-reference.html`.
- Frontend build and locale parity (670 keys) passed. The full frontend suite
  passed (33 files / 162 tests), as did all 306 Rust workspace tests.
- Production tray CSS now has one authority section; legacy main-window
  TrayPanel fallback and verified dead reveal/hide state were removed.
- The project documentation system was migrated to the current
  `cross-tool-dev-workflow` contract. See `DOCUMENTATION.md` and
  `PLATFORM_ACTIVITY_LOG.md`.
- Tray menu naming was reduced to three canonical keys: `TrayOpenPanel`,
  `TrayOpenDashboard`, and `TrayShowMiniStatusBar`; all locale and active
  menu references use the same names.
- Tauri development now watches the shared `rust/` crate through
  `additionalWatchFolders`, so Rust and locale changes trigger the normal
  dev rebuild/restart automatically.
- Windows development has one safe entry point at
  `scripts/dev-windows.ps1` (with a hidden VBS wrapper) that cleans only
  checkout-scoped processes and writes logs under `%TEMP%\tokenbar-dev\`.
  Normal launches clear inherited `CODEXBAR_PROOF_MODE`; proof runs must pass
  `-ProofMode` explicitly so blur-dismiss and tray-toggle close behavior are
  present in the user-facing development build.
- The Windows taskbar usage strip now follows TrafficMonitor's native-child
  path: a popup is attached to `Shell_TrayWnd`, converted to `WS_CHILD`, and
  kept in taskbar-client coordinates. Its native context menu routes back to
  the Tauri app.
- Display settings now expose a live placement choice: taskbar left side or
  immediately before the notification area. The strip is rendered as a compact
  transparent two-line readout aligned to the taskbar row.

## Known risks

- Settings still has a separate window/proof route that requires a later,
  explicitly scoped task; it is outside the flyout rewrite.
- The supported development chain is running from this checkout in normal
  mode for user inspection. The previous proof-mode process was stopped after
  its suppression logs explained the reported close failure.
- The remaining visual acceptance gap is real-window confirmation of compact,
  minimal and dark variants; this does not block normal interaction behavior.
- Taskbar overlay placement and visual styling still need user acceptance on
  different taskbar positions or multi-monitor layouts.
- Existing uncommitted application changes predate the documentation migration.

## Next milestone

The user reviews the fixed tray flyout before authorizing any application commit
or publication. No commit is implied by this status file.

## Branch and upstream boundary

- `platform/windows` → `Finesssee/Win-CodexBar` direct Windows upstream.
- `platform/macos` → independent macOS product line.
- `main` → cross-line documentation and decisions only.

Historical status and handoff records are preserved under `docs/archive/` and
are not current task instructions.
