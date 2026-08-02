# CURRENT_TASK

## Phases 2 and 3 complete (2026-07-31)

Task ID: `TASK-QUOTA-PRESENTATION-TASKBAR-018`
Branch: `platform/windows`, HEAD `b8529fb5`, **uncommitted**
Executor: Claude Code / claude-opus-5 (sole writer)

All items A, E, F, G, H are implemented, plus two out-of-band fixes the user
asked for (Grok forecast, macOS pace parity). Item 3 of the follow-up list
(historical/probabilistic layer) is **specified but not built** — see below.

### Item E — corrected 2026-08-01 after user review

The first attempt tuned shadow blur and gutter width. The user pointed out the
flyout had already solved this, and it had: `.menu-surface--tray` uses
`box-shadow: none; filter: none; border: 0` with `border-radius` **plus**
`clip-path: inset(0 round …)`, and no transparent gutter — the window *is* the
card.

Settings now uses the identical composition. This removes the problem class
rather than tuning it: a shadow drawn inside a fixed-size window can only ever
be clipped by the window edge, and the gutter exists only to hold that shadow.
With neither, there is nothing to clip and no hard outer ring.

`clip-path` is doing real work, not duplicating `border-radius`: the radius alone
leaves square corners on descendants that paint their own background, such as
the title bar.

Verified by comparing every frame property against `.menu-surface--tray` in a
browser against the shipping stylesheet: box-shadow, filter, border width,
radius and clip-path all match, and the card fills the window exactly. The
window size reverted to 912x580 since no gutter is reserved.

Two measurement traps worth remembering, both of which produced confidently
wrong readings this round:

- A `?v=` cache-buster on the preview HTML does **not** bust a linked
  stylesheet. Two consecutive readings reported the old shadow after the file
  had already changed.
- Forcing `ShowWindow` on the prewarmed Settings window does not bring it
  forward; a screen capture at its coordinates grabbed unrelated desktop
  content instead. Do not treat such a capture as evidence.

### Item G — ordered taskbar entries

`taskbar_widget_content` (three fixed values) is replaced by
`taskbar_widget_entries`, an ordered `provider + window` list.

- Legacy migration is exact: `usage → [session, weekly]`, `speed → [speed]`,
  `usage_speed → [session, speed]`, so upgrading changes nothing on screen. The
  old field survives as a migration source only.
- `provider_id: "auto"` follows the tray's own pick, which is how the default
  reproduces the previous behaviour without naming a provider.
- `normalize_taskbar_entries` drops unknown windows, blank providers and
  duplicates, and caps at `TASKBAR_MAX_ENTRIES`; an empty result falls back to
  the default rather than leaving an unrenderable strip.
- New `taskbar_entries.rs` resolves entries against live snapshots. **Windows are
  matched by declared length, never by slot** — reading `primary` positionally is
  the old bug that made Claude's 5-hour usage render as its weekly figure.
- **Never a silent zero.** Every unresolvable entry carries a reason
  (`ProviderDisabled` / `NoData` / `ProviderError` / `WindowUnsupported`) and the
  strip prints that reason instead of a number. A balance is money, not a
  percentage, so it resolves to `WindowUnsupported` rather than being coerced.
- The native strip renders N ordered lines with ellipsis trimming
  (`IDWriteTextFormat::SetTrimming`, GDI `DT_END_ELLIPSIS` on the fallback path),
  showing the first `MAX_VISIBLE_LINES`; order is what decides what survives.
- The Taskbar page gained an entry composer: add, remove, reorder, restore
  defaults, and entries past the visible lines are dimmed and labelled rather
  than hidden, so a configured-but-invisible entry is never a mystery.
- Fixed in passing: `tray_bridge.rs` hardcoded the Chinese strings `周额度` and
  `速度`. All strip text is now localized in all six languages.

### Item H — settings split by component

Three component pages, each writing only its own keys: **FloatBar**,
**Dashboard**, **Taskbar**. `Display` keeps app-level appearance (theme) only.

- `floatBarShowAsUsed`, `floatBarResetTimeRelative` and `taskbarShowAsUsed` had
  **no UI at all** before this — they could only be set by migration. Each now
  has a control on its own page.
- Each page has its own restore-defaults over its own key set.
- `ComponentIsolation.test.tsx` is the guarantee: for all three pages it clicks
  every toggle and the reset button and asserts no foreign component's key is
  ever written. That is the one property item H exists to enforce.

### Verification (all real, 2026-07-31)

- Frontend: `tsc --noEmit` clean; 37 files / **213 tests**; `pnpm build` passed;
  locale drift **742 keys**.
- Tauri: **325 tests**. Shared Rust: **617 tests** plus launcher.
- `git diff --check` clean. No commit, no push.
- `App.test.tsx > fails closed for an unknown main-window surface mode` fails
  intermittently under full parallel load and passes standalone and on a clean
  rerun; this is the pre-existing flake already recorded for task 016.

### Not done, and why

- **The historical/probabilistic layer** (run-out probability, 5-hour
  session-equivalents, predictive notifications). Fully specified in
  `docs/MACOS_WEEKLY_QUOTA_MODEL.md`; not built. It needs a usage-sample store
  and a median burn estimator first — everything else in that feature depends on
  them.
- **Workday-aware pacing** was explicitly dropped by the user ("我不光工作日用").
  macOS computes the expected line from workday seconds when configured; we
  deliberately stay on wall-clock.
- **No visual verification of items G and H this round.** The entry composer, the
  three new settings pages and the multi-line strip have automated coverage only.

## Phase 2/3 in progress (2026-07-31, items A and F done; E, G, H not started)

Task ID: `TASK-QUOTA-PRESENTATION-TASKBAR-018` — phases 2 and 3, requested as one pass
Branch: `platform/windows`, HEAD `b8529fb5`, **uncommitted**
Executor: Claude Code / claude-opus-5 (sole writer)

The user asked for phases 2 and 3 in one round, and chose the real DirectWrite
route for item F over hand-rolled COM, authorizing a new dependency.

### Done and verified

**Item A — corrected 2026-07-31 after user review.** The first attempt only
moved the forecast *text* into the quota block and left it as a tinted panel,
and the "expected position" marker it added had **no CSS at all**, so it never
rendered. The user rejected that ("不是让你把文字搬上去啊，还有进度状态") and
pointed at the macOS app as the reference.

Ported from macOS `Sources/CodexBar/UsageProgressBar.swift` (upstream
`steipete/CodexBar`): the bar itself carries the pace state. The pace position is
**cut out of the track and fill** — Core Graphics `destinationOut` there, a CSS
mask here — and one coloured stripe is drawn in the gap, flanked by two
transparent gutters. Those gutters are what make the stripe legible against both
the filled and the empty side; a line simply overlaid on the bar does not read.
Green means reserve, red means deficit, matching macOS's mapping where being
ahead of the expected burn is a deficit.

The copy follows macOS `UsagePaceText.detailLeftLabel`: `On pace` /
`N% in reserve` / `N% in deficit`, as one compact line with the runway on the
right — no longer a tinted panel, and the "week elapsed" yardstick is gone
because the stripe on the bar now *is* that number. `paceStateOf` is shared by
the stripe and the label so the colour and the words cannot disagree.

**Item A — weekly quota merged with its forecast (original notes).** The card used to draw the
weekly quota bar and then a second, visually identical "pace" section with its
own actual/expected track: two blocks describing the same window. The forecast
now annotates the weekly quota block's own bar — the expected position is a
marker on that track (`ProviderQuotaBlock markerPercent`), and the copy (week
elapsed, over/under by, runway) sits inside the block. Both duplicate pace
sections (legacy card and density tiers) are gone.

Two correctness points worth keeping:

- The marker is mirrored into the displayed semantics by
  `forecastMarkerPercent`, so a bar showing *remaining* puts it at
  `100 - expected`. Otherwise the marker would sit on the opposite side of the
  bar from the number it annotates — item B's mismatch, expressed geometrically.
- `quotaForecastDisplay` now falls back to deriving the projection from the
  window's own `windowMinutes` + `resetsAt` via `paceBudget` when the provider
  supplies no pace block. The first version required `provider.pace` and would
  have shown "not enough data" for windows that plainly have enough. That is
  derived real data, not a fabricated number, and it is covered by a test.
- The forecast attaches only to the weekly row (`weeklyMetricId`), because the
  bridge computes `provider.pace` against the weekly window by length. Attaching
  it to `primary` is the old bug that made Claude's 5-hour usage read as weekly.

**Item F — DirectWrite variable weight is real, and measured.** `windows`
0.62.2 added (Direct2D + DirectWrite + GDI features only). New
`apps/desktop-tauri/src-tauri/src/taskbar_text.rs` renders through
`ID2D1DCRenderTarget::BindDC` + `IDWriteTextFormat3::SetFontAxisValues`.

Measured on this machine, Bahnschrift, ink (summed subpixel coverage) by weight:

| 200 | 400 | 430 | 560 | 700 | 900 |
|---|---|---|---|---|---|
| 901638 | 1252545 | 1263555 | 1517802 | 1685811 | 1973607 |

430 and 560 differ from the named stops on either side. That is the proof the
axis is continuous, not four static faces — the exact thing GDI could not do
(it collapsed 100..=550 to 400 and everything above to 700). System font
enumeration reports **398 families, 16 with a `wght` axis**.

Three real defects were found and fixed getting there, all documented in the
module:

1. `D2D1_RENDER_TARGET_TYPE_DEFAULT` lets D2D create a D3D device and blocks
   indefinitely in a process with no COM apartment. A DC render target
   composites to GDI anyway, so `..._SOFTWARE` is both correct and fast.
2. Font enumeration was O(n²) across ~400 families; now computed once into
   `FAMILY_CACHE`.
3. libtest gives each test its own thread and `RENDERER` is thread-local, so a
   second test that draws tries to create a second single-threaded D2D factory
   on an apartment-less thread and blocks. All drawing assertions therefore live
   in one test. The widget only ever draws from the app UI thread, so one
   renderer per process is the real usage pattern.

### Item F is now wired end to end

`taskbar_widget.rs` paints through `taskbar_text` (DirectWrite), with the old
GDI path retained only as a fallback if DirectWrite is unavailable — the strip
must never render an empty rectangle over the taskbar.

- `Settings.taskbar_widget_font_weight` is an OpenType `wght` axis value clamped
  to 100..=1000. The old three-stop snapping is **gone**; it existed only because
  GDI could not do better, and re-adding it would discard a real user choice.
  Legacy named values (`normal`/`medium`/`semibold`/`bold`) still load.
- New `Settings.taskbar_widget_font_family`, defaulting to `Microsoft YaHei UI`
  (present everywhere, and the strip routinely renders Chinese).
- New `get_taskbar_font_families` command enumerates the live
  `IDWriteFontCollection` and reports, per family, whether it has a real weight
  axis. The settings UI is populated from that — never a hardcoded list, so it
  cannot offer a family this machine lacks.
- The Taskbar page has a font picker and a continuous 100..=1000 slider whose
  helper text changes per family: a variable family says every value renders a
  distinct stroke, a static family says nearby values snap to the nearest
  installed face. That is the honest version of what item F asked for.

### Still not done

- Item G (ordered multi-provider taskbar entries: data model, native renderer,
  composition UI) — not started.
- Item H (splitting settings into three independent component pages) — not
  started. `float_bar_show_as_used` and `taskbar_show_as_used` still have no UI
  control; they are settable only by migration.
- Item E (Settings window frame re-verification) — not started.

### Visual evidence (real, DPI-aware)

A DPI-aware capture of `Shell_TrayWnd` (2560x60 at y=1380 on this 2560x1440 /
125% display — the real physical rect, confirming the capture is not
DPI-virtualized) shows the strip live in the taskbar rendering
`Codex 16%` / `周额度 0%`.

At 4x zoom the DirectWrite output is confirmed correct on the three things that
could have regressed when swapping renderers:

- Grayscale antialiasing with no colour fringes. TrafficMonitor's glyphs a few
  pixels to the left in the same capture show ClearType colour fringing; ours do
  not, which is what keeps the colour-key transparency clean.
- No opaque rectangle behind the text — the taskbar gradient shows through.
- Latin and CJK both render from the same family.

**Still outstanding for item F acceptance:** the package requires a real-taskbar
A/B across at least three weights and two font families. That was not done. The
settings file is DPAPI-protected, so the values cannot be scripted from outside
the app without risking the credentials stored alongside them, and driving the
Settings WebView by synthetic input is not reliable enough to be evidence. It is
now a ~10-second manual check: Settings → Taskbar → move the weight slider and
change the font; the strip repaints live with no restart.

### Verification (all real results, 2026-07-31)

- Frontend: `tsc --noEmit` clean; 35 files / **205 tests** passed;
  `pnpm build` passed; locale drift **715 keys** matched.
- Tauri: **317 tests** passed (was 314; +3 DirectWrite).
- Shared Rust: **614 tests** plus the launcher test passed.
- `git diff --check` exit 0.
- No commit, no push.

### Environment defect found (reported, not fixed — outside this task's files)

`scripts/dev-windows.ps1` takes its exclusive lock **before** calling
`Stop-TokenBarProcesses`, so `-StopOnly` can never stop a chain that is actually
running — it deadlocks against its own purpose. A launcher from the previous day
(PID 43540) was holding the lock and had to be killed by hand. This is the
"Another TokenBar Windows development launcher is already running" failure that
has recurred across several sessions. One-line fix: move the lock acquisition
after the `-StopOnly`/`-DryRun` early exits. Not applied — needs user sign-off,
since the script is outside this task package's scope.

## Phase 1 complete (2026-07-30, shared quota display layer and per-component settings)

Task ID: `TASK-QUOTA-PRESENTATION-TASKBAR-018` — phase 1 of 3
Branch: `platform/windows`, HEAD `b8529fb5`
Executor: Claude Code / claude-opus-5 (sole writer)
Status: implementation and automation complete; independent re-review is
**Conditional Go** (P1 fixes closed, P2 dashboard-surface decision open);
**uncommitted**; awaiting user acceptance before phase 2 starts.

The user split this task package into three phases and confirmed the split, plus
the decision that item F takes the real DirectWrite variable-weight route (in
phase 3, not this one):

- **Phase 1 (this round)** — inventory + migration table, shared
  `QuotaDisplayModel` formatting layer, items B/C/D data semantics, and the
  three-component independent settings keys with legacy-field migration.
- **Phase 2** — item H settings-page split, item A weekly quota/forecast merge,
  item E Settings-window shadow re-verification.
- **Phase 3** — item F DirectWrite variable font weight plus font-family
  choices, item G ordered multi-provider taskbar entries.

### What landed

The inventory and the full migration table are in
`docs/QUOTA_PRESENTATION_INVENTORY_018.md`; read that before phase 2.

1. **Three independent components.** `float_bar_*`, `dashboard_*` and
   `taskbar_*` copies of `show_as_used` / `reset_time_relative` now exist in
   shared `Settings`, `RawSettings`, the Tauri `SettingsSnapshot` /
   `SettingsUpdate`, and `types/bridge.ts`. `RawSettings` holds them as
   `Option<bool>` with a field-level `#[serde(default)]`, so a key absent from an
   older `settings.json` seeds once from the legacy global while an explicitly
   stored `false` is preserved. After that seed the three are fully independent —
   there is no runtime inheritance, which is what item H forbids.
2. **The legacy globals are retired, not deleted.** `Settings.show_as_used` and
   `Settings.reset_time_relative` remain deserializable purely as migration
   sources. No display surface reads them any more. Do not re-point a surface at
   them.
3. **One shared formatting layer.** New `apps/desktop-tauri/src/lib/quotaDisplay.ts`
   owns component→settings mapping (`quotaDisplayContext`), percent/level
   resolution (`quotaPercentDisplay`, `quotaLevel`), the five-state data status
   (`resolveProviderStatus`), reset-time resolution (`formatResetDisplay`) and
   forecast normalization (`quotaForecastDisplay`). 29 unit tests.
4. **Item B contradiction fixed.** The old `ProviderQuotaBlock.levelOf()` graded
   colour from `remainingPercent` against hardcoded 25/5 cutoffs while the number
   switched with `showAsUsed`, and it ignored the user's configured
   `highUsageThreshold` / `criticalUsageThreshold`. Risk is now graded once from
   used-percent against those configured thresholds, and the number and the bar
   fill both follow the chosen semantics, so colour and bar direction always
   agree.
5. **Item C reset semantics.** An elapsed window now reads
   `QuotaResetExpiredWaiting` ("已到期，等待刷新") instead of the old
   "resetting now"; a window with neither `resetsAt` nor a provider description
   reads `QuotaResetUnknown` instead of blank; absolute mode includes the date
   only when the reset crosses a local day boundary; the countdown still ticks
   from local time without refetching. Two new locale keys in all six languages
   (709 total, parity passing).
6. **Item D audited, no duplication found.** Reset information appears exactly
   once per window, in the block's title row (`.provider-quota__reset`, now
   carrying `data-reset-state`). `MenuCard`'s other reset string belongs to the
   separate cost window, and the FloatBar renders one per pill. No change was
   needed; recorded so phase 2 does not re-audit it.
7. **Surfaces re-pointed.** FloatBar → `floatBar`; TrayPanel and PopOutPanel →
   `dashboard`; the tray icon and taskbar strip (`selected_tray_percents`) →
   `taskbar`. `ProviderQuotaBlock`, `MenuCard` and `ProviderGrid` now take one
   `display: QuotaDisplayContext` object instead of loose booleans, so a caller
   cannot supply the semantics and forget the thresholds that colour it.
   Cross-contamination regression tests were added on all three sides.

### Component boundary decision — confirmed by user (2026-07-30)

The package names three components but the code has four display surfaces. The
tray flyout and the PopOut dashboard intentionally share one `dashboard`
component because they render the same cards from the same snapshot. The user
confirmed they should share settings; do not add a fourth id or settings-key pair.

### Verification (all real results)

- `pnpm build` (tsc + vite) passed; locale drift 709 keys matched.
- Frontend: 35 files / 197 tests passed.
- Shared Rust: 612 tests plus the launcher test passed.
- Tauri: 314 / 314 passed.
- `cargo check` passed for both workspaces; `git diff --check` passed.
- **No visual verification was performed this round** and none is claimed.
  Phase 1 changes no layout; the only user-visible change is that the Display
  tab's two toggles now write the dashboard keys. Live acceptance is a user
  check.

### Explicitly not done in phase 1

Items A, E, F, G, H (phases 2 and 3); the hardcoded Chinese strings in
`lib/providerBalance.ts` (`"余额"`, `"API 状态"`, `"暂不可用"`, `"含赠送 "`),
which violate the package's terminology requirement and are queued for phase 2;
`RateWindow::format_countdown()` in shared Rust, which is a second unlocalized
countdown implementation left in place because Rust callers still use it. No
commit, push or merge.

### Independent review gate (2026-07-30) — both P1 findings fixed

The review raised two P1s (full text and the executor response in
`CODE_REVIEW.md`). Both are fixed in code, not deferred.

**P1-1 — `taskbarResetTimeRelative` had no consumer. Resolved by deleting the
setting.** The task package's Taskbar page defines enablement, font, width,
alignment and entry composition — **no reset-time mode** — and the native strip
renders no reset text. The field was symmetry-driven over-engineering from the
first pass. Inventing a consumer would have meant changing the taskbar content
contract, which is phase 3 (item G) and needs screenshot acceptance.

Removed from `Settings`, `RawSettings`, `SettingsSnapshot`, `SettingsUpdate`,
`types/bridge.ts` and every fixture. Guarded against re-introduction two ways:

- The TS context is split into `QuotaPercentContext` (`showAsUsed` + thresholds)
  and `QuotaDisplayContext` (adds `resetTimeRelative`), with
  `ResetAwareComponent = "floatBar" | "dashboard"`. Asking the taskbar for a
  reset mode is now a **compile error**, not a dead setting.
- Rust test `taskbar_has_no_reset_time_mode_setting` asserts the key is never
  persisted.

Same-root fix included: `TaskbarTab`'s preview hardcoded `Codex 59%` /
`周额度 18%` regardless of `taskbarShowAsUsed`. It now runs its sample numbers
through `quotaPercentContext(settings, "taskbar")` + `quotaPercentDisplay`, so
it flips with the setting exactly as the native strip does. That gives
`taskbarShowAsUsed` a visible frontend consumer alongside its Rust one in
`selected_tray_percents`.

**P1-2 — `ProviderGrid` drew a percentage track for non-quota rows. Resolved.**
New shared predicate `quotaDisplay.primaryQuotaState(provider)` →
`"quota" | "error" | "informational"`. It covers both the `isInformational` rows
the review named (sub2api's "Subscription active" / "No quota data") **and** the
case the review did not name: balance providers (DeepSeek, MiMo) carry a prepaid
amount in a synthetic 0% window that is *not* informational-flagged, so an
`isInformational`-only check would still have leaked a 0% bar. The predicate
reuses `getProviderBalance().excludeWindows`, the same source the cards use.

`ProviderGrid` now draws the track and number only for `"quota"`. Non-quota rows
render a muted non-quota marker with a `QuotaNoPercentageForProvider` tooltip
(all six languages). Regression tests on both sides: four predicate cases in
`quotaDisplay.test.ts` (including "a balance provider *with* a plan is still a
real quota"), plus a `TrayPanel.test.tsx` case asserting a mixed list renders one
track and the non-quota row contains no `0%`.

**P2 (dashboard = tray flyout + PopOut) is left open for the user**, not decided
unilaterally — same decision recorded above.

Post-fix verification, all real: `pnpm build` passed; locale parity 710 keys;
frontend 35 files / 204 tests; shared Rust 613 tests plus launcher; Tauri
314/314; both `cargo check`s; `git diff --check` exit 0. Still no visual
verification and none claimed.

## Active handoff contract (2026-07-30, quota presentation and taskbar configuration)

Task ID: `TASK-QUOTA-PRESENTATION-TASKBAR-018`
Branch: `platform/windows`
Status: task package prepared for Claude Code; implementation not started in this checkpoint.

Execution decision confirmed by the user: three gated phases with Codex review
and user acceptance after each phase. Phase 2 covers settings-page grouping,
weekly quota/forecast merge and Settings-frame verification. Phase 3 must
complete the DirectWrite variable-weight renderer and taskbar composition; the
existing GDI three-stop fallback is not sufficient to close that phase.

The complete scope, migration rules, ownership boundary, acceptance cases and
required commands are in `docs/CLAUDE_TASK_PACKAGE_018.md`. The central goals
are to merge weekly quota with its forecast, give the FloatBar, Dashboard and
Taskbar independent display settings over one shared data/formatting layer,
fix the Settings frame, and make the native taskbar strip support real font
choices plus ordered provider/window combinations.

Existing `show_as_used` and `reset_time_relative` fields are foundations, not
evidence that this task is complete. The taskbar currently has only the old
single-content contract and the native GDI renderer has only three proven
effective weights; Claude must not claim continuous weight or multi-entry
selection without real Windows evidence. No commit or push is authorized.

## Current implementation override (2026-07-29, effective taskbar font weights)

Task ID: `TASK-TASKBAR-FONT-WEIGHT-EFFECTIVE-017`
Branch: `platform/windows`
Status: implemented, fully tested, live in the normal development runtime,
uncommitted.

The earlier 100–900 taskbar font-weight slider was misleading. A direct GDI
probe on this Windows machine showed that both Microsoft YaHei UI and Segoe UI
Variable collapse requests from 100 through 550 to Regular 400 and requests
from 600 upward to Bold 700 when used through `CreateFontW`. GDI does not expose
the continuous variable-font axis used by DirectWrite.

The control is now a three-position slider containing only weights that
visibly exist in the current native renderer: Light 300, Regular 400 and Bold
700. Light explicitly selects the installed `Microsoft YaHei UI Light` family;
Regular and Bold use `Microsoft YaHei UI`. Frontend, Tauri updates and persisted
settings all normalize old or arbitrary numeric values to one of those three
effective values, so no slider position is intentionally inert. The detailed
100–900 checkpoint immediately below is superseded by this correction.

Verification: production build and 707-key locale parity passed; frontend 34
files / 164 tests passed; shared Rust 608 tests plus launcher test passed;
Tauri 311 / 311 tests passed; `git diff --check` passed. Normal development
runtime PID was 49180 at handoff. No commit or push.

## Current implementation override (2026-07-29, taskbar font-weight slider)

Task ID: `TASK-TASKBAR-FONT-WEIGHT-SLIDER-016`
Branch: `platform/windows`
Status: implemented, fully tested, live in the normal development runtime,
uncommitted.

The native taskbar status strip font weight is now a numeric slider instead of
four named choices. It covers 100 through 900 in 25-point steps, shows the
current value beside the control, previews every movement immediately, and
persists the value when interaction ends. The same numeric value travels
through TypeScript, the Tauri bridge and shared Rust settings into the GDI
`CreateFontW` weight argument, so the taskbar widget repaints without an app
restart.

Existing saved values remain compatible: legacy `normal`, `medium`,
`semibold` and `bold` settings migrate to 400, 500, 600 and 700. Out-of-range
numeric values are clamped to 100–900. The native renderer still uses
Microsoft YaHei UI through Windows GDI; Windows may resolve nearby numeric
weights to the same available face, so the slider is detailed but does not
promise 33 visibly unique glyph weights.

Verification: production build and 707-key locale parity passed; frontend 34
files / 164 tests passed; shared Rust 607 tests plus launcher test passed;
Tauri 311 / 311 tests passed; `git diff --check` passed. Normal development
runtime PID was 47088 at handoff. No commit or push.

## Current implementation override (2026-07-29, Settings window frame)

Task ID: `TASK-SETTINGS-WINDOW-FRAME-015`
Branch: `platform/windows`
Status: implemented, fully tested, hot-rebuilt in normal development runtime,
uncommitted.

The detached Settings window now uses the same frontend-owned transparent
window composition as the tray flyout. A shared `--window-radius: 24px` token
drives both silhouettes. Settings has a real theme-aware hairline border and
drop shadow inside a transparent 12px window gutter; its title bar and content
are clipped to the shared radius. The native Settings WebView is transparent,
and its square non-client painting is disabled while `WS_THICKFRAME` is kept
for edge resizing.

The proof-harness Settings test was also corrected from the removed `apiKeys`
route to the current `menuBar` route, eliminating a stale test name left by
the earlier settings information-architecture migration.

Verification: TypeScript check, production build and locale parity (708 keys)
passed; frontend 34 files / 164 tests passed; Tauri 311 / 311 tests passed;
`git diff --check` passed. Normal development runtime PID was 51804 at
handoff. No commit or push.

## Current implementation override (2026-07-29, settings information architecture)

Task ID: `TASK-SETTINGS-FUNCTIONAL-LAYOUT-014`
Branch: `platform/windows`
Status: implemented, fully tested, normal development runtime running, uncommitted.

Settings are now grouped by user-facing function instead of accumulating
unrelated controls in Display. The top-level order is General, Providers,
Notifications, Display, Taskbar, Advanced and About. Display owns application
appearance, tray-panel presentation and FloatBar behavior. The new dedicated
Taskbar page owns both the native taskbar status strip and notification-area
icon behavior.

The taskbar status strip page now provides a live preview plus controls for
enablement, position, displayed content, width, font size, font weight and text
alignment. Those fields are persisted, localized and applied to the native
widget live without restarting. Arbitrary foreground/background color controls
were intentionally not added because the native widget samples the Windows
taskbar surface; overriding those colors would reintroduce the opaque block
regression.

Verification: TypeScript check, locale drift (708 keys), full frontend suite
(34 files / 164 tests), production frontend build, full Tauri suite (311
tests), full shared Rust suite (605 tests plus launcher test), and both Rust
workspace checks passed. `git diff --check` passed. Automated visual inspection
was interrupted by the user's Escape key, so final visual acceptance remains a
manual check in the running app (PID 18120 at handoff); no visual result is
claimed.

## Current implementation override (2026-07-29, window readiness)

Task ID: `TASK-WINDOW-PREWARM-013`
Branch: `platform/windows`
Status: implemented, tested, cold-started, uncommitted.

The detached Settings window and tray flyout are now created hidden during
application startup and use explicit frontend-ready handshakes. Settings is
not shown until its bootstrap data, lazy Settings bundle and first layout are
ready, removing the first-open blank flash. The flyout is already loaded by
the time the tray icon is first clicked, so that click no longer pays the
WebView2 creation cost or appears to be ignored. A click that arrives unusually
early is retained as a pending reveal and completes when the frontend reports
ready; Settings also retains the latest requested tab.

Verification: `pnpm build`, full frontend tests, `cargo check`, and all 311
Tauri tests passed. A clean development restart produced a new runtime process
and confirmed the prewarmed `CodexBar Settings` and `CodexBar` windows both
exist but remain hidden before user interaction.

## Current implementation override (2026-07-29)

The previous overlay notes below are historical. The active implementation is
the TrafficMonitor-style native child: create a popup, attach it to
`Shell_TrayWnd`, convert it to `WS_CHILD`, and reassert taskbar-client
geometry. It is verified in the running Windows development chain.

Status: taskbar-strip rendering uses a native child attached to
`Shell_TrayWnd`, following TrafficMonitor's create-then-`SetParent` path. The
child paints a taskbar-color-matched two-line status readout, reasserts its taskbar
geometry, and owns a native context menu. Settings toggle, lifecycle, build
and real DPI-aware screenshot are verified.
The Display settings now contain a dedicated taskbar status strip section with
live controls for placement, font weight, and content (`usage`, `speed`, or
`usage + speed`). These settings are persisted, localized, bridged to Rust,
and applied to the running native widget without a restart.
Uncommitted.
Task ID: TASK-TASKBAR-WIDGET-SETTINGS-005
Executor: Codex controller (intervened after Claude's embedded-child dead end)
Branch: `platform/windows`
User authority: "帮当前情况和信息更新进文档，我准备让codex接手处理" — write up the
current state and hand the remaining investigation to Codex.

## Codex intervention — 2026-07-29

The previous `SetParent` child-window implementation is now retired. On this
Windows 11 build it created a real child and received `WM_PAINT`, but DWM/XAML
never composited its pixels into the taskbar. The replacement is a normal
top-level `WS_POPUP` with `WS_EX_TOOLWINDOW | WS_EX_NOACTIVATE | WS_EX_TOPMOST`.
It uses the same taskbar/notification-area anchor math, but never crosses the
Explorer process boundary. A DPI-aware real-screen capture now shows the
expected two lines (`Codex 3%` / `周额度 0%`) in the taskbar area.

This is the selected fallback for the user's earlier "no preference" decision:
keep the setting and data path, render the strip as an independent overlay, and
retain the tray-icon percentage fallback. Do not restore `SetParent` embedding.

## What is done and verified (do not redo)

- `Settings.taskbar_widget_enabled: bool` (default `false`) is persisted via
  `rust/src/settings.rs` / `rust/src/settings/raw.rs`, round-trips through
  `RawSettings`, and is backward compatible with old `settings.json` files.
- A Settings-tab toggle exists (`DisplayTab.tsx`, menu-bar section,
  `TaskbarWidgetLabel`/`TaskbarWidgetHelper`), wired through
  `SettingsUpdate` → `update_settings` → `taskbar_widget::set_enabled`,
  mirroring the floatbar's patch/apply-state flow. Locale keys exist in all
  6 shipped languages and pass `check-locale-drift.mjs` (672 keys).
- `taskbar_widget.rs` has a real lifecycle now: `install()` (read setting at
  startup), `set_enabled(bool)` (live toggle), `stop()` (`DestroyWindow` +
  clear state) — previously there was only a create path, never a destroy
  path.
- The user confirmed the toggle itself works (they found it, turned it on,
  and reported back) — the problem is entirely in whether the strip's
  content actually appears on screen, not in settings plumbing.

## Historical failure record (resolved by the Codex intervention above)

The taskbar strip window is created, is a real window (`IsWindow` /
`IsWindowVisible` both true), receives `WM_PAINT` reliably with a valid
`HDC` every time (confirmed via a temporary `tracing::info!` still present in
`paint()`), and its `FillRect`/`DrawTextW` calls run without error — but the
drawn pixels never appear on screen, confirmed via a **DPI-aware** screenshot
of a screen region independently verified to be empty taskbar background.

See `AGENT_HANDOFF.md`, section "第十五轮", for the full investigation:
four different fixes were tried (three different screen anchors, plus
restructuring window creation to match TrafficMonitor's actual open-source
technique — create standalone, then `SetParent`, then `SetWindowPos` with
`SWP_FRAMECHANGED`, then `ShowWindow`). None of them made the content
visible. This is a real, evidenced dead end, not a guess.

**Critical diagnostic prerequisite**: any screenshot-based investigation of
this must first call `SetThreadDpiAwarenessContext(DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2)`
(value `-4`) in the capturing process before `GetWindowRect`/`CopyFromScreen`.
Without it, coordinates are silently scaled and you will capture the wrong
screen region entirely (this happened once this round and produced a
screenshot of unrelated content, initially misread as a compositing bug).

## Historical untried directions (no longer needed after the overlay fix)

- Unconditionally set `WS_EX_LAYERED` and paint via
  `UpdateLayeredWindow`/`SetLayeredWindowAttributes` instead of plain
  `BeginPaint`/`EndPaint` — TrafficMonitor only does this for its optional
  transparent-background feature, but it may be required more generally on
  this Windows build than their code implies.
- Check whether our process's declared DPI-awareness level differs from
  `explorer.exe`'s, and whether that mismatch affects how DWM composites a
  cross-process `SetParent`-ed child's redirection surface.
- Read TrafficMonitor's GitHub issue history and commit log for
  Windows-11-specific "doesn't show up" reports and their actual fixes,
  rather than only the current main-branch source.
- Try `RedrawWindow(hwnd, NULL, NULL, RDW_INVALIDATE | RDW_UPDATENOW | RDW_ERASE | RDW_ALLCHILDREN)`.

## Decision made for this handoff

The user answered "no preference" and asked Codex to take over. The safe
fallback selected in this handoff is the independent overlay:

1. Keep the taskbar-strip setting and data flow.
2. Render it as an independent, reliably composited Win32 overlay docked over
   the taskbar, rather than as an Explorer child.
3. Keep the existing tray-icon percentage fallback unchanged.

## Required checks (for whatever gets implemented next)

```powershell
cargo check --manifest-path apps/desktop-tauri/src-tauri/Cargo.toml
cargo test --manifest-path apps/desktop-tauri/src-tauri/Cargo.toml
cargo test --manifest-path rust/Cargo.toml
cd apps/desktop-tauri
pnpm build
pnpm test -- --run
node scripts/check-locale-drift.mjs
```

Runtime: `.\scripts\dev-windows.ps1`. The dev chain was left running for this
round; its PID will differ by the time you read this — check the process
list rather than assuming any PID recorded in `AGENT_HANDOFF.md` is current.

## Stop conditions

- Do not commit, push, or discard unrelated uncommitted work.
- Do not pick option 2 or 3 above unilaterally — confirm with the user first;
  they explicitly deferred this decision, they did not delegate it.
- If a fix attempt succeeds, get real screenshot confirmation using the
  DPI-aware capture method above before reporting success — every prior
  "looks right" claim in this task's history that skipped that step turned
  out to be wrong.
