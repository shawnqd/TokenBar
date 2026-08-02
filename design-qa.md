# Tray Flyout Design QA

Date: 2026-07-26
Task: `TASK-FLYOUT-REFERENCE-CLEANUP-003`
Branch: `platform/windows`

## Visual authority

- Layout and content: `design/floatbar-reference.html`
- Surface treatment: option D in `design/style-options.html`
- Production size: 328 x 776 logical px
- Windows validation scale: 125%, captured as 410 x 970 physical px

## Evidence

- Real Tauri flyout: `design/qa/flyout-live-light-detailed.png`
- Reference adaptation: `design/qa/flyout-current-light-detailed-fixed-dpi.png`

## Result

The dedicated `flyout` window renders the fixed provider switcher, scrollable
model-card body, and fixed action footer without clipping. Detailed cards match
the reference hierarchy: provider header, quota zone, insight zone, progress
bars, reset credits, output speed, local usage, pace and runway.

Option D is scoped to the tray flyout: elevated theme-aware zones, 12 px radii,
hairline-first shadows, and a separate flyout backdrop. Settings and dashboard
surfaces do not inherit these overrides.

Detailed, compact and minimal structures are covered by `MenuCard` and
`TrayPanel` tests. Minimal remains a readable flat summary rather than a blank
panel. The fixed-size phase deliberately ignores legacy content `zoom`; native
resize/scale is a separate future task.

## Acceptance

- [x] Fixed 328 x 776 logical window.
- [x] Dedicated flyout route only; unknown/main surfaces fail closed.
- [x] Detailed reference hierarchy and proportional type/spacing.
- [x] Compact and minimal have distinct tested DOM structures.
- [x] Light/dark colors use theme tokens.
- [x] Header and footer stay fixed while provider cards scroll.
- [x] No open P0/P1/P2 mismatch in the captured detailed state.

Final result: passed for the fixed-size reference implementation. Automated
coverage validates the non-captured density/theme branches.
