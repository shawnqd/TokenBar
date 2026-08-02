# Proof mode flyout visibility investigation

## Finding

`CODEXBAR_PROOF_MODE=trayPanel` suppresses the native
`WindowEvent::Focused(false)` blur-dismiss path in
`apps/desktop-tauri/src-tauri/src/shell/flyout_window.rs`. The observed
`IsWindowVisible=false` was not sufficient evidence that this guard failed:
the first flyout build intentionally creates the window with
`.visible(false)`, and the frontend used to wait for provider/cache data
before calling `reveal_tray_panel_window`.

Therefore a slow first refresh could leave the panel hidden while automation
was already moving focus. That is a pre-reveal state, not a blur-dismiss.

The panel also had independent explicit hide routes:

- the tray-icon toggle can hide an already-open flyout;
- the frontend Escape shortcut invokes `dismiss_tray_panel`;
- opening the dashboard invokes `dismiss_tray_panel` after the transition;
- a native close request hides the flyout.

The proof guard on `Focused(false)` does not and should not block the explicit
`hide-surface` proof command. These paths must be distinguished when reading
automation logs.

## Fix in the current worktree

- `TrayPanel.tsx` now starts the fixed-size reveal as soon as the React shell
  mounts (`canMeasure: true`); provider data is content and no longer a
  visibility gate.
- The proof-mode tray toggle now refocuses an open panel instead of hiding it.
- Proof mode suppresses the flyout close request, while the explicit
  `hide-surface` command remains available for harness teardown.
- Debug traces identify suppressed blur, toggle, and close paths:
  `flyout: proof mode suppressed blur-dismiss`,
  `flyout: proof mode kept panel visible on tray toggle`, and
  `flyout: proof mode suppressed close request`.

## Claude verification handoff

The supported normal development entry is `scripts/dev-windows.ps1`. It clears
an inherited `CODEXBAR_PROOF_MODE`, so ordinary development has the normal
outside-click and tray-toggle close behavior. Use the launcher with an explicit
`-ProofMode trayPanel` only when taking automated screenshots.

Launch the supported Windows development chain with the environment variable
set before the process starts. Move focus to another application and poll the
`flyout` HWND visibility only after the React shell has mounted. The expected
sequence is:

1. the first build becomes visible without waiting for provider refresh;
2. an outside focus event produces the suppressed-blur trace and does not
   call `window.hide()`;
3. a deliberate `hide-surface` proof command hides the panel; Escape or the
   dashboard action are separate explicit UI dismissals and should not be
   used as a focus-stealing step in the harness.

If visibility still changes to false after the reveal trace, capture the
adjacent log lines and the automation action. That would identify a new
explicit hide caller rather than the blur-dismiss branch.

## Verification completed

- `apps/desktop-tauri/src/surfaces/TrayPanel.test.tsx`: 27 tests passed,
  including reveal before the first provider refresh completes.
- `cargo test --manifest-path apps/desktop-tauri/src-tauri/Cargo.toml
  shell::flyout_window`: 3 tests passed.
