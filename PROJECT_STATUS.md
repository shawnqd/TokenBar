# PROJECT_STATUS

## Current phase

Windows Tauri/React/Rust product line, branch `platform/windows`. The tray
flyout has been rebuilt around a fixed native surface and the reference HTML's
three density modes. The documentation workflow migration is committed and
pushed; application changes remain in the worktree while waiting for the
user's final visual acceptance and separate application commit authorization.

## Completed

- Tray flyout routing and fixed geometry were rebuilt and verified with a debug
  build and direct Windows launch.
- Detailed, compact and minimal provider-card structures were aligned with
  `design/floatbar-reference.html`.
- Frontend checks passed: 33 test files / 159 tests; locale drift check passed;
  debug Tauri build passed.
- The project documentation system was migrated to the current
  `cross-tool-dev-workflow` contract. See `DOCUMENTATION.md` and
  `PLATFORM_ACTIVITY_LOG.md`.

## Known risks

- Settings still has a separate window/proof route that requires a later,
  explicitly scoped task; it is outside the flyout rewrite.
- The Windows Vite/dev-server cache issue is documented as a runtime risk;
  direct debug builds are the reliable validation path until that task is
  separately investigated.
- Existing uncommitted application changes predate the documentation migration.

## Next milestone

User visual review of the rebuilt tray flyout, especially compact/minimal modes;
then the user decides whether to authorize committing and publishing the
application changes. The documentation migration is already published; no
application commit is implied by this status file.

## Branch and upstream boundary

- `platform/windows` → `Finesssee/Win-CodexBar` direct Windows upstream.
- `platform/macos` → independent macOS product line.
- `main` → cross-line documentation and decisions only.

Historical status and handoff records are preserved under `docs/archive/` and
are not current task instructions.
