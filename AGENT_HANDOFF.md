# AGENT_HANDOFF

## Active checkpoint: tray percentage removed, tray section moved (2026-08-02)

Executor: Claude Code / claude-opus-5, sole writer. Branch `platform/windows`,
HEAD `b8529fb5`, uncommitted.

User: delete 在托盘中显示百分比, and the other three fields in the 通知区图标 block
belong on the 仪表盘 page.

**Deleted end to end**: `menu_bar_shows_percent` is gone from shared `Settings`
and its raw mirror, the Tauri `SettingsUpdate`/`SettingsSnapshot`, the TS bridge
type, every test fixture, and both `ShowPercentInTray*` locale strings. With the
setting gone, four things became unreachable and were deleted rather than left
as dead weight: `render_percent_icon_rgba` and its private `mark_glyph` /
`mask_has_set_neighbour` / `glyph_rows` helpers (137 lines + 5 tests),
`render_tray_icon_for_settings` (its call site now calls `render_bar_icon_rgba`
directly), `ProviderGrid`'s `showPercent` prop and its two branches, the
`.provider-grid__percentage*` CSS, and the `QuotaNoPercentageForProvider` locale
key whose only consumer was one of those branches. Locale keys 757 → 754.

`render_bar_icon_rgba` already ignored its percentage arguments — the mark has
been static since 第十六轮. Its doc comment still claimed the bars were
"colour-coded by UsageLevel" and pointed at "the optional numeric icon"; both
now say what the function does.

**Moved**: the 通知区图标 section (`trayIconMode`, `switcherShowsIcons`,
`menuBarShowsHighestUsage`) now renders in `DashboardTab.tsx` instead of
`TaskbarTab.tsx`. Behaviour unchanged — only where the controls live.

**Flagged to the user, not silently changed**: only one of the three is
dashboard-only. Traced consumers:
* `switcherShowsIcons` — `TrayPanel.tsx:285,335` and `PopOutPanel.tsx:221` only.
  Genuinely a panel setting.
* `menuBarShowsHighestUsage` — `TrayPanel.tsx:105` (panel ordering) **and**
  `tray_bridge.rs:432,575`, where it picks which provider the native tray icon
  and its status labels show.
* `trayIconMode` — `tray_bridge.rs:566` only. Purely the native tray icon; it
  does not touch the dashboard at all.

So two of the three now sit on a page whose title does not describe what they
do. That is the user's product call to confirm or reverse; the code was moved as
asked and the mismatch recorded here rather than being quietly "corrected".

**Verification, real**: shared Rust 624/624; Tauri crate 369/369; frontend
39 files / 243 tests; guards green (locale 754, provider marks 61, chart
providers 4); `pnpm tauri:build:debug` clean; app relaunched (PID 1432,
responding). No screenshot — same tray-driven capture limitation as the round
below.

## Active checkpoint: sign in instead of pasting cookies (2026-08-02)

Executor: Claude Code / claude-opus-5, sole writer. Branch `platform/windows`,
HEAD `b8529fb5`, uncommitted (this round is on top of the previous uncommitted
work).

User: "现在是让我自己上传 cookies 才能正常获取额度，但是 mac 版本是自己获取的"，
then chose: keep manual upload, add both A (in-app login) and B (local-first).

**Premise corrected first.** macOS does not auto-read browser cookies — the
`platform/macos` branch has no browser-cookie code at all (all deletions vs this
branch). Its `docs/PROVIDER_SOURCE_STRATEGY.md` states the priority as
*local CLI config > OAuth API > browser cookie*, with `browserCookie` opt-in and
Windows DPAPI marked "Phase 3". The "mac opens a browser to authorize" the user
remembers is the **CLI's own** `login` command, which writes a token to disk
(`~/.claude/.credentials.json`, `~/.codex/auth.json`, `~/.grok/auth.json`) —
files this branch already reads.

Windows already has automatic DPAPI cookie extraction (`browser/cookies.rs`).
It fails on Chrome/Edge 127+ **App-Bound Encryption**, which the code detects
and reports, falling back to the manual-paste message the user was seeing.
Defeating ABE through the elevated `IElevator` COM path was considered and
**rejected**: fragile across Chrome updates and detection-evasion-adjacent.

### B — local credentials stop being unreachable

Root cause found: `DEFAULT_COOKIE_SOURCE = "manual"` (settings.rs:639), and
`build_fetch_context`'s `"manual"` arm pinned every cookie provider **without** a
stored cookie to a single `SourceMode` — `Cli` in general, `OAuth` for Claude.
Pinning is what made a paste feel mandatory: a provider holding a perfectly good
local token was never allowed to try it. That arm now falls through to the
provider's own `usage_source` (default `Auto`), so the provider runs its full
ladder. A cookie the user supplied on purpose still goes straight to
`SourceMode::Web` — the manual path is untouched.

Claude's `Auto` ladder also opened with the browser session, so a user already
signed in through `claude login` watched every refresh fail on ABE before
anything read the token on disk. Order is now Admin API → OAuth → CLI → Web,
lifted into a named `AUTO_SOURCE_ORDER` const so the policy is testable without
a network call (`auto_reads_local_credentials_before_any_browser_session`).

### A — in-app login, generic across all 25 cookie providers

New `commands/provider_login.rs`. The session is established in a webview *we
own*, so its cookie store is ours to read and no other application's encryption
is involved — ABE is sidestepped, not attacked.

* `open_provider_login` builds one reusable window (label `provider-login`) at
  `https://{resolved cookie domain}/`. No per-provider login-URL table: the
  destination is derived from the same `provider_cookie_domain` resolver the
  capture filter uses, so a `cn` MiniMax account cannot be sent to the
  international host, and there is nothing to drift.
* `capture_provider_login` is **async on purpose** — reading the WebView2 cookie
  store blocks on the event loop and a sync command would deadlock the app.
* Capture writes into the existing `ManualCookies` store, so every provider
  fetcher picks it up with **zero per-provider wiring**. It reuses
  `browser_import.rs`'s domain matching and dedupe (now `pub(crate)`), which is
  what keeps one provider's login from walking off with another site's session
  and what preserves Claude's companion domains.
* The login window's label is deliberately absent from
  `capabilities/default.json`, so the remote page gets no IPC.

UI: `CookieSection.tsx` leads with the sign-in block; the paste textarea stays
below untouched. Capture is a button, not a poll — only the user knows when SSO
/ 2FA / an org picker is finished, and guessing would store a half-authenticated
session. Failure keeps the window open so a too-early click does not discard
sign-in progress. Switching providers closes any open window.

**Verification, real**: shared Rust 629/629; Tauri crate 370/370 (6 new
`provider_login` tests); frontend 39 files / 243 tests (5 new `CookieSection`
tests, the provider-switch one made non-vacuous by clearing the mount call);
all three guards green (locale 757 keys, provider marks 61, chart providers 4);
`pnpm tauri:build:debug` clean, app relaunched (PID 50792, responding).

**Not verified**: no screenshot of the new settings block. The window has to be
opened from the tray and this environment's automated Windows capture is the
known-broken path recorded in 第十五轮. The user should open Settings → a
cookie provider → Browser Cookies to eyeball it.

**Left for the user (per the rules, credentials/privacy are theirs)**: whether
in-app login should also be offered for providers where a CLI already covers it,
and whether the login window should persist its own profile separate from the
app's.

## Active checkpoint: one cycle classifier, not three (2026-08-01)

Executor: Claude Code / claude-opus-5, sole writer. Branch `platform/windows`,
HEAD `b8529fb5`, uncommitted.

User asked whether the three surfaces share their data. They share the *fetch* —
all read the same `ProviderUsageSnapshot` from `AppState.provider_cache`. They
did **not** share the derivation: "which window is the weekly one" was
implemented three times, and the copies had drifted.

| where | served | matched on |
| --- | --- | --- |
| `taskbar_entries.rs::window_kind` | taskbar | length → else slot name |
| `lib/quotaWindows.ts::windowByKind` | float bar | **length only** |
| `MenuCard.tsx::isWeeklyWindow` | card forecast | `minutes >= 7d`, **no upper bound** |

Two live consequences, both now fixed:
* a provider publishing a percentage with no declared length matched no cycle in
  TypeScript, so the float bar showed nothing where the strip showed a reading;
* a **monthly** window satisfied the card's open-ended weekly test, so it
  collected the weekly forecast and was labelled "Weekly" in
  `ProviderQuotaBlock`, while the strip called the same window 月.

The bridge was handing out ingredients (`windowMinutes`) and letting every
consumer cook. Fix: classify once, ship the answer. New
`src-tauri/src/quota_cycle.rs` owns the bands; `RateWindowSnapshot` gains
`kind: Option<&'static str>`, filled in `bridge.rs` from the declared length and
the provider's own slot label. All three consumers now read the field.

A drift *guard* was considered and rejected — a guard concedes the duplication
and only promises the copies stay equal. This deletes the second and third copy,
so there is nothing left to keep in step. Net **−1 module of logic**, not +1
script. (The three existing `check-*.mjs` guards stay: locale keys, provider
marks and chart providers state facts about *what code we have written*, which
cannot be derived. Rule of thumb now recorded: derivable → delete the duplicate;
not derivable → guard it.)

Also fixed while in there: `windowByKind("primary")` took the primary slot on
trust, so a prepaid provider's synthetic 0%/100% balance carrier — which is *not*
flagged informational — was selected and printed a reset time for a window that
is not a cycle. Rust already skipped these; TypeScript now does too, by text
shape rather than a provider list.

Files: `quota_cycle.rs` (new), `commands/bridge.rs`, `taskbar_entries.rs`,
`tray_bridge.rs`, `main.rs`, `lib/quotaWindows.ts`, `lib/quotaWindows.test.ts`
(new, 8 tests), `components/MenuCard.tsx`, `components/ProviderQuotaBlock.tsx`,
`lib/trayProviders.ts`, `types/bridge.ts`, plus 12 test files whose fixtures now
state their cycle instead of implying it through a length.

**Expect visible changes** — that is the fix, not a side effect. A provider whose
only window is monthly loses the weekly forecast on its card and stops being
labelled "Weekly"; the float bar may gain a row for a provider whose cycle it
could not previously name, and lose a bogus reset row on a prepaid account.

Verification: shared Rust 628/628, Tauri crate 364/364 (+7), frontend 38 files /
238 tests (+8), `pnpm run build` green (locale 751, provider marks 61, chart
providers 4), debug rebuilt and relaunched (PID 30400). Not committed.
**Not visually verified.**

## Previous checkpoint: Grok local usage scanner (2026-08-01)

Executor: Claude Code / claude-opus-5, sole writer. Branch `platform/windows`,
HEAD `b8529fb5`, uncommitted. Files: `rust/src/cost_scanner.rs`,
`apps/desktop-tauri/src-tauri/src/commands/chart.rs`.

User asked where Grok's recent usage was. It was never implemented:
`chart.rs::scan_local_cost` matched only `"codex"` and `"claude"` and returned
`None` for everything else, so `load_local_usage_summary` bailed and the block
never rendered. The data was there the whole time — the Grok CLI writes a
`turn_completed` event per assistant turn into each session's `updates.jsonl`
with `inputTokens`, `outputTokens`, `cachedReadTokens`, `costUsdTicks` and a
per-model `modelUsage` breakdown.

Two things were checked against the real logs *before* writing the scanner,
both of which would have produced silently wrong numbers:

* **The turn totals are per-turn, not cumulative.** The first two records I
  looked at rose, which reads as a running total; across all three multi-turn
  sessions, zero had non-decreasing totals. Summing cumulative snapshots would
  have inflated everything. Pinned by `sums_grok_turns_rather_than_taking_the_last`.
* **`cachedReadTokens` is a SUBSET of `inputTokens`**, unlike Claude's cache
  fields which sit alongside its input count. The logs satisfy
  `inputTokens + outputTokens == totalTokens` exactly. Adding cache to input
  would double-count the majority of all input (97% of it is cached reads on
  these logs). Pinned by an assertion against the log's own `totalTokens`.

`costUsdTicks` carries no unit. Scale inferred by magnitude and isolated in
`GROK_COST_TICKS_PER_USD = 1e9`: one observed turn is 12_971_000_000 ticks for
2.7M tokens, i.e. $12.97 at 1e9 and an impossible $12,971 at 1e6. **If this is
ever shown to be wrong, that constant is the only thing to change.**

Only `updates.jsonl` is read per session directory — the other dozen files
(`chat_history.jsonl` is often the largest) carry no turn events. Events are
de-duplicated by `_meta.eventId` because a resumed session replays them.
`GROK_HOME` is honoured the same way `GrokProvider` honours it.

Predicted from the current real logs, for checking the UI against:
30d ≈ **35,274,477 tokens / $222.70 / 3 sessions**, top model `grok-4.5-build`;
7d ≈ 35,003,442 tokens / $219.81 / 2 sessions.

**Known imprecision, deliberately not changed:** the block's footnote is
`PanelEstimatedFromLocalLogs` ("estimated from local logs"). For Grok the cost
is the CLI's own reported figure rather than something priced by our tables, so
"estimated" understates it. Correcting the wording means a new locale key across
6 `.ftl` + `locale.rs` + `keys.ts`; left for the user to decide, since it is
wording, not data.

**The backend alone showed nothing** — user reported the card was still empty in
both the flyout and Settings. Cause: a SECOND allowlist,
`src/lib/providerCharts.ts::PROVIDER_CHART_DATA_IDS = {claude, codex, openai}`,
which decides whether the frontend calls `get_provider_chart_data` at all. It
said no for Grok, so the working scanner was never asked. Nothing failed; the
feature was simply invisible.

Fixed by adding `grok` — and, because two hand-maintained copies of one fact
will drift again, by adding `scripts/check-chart-providers.mjs` to `prebuild`
(alongside the existing locale and provider-mark guards). It parses the match
arms of `scan_local_cost` and the guard clause of
`load_openai_dashboard_chart_data` and fails the build when the TS set differs,
naming which side is missing what. Confirmed non-vacuous: removing `grok` from
the TS set reproduces the reported bug as a build failure.

Verification: shared Rust 628/628 (+5 new), Tauri crate 357/357, frontend 37
files / 230 tests, `pnpm run build` green (locale 751, provider marks 61, chart
providers 4), debug rebuilt and relaunched (PID 54376). Not committed.
**Not visually verified** — needs the Grok card opened.

## Previous checkpoint: preview reads the renderer, settings window fades in (2026-08-01)

Executor: Claude Code / claude-opus-5, sole writer. Branch `platform/windows`,
HEAD `b8529fb5`, uncommitted.

**1. The settings preview was lying.** User screenshot: preview said
`grok 周 18%` / `deepseek 余额 不支持` while the real strip beside it showed
`G 周 47%` / `D ¥38.38`. Cause: `TaskbarTab.tsx` composed its own imitation from
a hardcoded `samples` table and a hardcoded `不支持` for every `balance` entry —
written before balances rendered, never revisited. This is exactly the failure
`taskbar_entries.rs` already warns about for the availability menu; the preview
had the same disease and no one had said so.

Fix: the preview now renders the renderer's own line buffer.
`taskbar_widget::current_entries()` exposes it, `get_taskbar_preview_lines`
bridges it (glyph + `#rrggbb` + text), and the tab renders those verbatim,
column-major in two columns with the mark in its own colour. `update_settings`
refreshes tray presentation before it returns, so the buffer is already current
when the tab re-fetches after an edit — no polling. **Drift is now impossible
rather than unlikely**; nothing about the strip's content is restated in TS.

Files: `taskbar_widget.rs`, `commands/settings.rs`, `main.rs`, `lib/tauri.ts`,
`types/bridge.ts`, `TaskbarTab.tsx`, `styles.css`, `TaskbarTab.test.tsx`,
`ComponentIsolation.test.tsx`.

**2. Settings window entrance animation.** The window is prewarmed and re-shown,
never recreated, so the React tree never remounts and a CSS mount animation
would run once per app launch. New `SETTINGS_REVEALED_EVENT`, emitted from both
show paths — and in `open_or_focus` only when `is_visible()` was false *before*
`show()`, so clicking the tray at an already-open window focuses without
replaying a fade. Frontend drives it with the Web Animations API (170ms, opacity
+ 0.972 scale, `cubic-bezier(0,0,0,1)`), skipped under `prefers-reduced-motion`.
Only the frontend-drawn frame is animated, which is legitimate here because the
window is borderless — the CSS card *is* the whole visible window.

**First attempt flashed** — reported as "像是 bug". Cause: the frame rested at
full opacity while hidden, so `show()` painted a complete window for the frame
or two the reveal event spent crossing IPC, and the fade then started from 0.
Appear → blink out → fade in. No easing fixes that; it is an ordering fault.

Fix: the frame's opacity now *tracks window visibility* as an invariant —
**hidden ⟺ opacity 0**. `SETTINGS_HIDDEN_EVENT` is emitted from `dismiss()`
after the hide (so the reset happens behind an already-invisible window), and
the frame is parked at 0 on mount, which is safe because the component commits
while the window is still hidden. `show()` can then only ever reveal something
already transparent.

Two ordering rules keep it safe, both commented in place:
* the listeners are attached **before** `revealSettingsWindow()` is called — a
  lost reveal event would leave a visible window whose frame is parked at 0,
  i.e. Settings opening blank;
* the reveal handler sets the resting opacity **before** animating, so a
  cancelled or unsupported animation still leaves the window visible.

`dismiss()` is the only hide path for the `settings` label — the other
`window.hide()` calls in `shell/window.rs` and `shell/transition.rs` all operate
on `"main"`. Verified by grep; if a second hide path is ever added it must emit
`SETTINGS_HIDDEN_EVENT` or the next open will flash again.

Files: `shell/settings_window.rs`, `commands/surface.rs`, `App.tsx`.

Verification: shared Rust 623/623, Tauri crate 357/357, frontend 37 files /
230 tests, `pnpm run build` green, locale 751, provider marks 61, debug rebuilt
and relaunched (PID 38408). Not committed.
**Neither item is visually verified by me** — both are visual by nature.

## Previous checkpoint: Grok declares a weekly cycle (2026-08-01)

Executor: Claude Code / claude-opus-5, sole writer. Branch `platform/windows`,
HEAD `b8529fb5`, uncommitted. Files: `rust/src/providers/grok/mod.rs`,
`apps/desktop-tauri/src-tauri/src/taskbar_entries.rs` (doc/test wording only).

User reported Grok is evidently on a weekly cycle and asked for it to be
selectable as 周额度. Checked before changing anything, and **"monthly" was never
measured**:

* `session_label: "Monthly"` was a hand-written constant.
* `monthly_window_minutes` asserted the period started one calendar month before
  the reset date. Its own doc comment justified that by citing
  `session_label: "Monthly"` — a constant a few lines above it. The two agreed
  only because they were the same guess.
* `resets_at` itself is a heuristic: `parse_grpc_web_response` scans every varint
  in the payload and takes the earliest plausible future timestamp. Nothing in
  the response states a cycle.

So there was no evidence for monthly, and an observation of a live account beats
a circular citation. This is a wrong constant being corrected, not a
per-provider exception carved into the general matcher — the matcher from the
previous checkpoint is untouched and still reads whatever the provider declares.

Changes: `session_label` → `"Weekly"`; `monthly_window_minutes` replaced by
`CYCLE_WINDOW_MINUTES = 7 * 24 * 60`, published **unconditionally** rather than
only when a reset date arrives. The length is a property of the plan, not of
whether one response happened to include a timestamp, and gating it on that
timestamp is what left Grok with no identifiable window at all when the date was
missing. It is also load-bearing for the forecast: `UsagePace` returns nothing
without it, and the frontend refuses to forecast a window with no `windowMinutes`.

Two new tests in `grok/mod.rs`: the cycle is declared with or without a reset
date, and the slot name agrees with the declared length (they are read by
different consumers — card prints the name, strip identifies by the length — and
a disagreement makes two surfaces describe one quota differently).

**If this turns out wrong, the constant and `session_label` are the only two
places to change, and a test will fail if you change one and not the other.**
There is still no way to derive the cycle from the payload; if x.ai ever returns
a period start, derive it from that instead of declaring it.

Verification: shared Rust 623/623, Tauri crate 357/357, debug binary rebuilt and
relaunched (PID 47804). Frontend untouched this round. Not committed.
**No visual verification** — needs reading on the real taskbar.

## Previous checkpoint: identify the cycle, don't label it "main" (2026-08-01)

Executor: Claude Code / claude-opus-5, sole writer. Branch `platform/windows`,
HEAD `b8529fb5`, uncommitted. File: `apps/desktop-tauri/src-tauri/src/taskbar_entries.rs`.

The previous checkpoint (below) added a `primary` kind that always resolved but
frequently rendered *unlabelled*, and offered 主额度 in the menu next to the real
cycle names. The user rejected it: **identify whether the quota is weekly or
monthly — don't call it "main".** Correct. `primary` guaranteed a number would
appear; it did not answer what the number measured.

The answer was already in the codebase and had simply never been read.
`ProviderMetadata` carries `session_label` and `weekly_label` — the provider's own
name for each slot. Grok's `session_label` is literally `"Monthly"`. The matcher
only ever looked at `window_minutes`, which Grok derives from its billing reset
date, so a billing response without one left the length empty and *nothing*
matched.

`window_kind(window, label)` now asks two sources in order of authority:

1. `window_minutes` — measured data, precise, but optional.
2. the provider's own slot name — parsed by `kind_from_label`. A stated length
   ("5-hour", "7-Day", "Session (5h)") is converted to minutes and run through
   the **same bands** as a declared length, so a label and a length can never
   disagree about a boundary, and "7-Day" is a week rather than a day. Otherwise
   a bare cycle word ("Monthly", "Weekly cost", "Daily AFP").

This is what makes it general rather than per-provider: every provider already
had to name its windows to render a dashboard card, so a provider added tomorrow
arrives with its answer filled in. `minutes_from_label` requires a unit
immediately after the number, so "GPT-4" and "Sonnet 4.5" are not durations.
Slot names that state no cycle ("Credits", "Balance", "On-demand", "Gemini Pro")
return `None` deliberately — those are not dated cycles and inventing a word for
them would put an untrue label on the strip.

`primary` survives as an **escape hatch, not a synonym**: `available_windows`
offers it only when the main window's cycle cannot be named by either source.
Grok now offers 月额度 and nothing else. Codex offers 5小时/周. A stored `primary`
entry still resolves and still names itself, because rewriting a user's saved
configuration behind their back stays rejected.

Two traps worth keeping: `"daily"` does not contain the substring `"day"` (a
test caught this), and `kind_from_label` must return a stated length's verdict
*exclusively* — falling through to word matching would call a 14-day cycle
"daily".

Verification: Tauri crate 357/357 (21 in `taskbar_entries`), frontend 37 files /
230 tests, debug binary rebuilt and relaunched (PID 53640). Shared Rust untouched
this round. Not committed. **No visual verification** — the strip has to be read
on the real taskbar.

## Earlier checkpoint: `primary` window kind — no more per-provider bands (2026-08-01)

Executor: Claude Code / claude-opus-5, sole writer. Branch `platform/windows`,
HEAD `b8529fb5`, uncommitted. Files: `rust/src/settings.rs`,
`taskbar_entries.rs`, `tray_bridge.rs`, `types/bridge.ts`, `tabs/TaskbarTab.tsx`,
`rust/src/locale.rs`, `i18n/keys.ts`, all six `.ftl`.

Grok was still 不支持 after the last round, and the user named the real problem:
**stop fixing this per provider — what happens to the next one?**

They were right. Window selection matched a window's declared length against
four fixed bands (session ≤6h, daily 20–28h, weekly 6–8d, monthly ≥27d). That is
a taxonomy, and a taxonomy always has things outside it:

* a cycle in no band — 14 days,
* a percentage published with **no length at all** — which is Grok whenever its
  billing response carries no reset date, since `monthly_window_minutes` derives
  the length from that date,
* anything a future provider invents.

All of those were unrenderable no matter what the user picked, and no amount of
per-provider tuning fixes the class.

New `primary` kind: *this provider's main quota, whatever cycle it turns out to
be.* Asks no question about length, so it is the one kind guaranteed to resolve,
and it leads the dropdown. `primary_resolves_for_any_provider_whatever_its_cycle`
pins all five shapes above.

The label is **derived, not fixed**: a `primary` entry is named after the cycle
it landed on, so the strip reads `◆ 周 12%` when the length is known and `◆ 12%`
when it is not — never the word "primary", which tells a reader nothing.
`derived_kind` returns `None` for a length in no band, and the entry renders
unlabelled rather than mislabelled.

**The pre-existing availability test caught a real bug** while this went in: a
prepaid provider synthesises a 0%/100% window purely to carry its amount through
`reset_description`, and it is *not* flagged informational — so a length-agnostic
`primary` search happily selected it and would have printed `0%`. A fabricated
measurement, which this module's header exists to forbid. `primary` now skips any
window whose description parses as a balance, detected **by the shape of the
text, not a provider list**, so a new prepaid provider is covered on arrival.

Verification: shared Rust 621, Tauri crate 354, frontend 37 files / 230 tests,
`pnpm run build` green, locale 751, provider marks 61, debug binary rebuilt and
relaunched (PID 22820). Not committed.

Note for whoever picks this up: existing settings keep whatever kind they stored,
so a strip configured before this still shows 不支持 until the entry is switched
to 主额度. Changing stored entries silently was rejected — it would rewrite a
user's configuration behind their back.

## Earlier checkpoint: strip shows balances, dropdown stops offering dead choices (2026-08-01)

Executor: Claude Code / claude-opus-5, sole writer. Branch `platform/windows`,
HEAD `b8529fb5`, uncommitted. Files: `taskbar_entries.rs`, `tray_bridge.rs`,
`commands/settings.rs`, `main.rs`, `lib/tauri.ts`, `tabs/TaskbarTab.tsx`,
three test files.

Two defects, one root cause: the strip refused data it could have shown, and the
composer offered choices that could never resolve.

### The strip now prints a balance

`resolve_entries` returned `WindowUnsupported` for `balance` **unconditionally**,
on the stated grounds that "the strip prints percentages". That made the
dropdown's own 余额 option dead for every provider, and left prepaid accounts
(DeepSeek) with nothing the strip could show — while the floating bar showed
theirs fine.

`ResolvedEntry` gained `amount: Option<String>`, kept separate from `percent`
because a balance carries a currency and no denominator: it can never drive a
bar or a threshold colour. The window word is dropped from the cell — the amount
already says what it is.

`balance_amount` scans primary/secondary/tertiary for a `reset_description`
shaped like money, rather than hardcoding the per-provider map (DeepSeek uses
`primary`, MiMo uses `secondary`) that drifts every time a provider is added.
Only the leading amount is kept: `"¥38.88 (Paid: … / Granted: …)"` → `¥38.88`.
It mirrors `lib/providerBalance.ts::parseBalanceText` but deliberately narrower —
the strip only needs "is there an amount".

Guards that matter: "Balance unavailable" is a state, not an amount, and a bare
number with no currency is far likelier to be a token count than money. Both are
pinned by `balance_parsing_rejects_everything_that_is_not_money`.

### The window dropdown adapts to the provider

New `available_windows(snapshot, has_speed)` + command
`get_taskbar_window_availability`, so the composer offers only kinds that
provider can answer for. Grok publishes one monthly window and nothing else,
which is why picking 周 for it saved fine and then printed 不支持.

**It shares `window_by_kind` / `balance_amount` with `resolve_entries` on
purpose.** Computing availability independently would let the menu and the strip
disagree, which is a worse failure than the one being fixed;
`every_offered_window_actually_resolves` asserts the two agree for every kind on
every fixture.

Two UI decisions worth keeping:

* `auto` offers the **union** — it follows whichever provider the tray settles
  on, so narrowing it would hide kinds that become correct on the next switch.
* The **current selection is always kept**, even when unavailable. Dropping it
  would make the control display a different value than the one stored; the
  honest signal for a stale entry is the 不支持 the strip prints.
* `availability === null` (not loaded) offers everything, so a slow backend never
  silently removes the user's choice.

`ComponentIsolation.test.tsx` also needed the new mock — its `vi.mock` replaces
the whole `lib/tauri` module, so any command a rendered tab calls must be
present or the render throws.

Verification: shared Rust 621, Tauri crate 352, frontend 37 files / 230 tests,
`pnpm run build` green, locale 750, provider marks 61, debug binary rebuilt and
relaunched (PID 48136). Not committed. Unverified visually.

## Previous checkpoint: strip prints a brand mark instead of the provider name (2026-08-01)

Executor: Claude Code / claude-opus-5, sole writer. Branch `platform/windows`,
HEAD `b8529fb5`, uncommitted. Files: `provider_mark.rs` (new), `taskbar_text.rs`,
`taskbar_widget.rs`, `taskbar_entries.rs`, `tray_bridge.rs`, `main.rs`,
`scripts/check-provider-marks.mjs` (new), `package.json`.

Four cells in a two-column strip clipped. Measured at 12px Microsoft YaHei UI,
the provider NAME is the longest part of a cell:

| cell | before | after |
| --- | --- | --- |
| `Codex 周 1%` → `◆ 周 1%` | 73px | 47px |
| `Claude 5小时 12%` → `◈ 5小时 12%` | **103px** | **73px** |
| `Grok 周 不支持` → `G 周 不支持` | 83px | 66px |

Cell width is 91px at a 200dip strip, so the Claude cell was the one clipping;
every cell now fits with room to spare.

**No SVG rasteriser.** Every provider already carries a one-character
`fallbackLetter` and a `brandColor` in `providerIcons.ts` — `◈` Claude, `◆`
Codex, `阿` Alibaba, `☽` Kimi. The DirectWrite renderer that is already there
draws those crisply at any size. Rasterising the real 100x100 brand SVGs would
have meant a large new dependency plus a Direct2D bitmap path, and would look
*worse*: at a ~14px row most of those marks reduce to a coloured blob.

Glyphs repeat across providers on purpose (`◈` vs `◆` are near-identical
outlines). **Colour carries the identity** — warm brown vs teal — which is why
`taskbar_text` draws the mark with its own brush rather than the text brush, and
why the rendering test asserts exactly that: a pure-red mark beside pure-blue
text must put red on the bitmap. Zero red means it was skipped or drawn with the
text brush, and the two providers would be indistinguishable.

That assertion had to be **called from `variable_font_weight_axis_is_continuous`
rather than made its own `#[test]`**. `RENDERER` is thread-local, libtest gives
every test its own thread, and a second single-threaded Direct2D factory on a
thread with no COM apartment hangs forever. The existing test already documented
this; a new drawing test would have deadlocked the suite. `ink_for` was
generalised into `render_channels`, which returns per-channel sums so a coloured
mark can be told apart from the text beside it — the weight-continuity numbers
are unchanged (MiSans VF 200→775443 … 900→2791182), which is the refactor's own
regression check.

`provider_mark.rs` is **generated** from `providerIcons.ts` (61 providers) and
guarded by `scripts/check-provider-marks.mjs`, wired into `prebuild` alongside
the locale check. Without that guard the drift is silent: an unknown id just
falls back to printing the provider's name, so the strip still looks right and
quietly loses the mark. `ResolvedEntry` gained `provider_id` to carry the
lookup; `set_entries` now takes `Vec<StripLine>` (mark + text) instead of
`Vec<String>`.

The GDI fallback prints the glyph inline and monochrome — one text colour per
DC. Losing the colour there is acceptable; losing the reading is not.

Verification: shared Rust 621, Tauri crate 347, frontend 37 files / 228 tests,
`pnpm run build` green, locale drift 750, provider marks 61 matched, debug
binary rebuilt and relaunched (PID 30348). Not committed. **No screenshot of the
real strip** — widths are measured and the mark's colour is proven in an
offscreen render, but how the glyphs read on the taskbar is the user's call.

## Previous checkpoint: taskbar strip lays out four entries as 2x2 (2026-08-01)

Executor: Claude Code / claude-opus-5, sole writer. Branch `platform/windows`,
HEAD `b8529fb5`, uncommitted. Files: `taskbar_widget.rs`, `tabs/TaskbarTab.tsx`,
all six `.ftl`.

The strip showed two entries, stacked. It now shows four, as **2 columns x 2
rows** — the layout Windows' own network/CPU monitor widgets use.

`MAX_VISIBLE_LINES = 2` is replaced by `MAX_VISIBLE_ROWS` (2) x
`MAX_VISIBLE_COLUMNS` (2) = `MAX_VISIBLE_ENTRIES` (4). Two text rows is what a
taskbar gives you at a readable size and that has not changed; entries 3 and 4
open a **column**, not a third row.

New pure `cell_rects(client, count, pad) -> Vec<Rect>`, with the paint routine's
inline arithmetic moved into it so the layout is testable without a window.

**Filled column-major**, which is the decision worth remembering:

* 1 and 2 entries lay out **exactly as before** — 1 spans the full height, 2
  stack. Row-major would have moved entry 2 from bottom-left to top-right the
  moment a third was added, silently rearranging existing users' strips.
* A column is a pair that belongs together (up/down, or one provider's two
  windows), which is how the reference layout groups.

`rows = count.div_ceil(columns)` rather than a fixed 2, so a single entry still
spans the full height instead of sitting in the top half of an empty grid.
The gutter between columns comes out of the left column's width, so the right
column still ends on the padded edge.

Seven tests in a new `taskbar_widget::tests` — that file had none. They pin the
1- and 2-entry layouts as unchanged, the column-major fill, that rows line up
across columns, that columns do not touch, the cap at 4, and that a strip
narrower than its own padding produces no inverted rects (DirectWrite rejects
those and leaves the strip blank).

`TASKBAR_MAX_ENTRIES` stays 6: the list still holds spares below the fold, and
entries 5–6 keep the "beyond the strip" note. Two columns at the default 132dip
width will ellipsize — the composer helper now says to widen it. The width clamp
(96–240) is untouched; 240 leaves ~114dip per cell, enough for "Codex 周 59%"
at 12px.

Verification: shared Rust 621, Tauri crate 343, frontend 37 files / 228 tests,
`pnpm run build` green, locale drift 750 matched, debug binary rebuilt and
relaunched (PID 49572). Not committed. **No screenshot of the real strip** —
the geometry is tested, the rendering is not.

## Previous checkpoint: the float bar page had three names (2026-08-01)

Executor: Claude Code / claude-opus-5, sole writer. Branch `platform/windows`,
HEAD `b8529fb5`, uncommitted. Files: `lib/quotaDisplay.ts` (+ 4 test files),
`tabs/FloatBarTab.tsx`, `rust/src/locale.rs`, all six `.ftl`, `i18n/keys.ts`.

### The "重置" prefix is gone

`QuotaResetAt` (added one checkpoint ago) is **removed**, not just unused —
a consumerless locale key is what `CODE_REVIEW.md` P1-1 already punished once.
The absolute form is now `今天 15:00` / `明天 07:30` / `8月2日 07:30`.

My reasoning for adding it was that a wall-clock time does not label itself.
The user overruled it and is right: the string only ever renders in a slot that
already means "reset" — beside a quota title, or in the floating bar's chip
behind a reset icon — so the word was pure repetition in the one place with no
width to spare. `quotaDisplay.test.ts` now asserts the text matches no /reset/i
at all, so it cannot creep back. 751 → 750 keys.

### One name per language for the floating bar

The page was reported as unreadable: "这个是设置哪里的，如果是悬浮栏，那悬浮栏
现在还没有打开呢". Two causes, both real:

* **The surface had several Chinese names at once.** zh-CN: tab and description
  said 浮窗 ("floating window") while the section inside that same page, its
  master switch and every helper said 悬浮栏. zh-TW was worse — 浮動列, 懸浮列
  and 浮窗 together (and I had just added a fourth, 懸浮欄). Now one term each:
  **悬浮栏** / **懸浮列**, the majority spelling already in each file.
* **The master switch was below what it governs.** The page opened with two
  usage-display toggles for a surface that was switched off, several screens
  above the switch that turns it on. `FloatBarSettingsSection` — whose first row
  IS "显示悬浮栏" — now renders first, and `FloatBarSettingsDescription` states
  which surface the page owns instead of describing it.

`FloatBarResetFormatNeedsInline` was reworded to point up, not down.

**When a surface gets a new string, check what the other strings call it.**
Nothing in the build catches a second name; the locale drift check compares key
lists, not vocabulary.

Verification: shared Rust 621, Tauri crate 336, frontend 37 files / 228 tests,
`pnpm run build` green, locale drift 750 matched, debug binary rebuilt and
relaunched (PID 44780). Not committed. Both changes are visual and unverified
by me.

## Previous checkpoint: the float bar hides its own reset setting (2026-08-01)

Executor: Claude Code / claude-opus-5, sole writer. Branch `platform/windows`,
HEAD `b8529fb5`, uncommitted. Files: `tabs/FloatBarTab.tsx`,
`commands/settings.rs` (test), `floatbar/FloatBar.test.tsx` (tests),
`rust/src/locale.rs`, all six `.ftl`, `i18n/keys.ts`.

Reported as "浮窗这两个功能是无效的" (`floatBarShowAsUsed`,
`floatBarResetTimeRelative`). One of the two has a real explanation:

**`floatBarResetTimeRelative` is invisible by default.** `FloatBar.tsx` renders a
reset chip only when `floatBarShowResetInline` is on, and that toggle lives in
the *次* section of the same page. With it off — the default — the reset-time
mode reaches the pill's hover `title` and nothing else. On screen that is
indistinguishable from a dead switch, which is exactly how it was reported.

Not fixed by disabling the row: the tooltip really does follow it, so disabling
would be a lie. `FloatBarTab` now appends `FloatBarResetFormatNeedsInline` to
the field description while inline resets are off. 750 → 751 keys.

**`floatBarShowAsUsed` could not be reproduced.** A pill built from a real
snapshot flips 71% used ↔ 29% remaining, and the bar ignores the dashboard's
copy of both choices. If the user still sees no change there, the next thing to
instrument is delivery of `float-bar-config-changed` to that webview — every
layer either side of it is now pinned.

Coverage added, both at the join nobody was testing:

* `settings::tests::float_bar_display_settings_survive_the_bridge_round_trip` —
  camelCase JSON → `SettingsUpdate` → `notifies_float_bar()` → `apply_to` →
  snapshot back out. **`notifies_float_bar()` is asserted deliberately**: the
  float bar does not use `useSettings`/`settings-changed`, it waits on
  `float-bar-config-changed`, which only that predicate emits. A field left off
  that list would persist correctly and never reach the bar.
* `FloatBar > its own usage-display switches` — three cases: used↔remaining,
  countdown↔wall-clock, and that the dashboard's copy is ignored.
* `FloatBar > prints no reset chip at all until inline resets are switched on` —
  pins the gate above so it cannot silently become the default answer again.

Verification: shared Rust 621, Tauri crate 336, frontend 37 files / 227 tests,
`pnpm run build` green, locale drift 751 matched, debug binary rebuilt and
relaunched (PID 14984). Not committed.

## Previous checkpoint: reset-mode toggle investigated, no defect found (2026-08-01)

Executor: Claude Code / claude-opus-5, sole writer. Branch `platform/windows`,
HEAD `b8529fb5`, uncommitted. Files: `commands/settings.rs` (test only),
`surfaces/TrayPanel.test.tsx` (test only).

Reported as "这个按钮似乎没有用". **Not reproduced.** Every link was traced and
the two that had no coverage now have it:

| link | status |
| --- | --- |
| `{"dashboardResetTimeRelative":false}` → `SettingsUpdate` | now tested |
| `apply_to` writes it, floating bar's copy untouched | now tested |
| `refreshes_tray_presentation()` true → tray repaints | now tested |
| snapshot serializes back as camelCase | now tested |
| `quotaDisplayContext(settings, "dashboard")` → card text | now tested |
| `useSettings` re-fetches on `settings-changed` | pre-existing |

`settings::tests::dashboard_reset_time_mode_survives_the_bridge_round_trip`
covers the Rust half — it calls `apply_to`, not `apply_display_settings`,
because the command calls `apply_to` and a field reachable only by the narrower
helper would be just as dead. `TrayPanel > reset time mode` covers the render
half: the same card, same fixture, toggle on → `Resets in 4h 33m`, toggle off →
`Reset: Today at …`.

Two things the report most likely came down to, neither a code defect:

1. **The switch in the screenshot is still ON** (blue = relative = countdown),
   which is exactly the state being rendered.
2. **There are two independent switches** — Settings → 浮窗 and Settings → 仪表盘
   — and the crop does not show which page it was. 仪表盘 governs the tray flyout,
   the PopOut panel and the Providers-page preview card; 浮窗 governs only the
   floating bar. Flipping one while watching the other's surface looks exactly
   like a dead switch. That independence is item H's whole point, so it is not
   something to "fix", but it is a discoverability trap worth remembering.

Verification: Tauri crate 335 tests, frontend 37 files / 223 tests, `pnpm run
build` green. No production code changed in this checkpoint. Not committed.

## Previous checkpoint: absolute reset mode printed a bare clock time (2026-08-01)

Executor: Claude Code / claude-opus-5, sole writer. Branch `platform/windows`,
HEAD `b8529fb5`, uncommitted. Files: `lib/quotaDisplay.ts` (+ its test),
`hooks/useFormattedResetTime.test.tsx`, `rust/src/locale.rs`, all six `.ftl`,
`i18n/keys.ts`.

The switch the user asked for already existed — `ResetTimeRelative` on both the
FloatBar and Dashboard settings pages, backed by `float_bar_reset_time_relative`
/ `dashboard_reset_time_relative`. What it produced when turned off did not
answer the question it was for:

* **No label.** The countdown templates are self-labelling ("… 后重置"); the
  absolute branch returned raw `Intl.DateTimeFormat` output, so the row read
  `15:30` with nothing saying that was a reset time, sharing a header row with a
  quota title and a percentage.
* **No day for a same-day reset.** The old rule printed the time alone when the
  reset fell on today. That was a deliberate item C choice for row width, but it
  means the mode a user picks to learn *which day* never says today.

Now every absolute reset names its day and carries a label:
`重置：今天 15:30` / `重置：明天 07:30` / `重置：8月2日 07:30`.
Today and tomorrow get words — "8月2日" for something four hours out reads like
a distant deadline.

Two new keys in all six locales: `QuotaResetAt` (colon form, so it composes with
all three day phrasings — "Reset: Today at …" would be wrong English if the
prefix were "Resets at") and `TodayAt`. `TomorrowAt` already existed in all six
and was consumed by nothing; ja-JP still held the untranslated English string,
which this change would have made visible, so it is now `明日 { "{}" }`.
`ResetTimeRelativeHelper` described an output the app never produced
(`显示"2h 30m"而不是"3:00 PM"`) and now shows both real forms. 748 → 750 keys.

Day selection counts **calendar days in the display timezone**, via
`localDayIndex` (format y/m/d in that zone, repack through `Date.UTC`), not
elapsed hours — 23:30 and 00:30 are an hour apart and two different days, and a
24-hour subtraction also breaks across DST. Both are pinned by tests, as is the
timezone dependence (02:00Z on the 31st is "today" in New York, "tomorrow" in
UTC).

`fill()` appends when a template has lost its `{}` rather than returning the
template unchanged, so a broken translation degrades to "Reset 15:30" instead of
silently dropping the time the string exists to carry.

Verification: shared Rust 621 tests (covers `.ftl` completeness across all six
languages), frontend 37 files / 221 tests, `pnpm run build` green, locale drift
750 keys matched, debug binary rebuilt and relaunched (PID 37056). Not
committed. The default is still relative; flipping it is the user's click.

## Previous checkpoint: forecast runway icon was hanging above the text (2026-08-01)

Executor: Claude Code / claude-opus-5, sole writer. Branch `platform/windows`,
HEAD `b8529fb5`, uncommitted. Files: `components/MenuCard.tsx`, `styles.css`,
`design/qa/forecast-icon-alignment.html` (new).

Reported twice as "图标是歪的". It was never rotated — the glyph is an exact
circle plus an exactly vertical bar. **An inline `<svg>` defaults to
`vertical-align: baseline`, which rests its bottom edge ON the baseline**, so a
13px icon on an 11px line floated 1.5–3.0px above the text's optical centre.

The first fix I reached for was wrong and is worth remembering: copying
`.menu-card__pace-runway-status`'s `display:inline-flex; align-items:center`.
That works there because its parent does not baseline-align. `.menu-metric__forecast`
**does** (`align-items: baseline`), and a flex container takes its baseline from
its first flex item — the icon — which drops the whole "按当前速度…" clause off
the shared baseline. Measured: it moved the error rather than removing it.

Shipped instead, on `.menu-metric__forecast-runway svg`: `width/height: 1.2em`
+ `vertical-align: -0.2em`. Sizing in `em` makes the icon track the flyout's
`--flyout-u` text scaling like every other icon in the card, and keeps the
correction exact at every size.

`design/qa/forecast-icon-alignment.html` measures all three variants live:

| variant | icon off-centre (10–13px) | clause baseline drift |
| --- | --- | --- |
| A inline svg, `baseline` (the bug) | +3.00 … +1.50 | 0.00 |
| B wrapper as inline-flex (the trap) | −3.50 … −5.00 | −6.50 |
| C 1.2em + `-0.2em` (shipped) | +0.50 … +0.20 | 0.00 |

**Do not measure text alignment with `getBoundingClientRect` midpoints** — an
inline box is inflated by `line-height`, so its centre moves with leading and
its bottom is not the baseline. The probe reads the baseline from a zero-sized
`inline-block` marker and the glyph extent from canvas
`actualBoundingBoxAscent/Descent`. My first version of that page used bounding
boxes and reported a baseline drift on variant A that does not exist.

Also replaced the shared ring in `PaceIcon`/`CheckIcon`/`WarnIcon` — a two-arc
path whose chord is exactly the diameter, the degenerate case for the large-arc
flag, plus a zero-length closing segment — with `<circle cx=12 cy=12 r=9>`.
Same shape, no degenerate input for the rasteriser. Mirror-symmetry error of
the rendered pixels measured 7.64% → 6.89%: real but minor, and NOT what was
being reported.

Verification: frontend 37 files / 217 tests, `pnpm run build` (tsc --noEmit +
locale drift 748 keys + vite) green, debug binary rebuilt and relaunched
(PID 51128). Not committed. **Visually unverified by me** — the alignment
numbers are measured, but only the user can confirm it now reads straight.

## Previous checkpoint: taskbar entry composer was inert (2026-08-01)

Executor: Claude Code / claude-opus-5, sole writer. Branch `platform/windows`,
HEAD `b8529fb5`, uncommitted.

Item G's composition UI was shipped **fully non-functional** and three green
test suites did not notice. Two separate breaks, both in the bridge:

### The save path did not exist

`commands::SettingsUpdate` had **no `taskbar_widget_entries` field at all**.
Serde drops unknown keys silently, so `set({ taskbarWidgetEntries })` was
accepted by the UI, sent, and discarded without an error anywhere.

Why the tests missed it: the TypeScript test asserts `set` was *called*, the
shared-Rust test asserts `normalize_taskbar_entries` *works*. Nobody tested the
layer between them. **When a setting is added, the regression test belongs on
the bridge round-trip, not on either end.**
`commands::tests::settings_update_accepts_taskbar_entries_from_the_frontend`
now covers it.

### The read path shipped snake_case

`#[serde(rename_all = "camelCase")]` renames a struct's **own** fields only —
never a nested type's. `SettingsSnapshot` embedded
`codexbar::settings::TaskbarEntry` (the on-disk type, correctly snake_case), so
the frontend received `{"provider_id": ...}` and read `entry.providerId` →
`undefined` → a blank provider dropdown.

Fixed with `TaskbarEntryBridge` in `bridge.rs`, which is what every other nested
bridge type already does. **Do not put an on-disk settings type in a bridge
struct.** The disk format stays snake_case; the wire format is camelCase.

### Font weight slider looked dead while dragging

The preview set only CSS `font-weight`, which the engine may round to the
nearest 100. With `step={10}`, nine of every ten slider positions rendered
identically. The preview now also sets
`font-variation-settings: "wght" N` — the CSS analogue of the renderer's
`SetFontAxisValues`. Both are kept: the axis for variable families, the keyword
for static ones that have no axis to drive.

Persistence still commits on pointer release, deliberately — every `set()` is a
DPAPI-encrypted disk write, so committing per drag event is not an option.

### Verification, real

- Tauri crate **334 tests**, shared Rust **621 tests**, frontend **37 files /
  217 tests**, `tsc --noEmit` clean, `pnpm build` succeeded, locale **748 keys**.
- Debug binary rebuilt and relaunched.
- **Not visually verified** — the three reports above are visual and only the
  user can confirm them on screen.

## Active checkpoint: five review follow-ups (2026-08-01)

Executor: Claude Code / claude-opus-5, sole writer. Branch `platform/windows`,
HEAD `b8529fb5`, uncommitted. No commit, push or merge.

The user reported five defects after using the build. All five are addressed.

### 1. Font picker was too long, and the strip's font was noticeably poor

`taskbar_text::FontFamilyInfo` gained `has_cjk` and `recommended`. Coverage is
asked of the font itself (`IDWriteFont::HasCharacter(U+4E2D)`), never guessed
from the family name. `family_rank` orders: variable+CJK, variable, curated+CJK,
curated, everything else — and the same ranking drives the sort, the
`recommended` flag and the settings page's "restore defaults" pick.

Measured on this machine: **398 families → 22 recommended**, and the top entry
is **MiSans VF**, which is the Xiaomi variable font the user remembered. Its
`wght` axis is genuinely continuous — ink by weight: 200→775443, 400→1620822,
430→1710273, 560→2108484, 700→2508369, 900→2791182. The 430 and 560 values
differ from the stops around them, which is what proves an axis rather than face
snapping.

Alibaba PuHuiTi is in `PREFERRED_FAMILIES` but is **not installed here**, so it
does not appear. The list is filtered against the live font collection; the
constant is a preference order, never a source of names.

The full ~400 families stay reachable behind a "show all" switch. Nothing is
hidden permanently.

### 2. The taskbar entry composer looked wrong

It referenced two classes that **do not exist** (`settings-section__description`,
`settings-select`) — the same mistake as the invisible pace marker. Now built
from the real grouped-list vocabulary: rows inside `.settings-section__group`
with `.settings-field`-matching padding, hairline separators, an index chip,
full-width dropdowns, a right-aligned action cluster, and "add entry" as a final
row of the same card rather than a link hanging below it.

**Before adding markup, confirm every class it references exists in
`styles.css`.** `--border-subtle` was also wrong; the real token is
`--panel-border`.

### 3. The floating bar could not say which resets it showed

New `float_bar_reset_windows: Vec<String>` (shared Rust, normalized, capped at
`FLOAT_BAR_MAX_RESET_WINDOWS` = 3, order preserved). Default `["primary"]`
reproduces the old behaviour exactly, so an upgrade changes nothing on screen.

Windows are resolved by **declared cycle length**, mirroring
`taskbar_entries::window_by_kind`, in the new `lib/quotaWindows.ts`. A provider
that does not publish the requested cycle prints nothing for it — never another
window standing in. `primary` and a named cycle that resolve to the same window
are deduped, because Codex's primary *is* its weekly and two identical chips
would read as two deadlines. Labels appear only when a pill carries more than
one reset.

An empty selection survives normalization: "show no reset text" is a real choice.

### 4. The floating bar icon looked crooked

Two causes, both geometry. `.provider-icon--svg svg` is sized `84%`, and 84% of
an 11px box is 9.24px — the artwork landed off the device-pixel grid. The shared
`.provider-icon` badge (rounded background plus a 1px inset ring) is also drawn
for a 22px avatar; around an 11px glyph the ring alone ate a quarter of the box.

Fix: icon sizes are forced **even** in `FloatBar.tsx`, and `FloatBar.css`
overrides the badge inside a pill so the SVG fills its square exactly.
`line-height: 0` removes the inline half-leading that shifted it further.

### 5. Grok had no output speed

Grok's local logs were decoded for this. It is **not** shaped like the other
two: `~/.grok/sessions/<cwd>/<id>/updates.jsonl` carries a `turn_completed`
update with a complete `usage` object —
`{inputTokens, outputTokens, totalTokens, reasoningTokens, modelCalls, apiDurationMs, ...}`.

Two facts pinned by tests, because the rate doubles silently if either changes:

- `totalTokens == inputTokens + outputTokens`, and `reasoningTokens` is a
  **subset** of `outputTokens`, not an addition. So `outputTokens` is directly
  comparable to Codex's `last_token_usage.output_tokens` and Claude's
  `usage.output_tokens`.
- `apiDurationMs` is summed model time across the turn's `modelCalls`. Wall
  clock would also count tool execution between calls and understate the rate on
  tool-heavy turns, so it is used as reported rather than re-derived.

Note the search is by **filename**: a Grok session directory holds several
`.jsonl` files and only `updates.jsonl` has usage, so "newest file in the tree"
— which is right for Codex and Claude — picks the wrong one here.

`OUTPUT_SPEED_PROVIDER_IDS` in `lib/outputSpeed.ts` is now the single list; two
surfaces previously hardcoded `codex || claude` independently.

### Verification, real

- Frontend: `tsc --noEmit` clean, **37 files / 217 tests** passed, `pnpm build`
  succeeded, locale drift **748 keys** matched across Rust and TS.
- Tauri crate: **331 tests** passed.
- Shared Rust: **621 tests** passed.
- Font enumeration and weight continuity re-measured live (numbers above).

**Still no visual acceptance.** The five fixes above are visual by nature and
only the user can confirm them on screen. Items G and H likewise still have
automated coverage only.

### Removed on purpose

There is no `best_default_family()` in Rust. The enumerated list is already
sorted best-first and the settings page reads its first recommended entry — one
ranking, in one place. A second copy of the preference order would drift from
the one the user actually sees.

## Active checkpoint: TASK-QUOTA-PRESENTATION-TASKBAR-018 complete (2026-07-31)

Executor: Claude Code / claude-opus-5, sole writer. Branch `platform/windows`,
HEAD `b8529fb5`, uncommitted. Detail in `CURRENT_TASK.md`.

Items A, E, F, G and H are all implemented. Two extra fixes the user asked for
landed too: Grok's missing forecast, and macOS pace parity.

Item E was corrected on 2026-08-01: the Settings window frame now uses the tray
flyout's exact composition (no gutter, no border, no shadow, `border-radius` plus
`clip-path: inset(0 round ...)`). Tuning shadow parameters inside a fixed window
is a dead end — the window edge always clips the shadow, and the gutter exists
only to hold it. When a surface needs a frame, copy `.menu-surface--tray` rather
than inventing one.

Facts a later agent must not undo:

1. `taskbar_text` must use `D2D1_RENDER_TARGET_TYPE_SOFTWARE`. `DEFAULT` lets
   Direct2D create a D3D device, which blocks indefinitely in a process without
   a COM apartment.
2. All DirectWrite drawing assertions live in **one** test. libtest gives each
   test its own thread, `RENDERER` is thread-local, and creating a second
   single-threaded D2D factory on an apartment-less thread hangs.
3. Font enumeration is cached in `FAMILY_CACHE` — it opens a font face per font
   per family (~400 families); calling it per lookup is an O(n^2) hang.
4. **Taskbar windows are matched by declared length, never by slot**
   (`taskbar_entries::window_by_kind`). Providers disagree on which slot holds
   which cycle; reading `primary` positionally made Claude's 5-hour usage render
   as its weekly figure.
5. **An unresolvable taskbar entry never prints a number.** It carries an
   `EntryUnavailable` reason and the strip renders that. Do not "helpfully"
   default to 0%.
6. **The taskbar has no reset-time mode, on purpose** — its renderer shows no
   reset text. `ResetAwareComponent` makes asking for one a compile error.
7. **Non-quota rows never draw a percentage** — gate on
   `quotaDisplay.primaryQuotaState`, which catches informational rows *and* the
   synthetic 0% windows balance providers use as carriers.
8. **Each settings page writes only its own component's keys.**
   `ComponentIsolation.test.tsx` enforces it by clicking every toggle and reset
   on all three pages. A global control overriding three components is the exact
   thing item H removed.
9. The pace stage comes from shared Rust (`UsagePace::stage_for_delta`, +/-2/6/12).
   TS consumes it rather than re-deriving buckets — that drift once made "on
   pace" unreachable.
10. Grok's window length is derived from `resets_at` minus one calendar month,
    not a 30-day constant. A fixed constant misplaces the expected line by up to
    a day, and with no length at all `UsagePace` silently refuses for most of
    every month.

Measured evidence for item F (Bahnschrift, ink by `wght`): 200->901638,
400->1252545, 430->1263555, 560->1517802, 700->1685811, 900->1973607. The 430 and
560 values differ from the stops around them, which is what proves a real axis.
398 families installed, 16 with a weight axis.

Verification, real: frontend tsc clean, 37 files / 213 tests, `pnpm build`,
locale 742 keys; Tauri 325 tests; shared Rust 617 plus launcher;
`git diff --check` clean. One DPI-aware taskbar screenshot confirms the
DirectWrite strip renders live with grayscale antialiasing and no opaque backing.
**Items G and H have automated coverage only — no visual acceptance.** No commit
or push.

Known gaps carried forward:

- The historical/probabilistic layer (run-out probability, 5-hour
  session-equivalents, predictive notifications) is **specified in
  `docs/MACOS_WEEKLY_QUOTA_MODEL.md` but not built**. It needs a usage-sample
  store plus a median burn estimator; the other three features all depend on
  those two.
- Workday-aware pacing is deliberately not implemented — the user does not use
  the app only on workdays.
- `lib/providerBalance.ts` still hardcodes Chinese strings.
- `scripts/dev-windows.ps1` takes its exclusive lock **before**
  `Stop-TokenBarProcesses`, so `-StopOnly` cannot stop a running chain. This is
  the recurring "Another TokenBar Windows development launcher is already
  running" failure. One-line fix (move the lock after the early exits); left
  unapplied because the script is outside this task package.

## Active checkpoint: TASK-QUOTA-PRESENTATION-TASKBAR-018 phases 2/3, partial (2026-07-31)

Executor: Claude Code / claude-opus-5, sole writer. Branch `platform/windows`,
HEAD `b8529fb5`, uncommitted. Full detail in `CURRENT_TASK.md`.

The user asked for phases 2 and 3 in one pass and chose the real DirectWrite
route for item F, authorizing the `windows` crate as a new dependency.

**Landed:** item A (weekly quota and its forecast are now one block, with the
expected-position marker on the quota bar itself) and a proven DirectWrite
renderer (`taskbar_text.rs`).

**Also landed:** item F is wired end to end. `taskbar_widget.rs` paints through
`taskbar_text`, keeping GDI only as a fallback. The weight setting is a real
100..=1000 `wght` axis value (the three-stop snapping is gone), there is a new
`taskbar_widget_font_family` setting, and `get_taskbar_font_families` populates
the picker from the live `IDWriteFontCollection` with a per-family flag for
whether the weight axis is genuine.

A DPI-aware capture of the real taskbar shows the strip rendering
`Codex 16%` / `周额度 0%` through DirectWrite, with grayscale antialiasing, no
colour fringes and no opaque backing block. What is NOT done is the package's
required A/B across three weights and two families on the real taskbar — the
settings file is DPAPI-protected so those values cannot be scripted safely from
outside the app. That is a manual check and it is live-updating.

**Not landed:** items E, G, H.

Item A was corrected after user review: the first version only moved text and
its marker had no CSS, so nothing rendered. The pace position is now punched
through the quota bar with a mask and a coloured stripe, ported from macOS
`UsageProgressBar.swift`. When porting more of this card, read that file and
`UsagePaceText.swift` in `steipete/CodexBar` first — the user treats the macOS
app as the reference design.

Facts a later agent must not undo:

1. `taskbar_text` must use `D2D1_RENDER_TARGET_TYPE_SOFTWARE`. `DEFAULT` makes
   Direct2D create a D3D device, which blocks indefinitely in a process without a
   COM apartment and buys nothing for a two-line text strip on a GDI DC.
2. All DirectWrite drawing assertions live in **one** test on purpose. libtest
   runs each test on its own thread, `RENDERER` is thread-local, and creating a
   second single-threaded D2D factory on an apartment-less thread hangs.
   Splitting them re-introduces a hang that looks like a product bug and is not.
3. Font enumeration is cached in `FAMILY_CACHE`. It opens a font face per font
   per family (~400 families); calling it per lookup is an O(n²) hang.
4. `quotaForecastDisplay` derives a projection from `windowMinutes` + `resetsAt`
   when the provider has no pace block. Requiring `provider.pace` makes windows
   with perfectly good timing report "not enough data".
5. The weekly forecast attaches to the weekly row only, via `weeklyMetricId`.
   The bridge picks the weekly window by length, not by slot; binding the
   forecast to `primary` is the old bug where Claude's 5-hour usage read as
   weekly pace.

Measured evidence for item F (Bahnschrift, ink by `wght`): 200→901638,
400→1252545, 430→1263555, 560→1517802, 700→1685811, 900→1973607. The 430 and 560
values differ from the named stops around them, which is what proves a real axis
rather than four static faces. 398 families installed, 16 with a weight axis.

Verification, real: frontend tsc clean, 35 files / 205 tests, `pnpm build`,
locale 715 keys; Tauri 317 tests; shared Rust 614 plus launcher;
`git diff --check` exit 0. One DPI-aware taskbar screenshot as described above;
no other visual acceptance claimed. No commit or push.

Environment defect, reported not fixed: `scripts/dev-windows.ps1` acquires its
exclusive lock before `Stop-TokenBarProcesses`, so `-StopOnly` cannot stop a
running chain. This is the recurring "Another TokenBar Windows development
launcher is already running" failure. Fix is to move the lock after the
`-StopOnly`/`-DryRun` early exits; left for the user to authorize since the
script is outside this task package.

## Active checkpoint: TASK-QUOTA-PRESENTATION-TASKBAR-018 phase 1 (2026-07-30)

Executor: Claude Code / claude-opus-5, sole writer. Branch `platform/windows`,
HEAD `b8529fb5`, work uncommitted.

The user divided `docs/CLAUDE_TASK_PACKAGE_018.md` into three phases and asked
to accept each one before the next begins. Phase 1 is implemented, fully tested,
and independent re-review is now **Conditional Go** after both P1 findings were
fixed in code (not deferred) — see `CODE_REVIEW.md` for the review text and the
executor response, and `CURRENT_TASK.md` for what changed. User acceptance is
still required; phases 2 and 3 have not been started. The user also decided item F must take the real DirectWrite variable-weight route
rather than settling for discrete GDI stops — that work belongs to phase 3.

Phase 1 delivered: the inventory and migration table
(`docs/QUOTA_PRESENTATION_INVENTORY_018.md`), a shared quota presentation layer
(`apps/desktop-tauri/src/lib/quotaDisplay.ts`), items B/C/D data semantics, and
three genuinely independent per-component settings pairs. Full detail, including
the one open decision, is in `CURRENT_TASK.md`.

Six facts a later agent must not undo:

1. `Settings.show_as_used` and `Settings.reset_time_relative` are **migration
   sources only**. They stay deserializable so old `settings.json` files can seed
   the six new `float_bar_* / dashboard_* / taskbar_*` fields once. No display
   surface may read them again.
2. The `RawSettings` mirrors of those six fields are `Option<bool>` with a
   **field-level** `#[serde(default)]`. That is load-bearing: `RawSettings` has a
   struct-level `#[serde(default)]` whose fallback would otherwise yield
   `Some(true)` and make an explicitly stored `false` indistinguishable from an
   absent key.
3. Risk colour is graded from used-percent against the user's configured
   `highUsageThreshold` / `criticalUsageThreshold`, once, in
   `quotaDisplay.quotaLevel`. The previous hardcoded 25/5 remaining-percent
   cutoffs in `ProviderQuotaBlock` are gone and must not come back.
4. `ProviderQuotaBlock`, `MenuCard` and `ProviderGrid` take one context object,
   not loose booleans. Splitting it back into separate props reopens the
   "displayed semantics without matching thresholds" bug that item B exists to
   fix.
5. **The taskbar has no reset-time mode, on purpose.** Its native renderer shows
   no reset text and the task package's Taskbar page does not define one, so
   `taskbar_reset_time_relative` was removed rather than given a fabricated
   consumer. The type split (`QuotaPercentContext` vs `QuotaDisplayContext`,
   `ResetAwareComponent = "floatBar" | "dashboard"`) makes asking the taskbar for
   a reset mode a compile error, and
   `settings::tests::taskbar_has_no_reset_time_mode_setting` asserts the key is
   never persisted. Do not re-add it for symmetry. If the taskbar ever *should*
   show reset text, that is a phase-3 item-G change with screenshot acceptance.
6. **Non-quota rows must never draw a percentage.** `quotaDisplay.primaryQuotaState`
   is the single predicate; it catches both `isInformational` rows *and* the
   synthetic 0% windows balance providers use as carriers (checking only
   `isInformational` misses DeepSeek/MiMo). Any new compact surface that renders a
   percentage must gate on it.

Component boundary decision confirmed by the user (2026-07-30): the package
names three components while the code has four display surfaces. The tray flyout
and the PopOut dashboard intentionally share one `dashboard` component because
they render the same cards from the same snapshot. Do not split them or add a
fourth settings-key pair.

Verification after the P1 fixes, real results: `pnpm build` passed; locale parity
710 keys; frontend 35 files / 204 tests; shared Rust 613 tests plus launcher;
Tauri 314/314; both `cargo check`s; `git diff --check` exit 0. **No screenshot or
live runtime acceptance was performed and none is claimed.** User-visible effects
are limited to: the Display tab's two toggles now write the dashboard keys; the
Taskbar tab preview follows `taskbarShowAsUsed`; and providers with no percentage
quota show a muted marker in the provider grid instead of a 0% track. No commit
or push.

Known gaps carried forward: `lib/providerBalance.ts` still hardcodes Chinese
strings (`"余额"`, `"API 状态"`, `"暂不可用"`, `"含赠送 "`), which breaks the
package's terminology rule — queued for phase 2. `RateWindow::format_countdown()`
in shared Rust remains a second, unlocalized countdown used by Rust callers.

Independent phase-1 review gate (2026-07-30): both P1 findings closed. P1-1 was
resolved by **removing** the consumerless `taskbar_reset_time_relative` rather
than fabricating a consumer for it, and P1-2 by adding the shared
`primaryQuotaState` predicate and gating `ProviderGrid` on it. The P2 finding
(dashboard covering both the tray flyout and the PopOut panel) is deliberately
left open for the user. Details in `CODE_REVIEW.md` and `CURRENT_TASK.md`.

## Superseded checkpoint: TASK-QUOTA-PRESENTATION-TASKBAR-018 package creation (2026-07-30)

The user requested a bundled Claude Code task covering quota semantics and the
Windows taskbar status strip. The actionable contract is in
`docs/CLAUDE_TASK_PACKAGE_018.md`; it is the only implementation brief for
this round.

Required outcomes are: merge weekly usage and forecast into one coherent block;
keep one shared quota data/formatting layer while giving FloatBar, Dashboard
and Taskbar independent used-versus-remaining and relative-versus-absolute
reset settings;
show reset expiry exactly once per window; repair Settings shadow/radius;
replace the misleading taskbar font-weight promise with a genuinely verified
DirectWrite/variable-font path or honestly named effective choices; and allow
ordered multi-provider/multi-window taskbar entries (5-hour, weekly, daily,
monthly or provider-supported balance/credits).

The current code already contains `show_as_used`, `reset_time_relative`,
`RateWindowSnapshot`, `PaceSnapshot`, and a three-stop GDI font workaround.
These are starting points only. Do not duplicate formatting rules or invent
quota values for API-balance providers. Claude is the sole writer for this
task; Codex reviews and performs the final verification. No commit, push,
merge or unrelated cleanup is authorized.

Handoff status: package created, implementation pending. The detailed file
also defines migration, loading/error states, locale consistency, runtime
entry point, manual acceptance and completion criteria.

Execution gate confirmed by the user: three phases, stop after each phase for
Codex review and user acceptance. Phase 1 is the shared data/settings boundary;
Phase 2 is settings-page grouping, weekly quota/forecast merge and Settings
frame verification; Phase 3 is a real DirectWrite/Direct2D variable-weight
taskbar renderer plus taskbar content composition and final visual/interaction
acceptance. Claude must not cross a gate without an updated handoff report.

## Active checkpoint: TASK-TASKBAR-FONT-WEIGHT-EFFECTIVE-017 (2026-07-29)

The numeric 100–900 taskbar font-weight experiment has been corrected after a
live Win32 GDI probe. `CreateFontW` selected only Regular 400 or Bold 700 for
Microsoft YaHei UI and Segoe UI Variable across the tested numeric range. The
separately installed `Microsoft YaHei UI Light` family reports an actual weight
near 300, so it is now selected explicitly for the light position.

Settings therefore retains the requested slider interaction but exposes only
three effective stops: 300, 400 and 700. One shared Rust normalization function
is used by persisted-settings migration, Tauri updates and the native widget.
The frontend mirrors the same three values. Old named weights and arbitrary
numeric values remain loadable and are normalized on the next save. Do not
restore intermediate GDI values without replacing this renderer with a tested
DirectWrite variable-font implementation.

Verification passed: production build, 707-key locale parity, frontend 34/164,
shared Rust 608 plus launcher, Tauri 311/311 and `git diff --check`.
Normal-development PID was 49180 at handoff. No commit or push.

## Active checkpoint: TASK-TASKBAR-FONT-WEIGHT-SLIDER-016 (2026-07-29)

Taskbar font weight is now one numeric contract end to end. Settings presents a
100–900 range slider in 25-point steps with immediate preview and a visible
numeric readout. Pointer and keyboard interaction commit on completion rather
than writing settings for every intermediate pixel. The native status strip
receives the numeric value and passes it to GDI `CreateFontW`, then repaints
live.

Shared settings deserialize both the new numeric form and legacy named values.
Legacy normal/medium/semibold/bold migrate to 400/500/600/700; all numeric
input is clamped to 100–900 and is saved back as a number. Keep this migration
path unless a formal settings-version migration supersedes it. The current
renderer uses Microsoft YaHei UI through GDI, so nearby values may map to the
same available Windows font face; exact continuous variable-font rendering
would require a later DirectWrite renderer replacement.

Verification passed: production build, 707-key locale parity, frontend 34/164,
shared Rust 607 plus launcher, Tauri 311/311 and `git diff --check`. The one
App routing test that failed during an overloaded parallel run passed both
alone and in the clean full rerun. Normal-development PID was 47088 at handoff.
No commit or push.

## Active checkpoint: TASK-SETTINGS-WINDOW-FRAME-015 (2026-07-29)

The detached Settings window now matches the flyout's outer-window model:
transparent native WebView plus a frontend-owned shell. Both surfaces consume
the single `--window-radius: 24px` token. Settings adds a 12px transparent
gutter, theme-aware hairline border and visible drop shadow, then clips its
title bar and body to the shared radius.

On Windows, `settings_window.rs` creates the WebView with transparency and an
alpha background. `force_borderless_transparent_resizable` disables square
native non-client painting while retaining `WS_THICKFRAME`, so edge resizing
remains available. Do not switch Settings back to
`force_dark_caption_resizable`: that restores the opaque rectangular backing
which caused this task.

Verification passed: TypeScript, production build, 708-key locale parity,
frontend 34/164, Tauri 311/311 and `git diff --check`. The stale proof test for
the removed `settings:apiKeys` route was updated to `settings:menuBar`.
Normal-development PID was 51804 at handoff. No commit or push.

## Active checkpoint: TASK-SETTINGS-FUNCTIONAL-LAYOUT-014 (2026-07-29)

Settings were regrouped around user-facing functions. The active tab order is
General, Providers, Notifications, Display, Taskbar, Advanced and About.
Display now contains theme, tray-panel/menu presentation and FloatBar
configuration. Taskbar is a dedicated page containing the native status strip
and notification-area icon settings.

The status-strip settings now include a live preview, enabled state, taskbar
position, content selection, width (96-240 px), font size (10-16 px), font
weight and left/center/right alignment. The new values persist through shared
Rust settings and the Tauri bridge, and update the live native widget without a
restart. Keep the internal surface id `menuBar` unless doing a deliberate
routing migration; its user-facing label is `TaskbarWidgetTab`. The proof
harness now accepts the actual current routes `notifications`, `menu` and
`menuBar`; obsolete `display`, `apiKeys` and `cookies` routes are rejected.

Verification passed: TypeScript check; locale parity at 708 keys; frontend 34
files / 164 tests; production build; Tauri 311 tests; shared Rust 605 tests plus
launcher test; both Rust checks; `git diff --check`. Automated visual inspection
was stopped by the user's physical Escape key. Do not claim screenshot-based
acceptance from this round; leave the normal supported dev runtime available
for manual review. The clean normal-development process was PID 18120 at
handoff. No commit or push was performed.

## Active checkpoint: TASK-WINDOW-PREWARM-013 (2026-07-29)

The Settings blank flash and ignored first tray click shared one cold-window
cause. Both detached WebViews are now prewarmed hidden during Tauri setup.
`AppState` tracks frontend readiness and pending reveal requests. The flyout
marks itself ready through its existing fixed-layout handshake; Settings adds
an equivalent handshake that runs only after the lazy Settings surface has
rendered. Ready windows show immediately;
early requests stay hidden and reveal once ready. The latest requested Settings
tab is retained across that cold-start boundary.

Verification: `pnpm build` passed (688 locale keys); full frontend tests passed;
`cargo check` passed; all 311 Tauri tests passed. The supported launcher was
cold-started and produced PID 25292 at the time of verification. Native window
enumeration confirmed `CodexBar Settings` and the flyout `CodexBar` existed
with `Visible=false` before interaction. No commit or push was performed.

## Active checkpoint: TASK-TASKBAR-WIDGET-NATIVE-009 (2026-07-29)

The current taskbar strip follows TrafficMonitor's native path. It creates a
native popup, calls `SetParent` into `Shell_TrayWnd`, changes the style to
`WS_CHILD`, converts screen geometry to taskbar-client coordinates, and
reasserts placement on a timer. Its GDI background samples the adjacent
Explorer taskbar surface so the full child remains hit-testable. A DPI-aware screenshot shows the two-line
Codex/weekly readout in the taskbar row. The native child also owns a Windows
context menu with panel, refresh, settings, and exit actions. The older
overlay-only and invisible-child notes below are historical checkpoints, not
the active implementation contract.

The taskbar strip is configured in its own Display-tab section. Live settings
cover placement, font weight (normal/medium/semibold/bold), and content
(usage/output speed/usage + speed); changes repaint the native widget without a
restart.

## Current checkpoint

- Date: 2026-07-29
- Task: `TASK-TASKBAR-WIDGET-OVERLAY-006` (taskbar widget rendering takeover)
- Branch: `platform/windows`
- Status: Settings toggle + persistence + destroy path work; taskbar strip now
  renders through an independent top-level overlay anchored over the taskbar.
  The old cross-process `SetParent` child path is retired. Uncommitted.
- Runtime: the supported Windows development chain is running from this
  checkout through `scripts/dev-windows.ps1` (PID may differ by the time you
  read this — restart via the script, do not assume the old PID is current)

## Delivered

- Rebuilt the tray flyout against `design/floatbar-reference.html`.
- Applied option D from `design/style-options.html`.
- Kept the native flyout fixed at 328 x 776 logical px.
- Consolidated production tray CSS into one authoritative section.
- Removed verified dead flyout reveal/hide helpers, stale state and obsolete
  tests.
- Removed the legacy main-window fallback that could render `TrayPanel`; only
  the dedicated Tauri `flyout` window may render it.
- Kept detailed, compact and minimal as intentional presentation variants over
  one shared data model.
- Disabled legacy CSS `zoom` inside the fixed-size flyout to prevent cropping.
- Unified active tray terminology and removed old key names from production.
- Restored the normal-development boundary: `scripts/dev-windows.ps1` now clears
  inherited `CODEXBAR_PROOF_MODE` by default, with `-ProofMode` required for
  screenshot automation. This keeps outside-click blur-dismiss and tray-toggle
  close actions active in the build handed to the user.
- Replaced the invisible cross-process taskbar child with a top-level
  `WS_EX_TOOLWINDOW | WS_EX_NOACTIVATE | WS_EX_TOPMOST` overlay. It uses the
  existing taskbar anchor math and remains connected to the persisted setting
  and live `set_lines` updates.

## Verification

- `pnpm build`: passed; locale parity 670 keys.
- Task-critical frontend tests: 50/50 passed.
- Full Rust workspace: 306/306 passed.
- Full frontend suite: 33 files / 162 tests passed. A provider-sidebar timing
  test failed once, then passed in isolation and in the full-suite rerun.
- `git diff --check`: passed.
- Real Windows capture: 410 x 970 physical px at 125% DPI, corresponding to
  328 x 776 logical px; no clipping.
- Runtime self-check: the prior process emitted proof-mode suppression logs;
  it was stopped and restarted through the normal launcher. The new process
  has no proof-mode suppression entries, so normal outside-click dismissal and
  tray-button toggling are enabled.
- Close-path regression check: `TrayPanel.test.tsx` passed 27/27, including
  Escape dismissal and dashboard-open-then-dismiss behavior.

## Boundaries

- Do not commit, push, merge or discard unrelated dirty-worktree changes without
  explicit user authorization.
- Do not reintroduce `TrayPanel` into the main window.
- Do not add another late CSS override block; edit the single tray authority.
- Keep using `scripts/dev-windows.ps1` as the only interactive development
  entry.
- Native user-controlled flyout scaling remains a separate future task.
- Real screenshots for compact/minimal/dark remain a user-acceptance item;
  automated coverage exists, but this was not a UI change in the current fix.

## Next action

The user can inspect the running development version. If accepted, the next task
is an intentional commit/push of the reviewed scope.

### 第十轮：卡片放大 1.2 倍 + 浮窗磨砂背板（Claude Code 执行）

用户选定尺寸档位 **×1.20**，并要求实现磨砂背板、Win10 需可兜底。

**一、卡片 1.2 倍**

引入单一缩放变量，卡片内 121 处尺寸改写为 `calc(N * var(--flyout-u))`：

```css
--flyout-scale: 1.2;
--flyout-u: calc(var(--flyout-scale) * 1px);
```

N 保持参考稿在 scale 1 下推导出的原值，**倍数只存在于一处**——`ref NN` 溯源不受影响，换档只改一个数。`k_type` 由 10/17 提到 12/17（最小字 10→12px，主数字 32→38.4px）。

不参与缩放的两类，已写进块头注释：provider 切换条（共用外壳，围绕固定 17px 品牌图标构建）、所有 1px 发丝线（1.2px 会糊）。

脚本按属性白名单转换，排除 `line-height` / `letter-spacing` / `font-weight`，并跳过 `999px` 药丸哨兵值与既有 `calc()`（后者手工改写）。styles.css 是**混合行尾**（6863 CRLF + 595 LF），脚本按字节保留各行原始结尾——否则会产生 2500 行噪声 diff 淹没真实改动。

**二、磨砂背板**

新增 `shell/backdrop.rs`。**未使用**文档化的 `DWMWA_SYSTEMBACKDROP_TYPE`，理由写在模块头：该 API 把材质画在 DWM **框架**里，需 `DwmExtendFrameIntoClientArea` 才能进客户区，而浮窗的 `force_borderless_transparent_fixed` 恰好抹平非客户区、关闭 NC 渲染、且从不扩展框架；把形状交还 DWM 也不可行——微软明确 per-pixel alpha 窗口**永远无法**被 DWM 圆角化，且 `DWMWCP_ROUND` 固定 8px。

改用 `SetWindowCompositionAttribute` + `ACCENT_ENABLE_ACRYLICBLURBEHIND`：作用于整个窗口而非框架，**dwm.rs 现有四处设置一个未动**，圆角仍归我们所有。一条路同时覆盖 Win10 1803+ 与 Win11，无需分层。

代价：材质由窗口**区域**裁剪，`SetWindowRgn` 无抗锯齿，圆角有台阶。抗锯齿任意半径只存在于合成器层（`IDCompositionRectangleClip`），须从 WebView2 手里接管合成，不在本轮范围。

降级策略 `decide()` 为纯函数、7 个单测覆盖：API 缺失 / 用户关闭透明效果 / 高对比度（无障碍硬要求）/ 省电模式 / 远程桌面，各自独立触发纯色。记录的是**实际结果**而非策略推导——未公开 API 可能拒绝，页面必须跟随现实。

**三、真机结论（Win11 26200）**

`tier=acrylic composition_api_available: true accent_applied=true` —— 该未公开 API 在此版本**仍然可用**，这是方案最大的未知项，已排除。

**四、用户截图反馈后的修正**

1. **圆角太小**：Codex 本轮将外壳圆角由 20px 降至 10px。恢复 20px，并与 `FLYOUT_CORNER_RADIUS_DIP` 双向注释绑定。
2. **方形阴影框**：根因不是圆角，是区域裁剪从未生效——原先放在 reveal 里，而 reveal 是**每次打开仅一次且可被整体跳过**的握手。移到 `reanchor`（每次打开必经，也是跨显示器缩放变化的时机）。
3. **白面板加透明度**：亚克力生效时 `--flyout-zone-bg` 改为 0.86 alpha；叠加外壳自身 0.72 后实际背衬约 0.96，正文对比度基本维持纯色档。纯色回退时仍为实心。
   —— 此条中文原句（「白色框我感觉也不要加一些不透明度」）存在两种相反解读，已按「让白面板半透明」实现并向用户明示，待其确认。

**验证**：`cargo check` 通过；Tauri 313 测试通过（含 7 个新增）；前端 `pnpm build` + 连续两次 33 文件 / 162 测试全绿。

**如实记录**：中途一次前端跑出 1 个失败，紧接两次连续全绿，与本项目既有 flake 同型，未做掩盖也未视为已修。截图由**用户提供**——我尝试自动点击托盘图标数轮未成功，Ctrl+Shift+U 打开的是主窗口而非浮窗。

**交回状态**：开发链以**正常模式**（无 proof）运行，供用户自行验收。未提交、未推送。

### 第十一轮：撤销磨砂，回到纯色 + 30px 圆角

用户决定**不要磨砂**，背板改为接近面板的白色，圆角 30px。整套亚克力代码已删除，不保留未调用的原生实现。

**保留下来的实测结论**（代码已删，结论不能丢）：

1. `SetWindowCompositionAttribute` + `ACCENT_ENABLE_ACRYLICBLURBEHIND` 在 **Win11 build 26200 上仍然可用**（`accent_applied=true`）。该未公开 API 未被移除。
2. **`SetWindowRgn` 裁不掉亚克力。** DWM 按窗口**矩形**合成材质，无视窗口区域。已用 `GetWindowRgn=3`（COMPLEXREGION）证明区域确实设上了，而毛玻璃四角依旧是直角。
3. 唯一能裁材质的是 **`DWMWA_WINDOW_CORNER_PREFERENCE`**，且它在这个 per-pixel alpha 窗口上**确实生效**——微软文档说这类窗口可能被拒，实测未被拒（依据是屏幕表现，不是 `hr=0x0`；那个返回码成败都是 S_OK）。
4. 但该属性只有 `DWMWCP_ROUND`（约 8px）与 `ROUNDSMALL`（约 4px）两档，**不可配置也不可读回**。
5. 因此结论：**磨砂与自定义圆角在本技术栈上互斥**。同时保留二者只能走合成器层的 `IDCompositionRectangleClip`，需从 WebView2 接管窗口合成。
6. 过程中出现过「两层圆角」，根因是一个窗口上同时挂了三个半径不同的形状（`SetWindowRgn` 20px、CSS `clip-path` 20px、DWM 8px）。

**本轮改动**：删除 `shell/backdrop.rs`、`get_flyout_backdrop` 命令及其注册、`lib/tauri.ts` 的 `getFlyoutBackdrop`、TrayPanel 的 `data-backdrop` effect 与测试 mock、styles.css 的整个 acrylic 块（2295 字符）。

浅色背板 `#eeeef0` → **`#f7f7f8`**，与面板 `#ffffff` 仅差约 3%，分层改由发丝线与投影承担而非色阶。深色未动（用户看的是浅色，不擅自外推）。`--flyout-radius` 20px → **30px**。

**我在本轮之前的两处误判，记录备查**：
- 判断「方形框是区域裁剪没跑」——错，`GetWindowRgn` 证明一直在跑。
- 上一版方案的核心卖点「走 accent 路线可保住 20px 圆角」——该前提自始不成立，是方案本身的错误，导致用户多花数轮才看到真实取舍。

**验证**：`cargo check` 无警告；Tauri **306** 测试通过（原 313，减去随模块删除的 7 个 backdrop 单测）；前端 33 文件 / 162 测试通过；`pnpm build` 通过。未提交。

### 第十二轮：视觉微调 + 托盘百分比重做

**浮窗微调**（用户逐条截图反馈）：圆角 30→24px；浅色背板 `#f7f7f8`→`#fbfbfc`（与面板 `#ffffff` 仅差约 1.6%，分层全靠发丝线与投影）；卡片内 provider 图标 22→18px（参考稿那个 45px 是**带底芯片**，内含图形仅 26px，我们渲染裸图标却按芯片尺寸缩放，故重了约 40%）；近 7 天使用行改为只显示 `≈ 11.2亿`。

**切换条改为参与缩放。** 之前以「共用外壳」为由排除，该理由不成立——它只在浮窗内出现。1.2 倍下卡片大了 20% 而它没动，图标相对面板偏小。块头注释已同步修正。

**两个渲染 bug**：`.provider-grid__item` 圆角 5px 是为 24px 格子写死的，放大后显紧，改为 `calc(8 * var(--flyout-u))`（同心关系＝内圆角＋间距）；`.menu-card__name` 末位字母被裁，根因是**负字距在最后一个字形之后也扣一次**，内容盒比字形实际绘制短 0.04em，叠加 `overflow: hidden` 即削边，补 `padding-inline-end: 0.08em`。

**托盘百分比重做**（`rust/src/tray/render.rs`）。看不清的根因：`45%` 是**三个字形**，`text.len() >= 3` 把 scale 压到 2，每个数字仅 6×10px，32px 图标再被 Windows 降采样到 16px。

- **去掉 `%`**：只显示百分比的指示器上它不携带信息，却占三分之一宽度。两位数由此从 scale 2 升到 4。
- **尺寸按可用空间计算**，同时受宽高约束取小值，不再写死。
- **加深色描边**：填充色按用量等级取，浅色任务栏上浅色档会糊；任务栏跟随系统主题，本进程不可控。与「用超」橙色同一教训——**填充色不等于可读色**。

删除随之死掉的 `draw_glyph`。新增两个测试：非透明像素**包围盒**（两位数须 ≥24×18，任何缩小数字的改动都会挂）、描边像素存在性。

**任务栏额度条：未新建功能。** 项目已有 FloatBar 且带 `taskbar` 样式与 `topmost_guard`，设置页已暴露。已告知用户开启方式，并说明它是**盖在任务栏上方的独立窗口**而非 TrafficMonitor 那样 `SetParent` 真嵌入，自动隐藏／多显示器场景行为不同。

**验证**：shared crate 605 测试通过；前端 33 文件 / 162 测试通过；`pnpm build` 通过。二进制 mtime 22:18:24 晚于 render.rs 22:18:13，确认运行中的是新构建。

**如实记录**：本轮某次前端测试出现 1 次失败且未捕获到详情，随后连续 5 次全绿。与既有 flake 同型但**未能确认是同一个**，不作为已修处理。

### 第十三轮：任务栏用量条 P0（未落盘记录，补记）

用户要求"托盘图标太小看不清，能不能做一个像左下角频率网速那样的额度显示"，明确排除 FloatBar 方案（"不是 floatbar，是托盘处这块地"）。新增 `apps/desktop-tauri/src-tauri/src/taskbar_widget.rs`：`SetParent` 进 `Shell_TrayWnd`，GDI `DrawTextW` 画两行文字，仿照 TrafficMonitor 的路数。落地方式是**一次性后台启动**（`CODEXBAR_TASKBAR_WIDGET=1` 环境变量 + 单次 `powershell.exe ... dev-windows.ps1`），日志证实 `hwnd=0x2cd0e82 parent=0x10290 rect=(444,0,165,60)`，z-order 位于 `Shell_TrayWnd` 子窗口第 0 位，命中测试在 x=421 与 x=470 均返回 `CodexBarTaskbarWidget`。当时判定为"成功"，但**这次落地从未写入本文件**，也没有任何持久化开关——那次进程一结束，用量条就随之消失。

### 第十四轮：任务栏用量条改为持久化设置（本轮）

用户复查时的反馈是"我没有谈到托盘区有新增什么，设置里面也应该有按钮和设置吧"。先查证根因：复查时**开发链根本没有在运行**（无 `codexbar-desktop-tauri.exe` 进程），上一轮的验证只覆盖了那次一次性后台启动，从未做成可以持续存在的功能。

**改动**：

1. `rust/src/settings.rs` / `rust/src/settings/raw.rs` 新增 `taskbar_widget_enabled: bool`（默认 `false`），走 `RawSettings` 双向映射，向后兼容旧 `settings.json`（`#[serde(default)]`）。
2. `taskbar_widget.rs` 补齐生命周期：此前只有 `start()`（创建），**没有销毁路径**。新增 `install()`（启动时读设置，历史行为等价于把环境变量判断换成持久化设置）、`set_enabled(bool)`（设置页实时开关调用）、`stop()`（`DestroyWindow` + 清空 `WIDGET_HWND`）。
3. `commands/settings.rs` 的 `SettingsUpdate` 新增 `taskbar_widget_enabled: Option<bool>`，`update_settings` 保存后在 `#[cfg(windows)]` 下调用 `taskbar_widget::set_enabled`，路数与 floatbar 的 `SettingsPatch::apply` + `apply_state` 一致但更简单（单一布尔，无需专属模块）。
4. `commands/bridge.rs` 的 `SettingsSnapshot` 补充该字段，前端 `types/bridge.ts` 同步。
5. 设置页 `DisplayTab.tsx` 的"菜单栏"分组内新增开关（`TaskbarWidgetLabel` / `TaskbarWidgetHelper`），6 语言 `.ftl` + `locale.rs` + `keys.ts` 全部补齐，`check-locale-drift.mjs` 672 keys 通过。
6. `paint()` 的填充色不再是硬编码探测灰（`0x202020`），改为读注册表 `HKCU\...\Themes\Personalize\SystemUsesLightTheme`（Explorer 自己用来判断任务栏明暗的同一个值），深色任务栏用近黑底白字、浅色任务栏用浅灰底近黑字——填充色由我们自己选定，因此文字对比度是可控的，不依赖去猜测真实任务栏材质颜色。

**未做（明确标注）**：没有改动用量条在任务栏上的**位置**（仍然锚定在 `ReBarWindow32` 左侧，即任务按钮区左边、开始按钮右边一带），因为这是上一轮就选定的锚点，且用户这次的反馈是"看不到"而非"位置不对"——但用户原话"托盘处这块地"如果指的是右侧系统托盘/时钟区（`TrayNotifyWnd`），两者是完全不同的窗口且当前实现并未覆盖，需要用户确认后再决定是否改锚点。

**验证**：`cargo check` 干净；Tauri 306+1 新增测试通过；shared crate settings 58 测试、locale 14 测试（含 `test_all_locale_keys_have_all_languages`）通过；`pnpm build` 与 vitest 33 文件 / 162 测试通过；`node apps/desktop-tauri/scripts/check-locale-drift.mjs` 672 keys 匹配。

**如实记录 —— 自动化验证的边界**：尝试用 `Ctrl+Shift+U`（全局快捷键）+ `EnumWindows` 按 PID 枚举确认主窗口（`CodexBar Desktop`，class `Tauri Window`）真的打开了，这一步可信。但随后用 `GetWindowRect` + `CopyFromScreen` 截取该窗口区域时，截出来的内容是**完全不相关的网页广告内容**，说明发起截图调用的 PowerShell 进程与目标窗口之间存在 DPI 坐标系不一致，坐标换算不可信。判定为环境已知的自动化局限（与此前"自动点击托盘/浮窗多轮失败"同类），**当场放弃**并删除了那张误截的图，没有以它作为任何视觉证据。因此：设置页开关的**真实点击路径**（开→创建窗口，关→销毁窗口）没有做端到端可视验证，只验证到单元测试与代码走查层面。开发链已重启并保持运行，交由用户自行在 Settings 里打开这个新开关验证。

### 第十五轮：任务栏用量条完全不显示——深挖后仍未解决，移交 Codex

用户自己在设置里打开了新开关后反馈：**"打开任务栏用量条，任务栏并没有看到"**。这说明设置持久化和开关本身是好的（用户能找到、能点、状态能生效），问题完全在用量条**本体渲染**上。本轮做了大量排查，逐条如实记录，**没有解决，问题原样移交**。

**关键工具，先记下来**：诊断这个问题必须先解决截图本身不准的问题。用非 DPI-aware 的 PowerShell 进程调用 `GetWindowRect`/`CopyFromScreen` 会拿到按 96 DPI 虚拟化过的坐标，`CopyFromScreen` 却按物理像素取屏，两者对不上，第一次这么截图截到的是完全不相关的另一个窗口内容（一堆网页广告），一度误判为"合成器把内容盖住了"。正确做法：先调用一次
`SetThreadDpiAwarenessContext(DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2)`（值为 `-4`）
再做 `GetWindowRect`/`CopyFromScreen`，坐标才是物理像素、可信。后续所有测量都用了这个前提。

**排查时间线（四次独立尝试，全部有真实证据支撑，全部没解决）**：

1. **原方案（贴 `ReBarWindow32` 左侧）**：截图放大发现那块区域显示的是网速/CPU/内存等**不相关内容**，宽度明显超出该窗口自己的 hwnd 尺寸（该窗口本身只有 191px 宽，但可见内容铺满了约 570px）。判断是本机已运行的第三方任务栏工具 TrafficMonitor **直接把文字画到屏幕对应位置**，覆盖范围比自己的窗口大得多，每次刷新都会把我们的内容盖掉。
2. **改到 `ReBarWindow32` 右侧、`TrayNotifyWnd` 左侧之间**：这块空间理论上没人占，但截图显示那个区域仍然显示着任务栏图标。测量发现 `ReBarWindow32` 的 hwnd 矩形（前后两次测量分别是右边界 1610px 和 1693px，物理像素）会随窗口数量变化而变化，且**跟图标视觉位置对不上**——Windows 11 的图标是由 `Windows.UI.Composition.DesktopWindowContentBridge`（覆盖整个任务栏宽度的 XAML 合成层）负责渲染，不严格依赖 `ReBarWindow32` 这个legacy 兼容窗口的真实边界，所以"rebar 右边就是空的"这个假设不成立。
3. **改成直接贴 `TrayNotifyWnd` 左边**：这个边界在两次独立测量中都稳定在同一个值（物理像素 2017），比 `ReBarWindow32` 可靠得多。放大截图确认这次目标区域**确实是空白任务栏背景**，没有被别的东西占用——但我们自己的内容也完全没有显示，连填充色都看不到，纯粹是任务栏本身的渐变色。这排除了"位置选错、被别的东西盖住"的可能性，问题变成了"内容根本没有被合成到屏幕上"。
4. **加诊断日志确认 `WM_PAINT` 是否真的执行**：在 `paint()` 里加了一行 `tracing::info!`，重新编译重启后，日志证实 `WM_PAINT` **每次都可靠触发**，`BeginPaint` 返回的 `hdc` 也都是有效值（例如 `0x6401159a`、`0xffffffffc10117a9`——后者看起来奇怪，实际上是 64 位系统上 GDI 句柄常见的高位符号扩展写法，不是错误）。也就是说 `FillRect`/`DrawTextW` 确实执行了，画到了一个有效的设备上下文里，但画出来的像素从未出现在屏幕上。
5. **对照开源项目 TrafficMonitor 的真实实现**（github.com/zhongyang219/TrafficMonitor）：
   - `Win11TaskbarDlg.cpp::AdjustTaskbarWndPos` 的定位逻辑和我们第 3 步收敛到的方案**几乎一样**——都是以 `TrayNotifyWnd` 的左边界为基准（`notify_x_pos - m_rect.Width() + 2`），说明定位思路本身是对的。
   - `TaskBarDlg.cpp` 的默认（不透明）渲染路径用的也是**同一套技术**：`SetParent` 挂进 `Shell_TrayWnd`，普通 `WM_PAINT` + `CPaintDC` 画图。`WS_EX_LAYERED` 只在用户主动设置"透明背景色"时才会加上（`ApplyWindowTransparentColor()`，仅用于色键抠图/透明合成），默认情况完全不需要。
   - 但发现一个可能有意义的差异：TrafficMonitor 是先把窗口当成**独立顶层窗口**创建（`OnInitDialog` 里正常 `Create()`），然后才**单独调用一次 `SetParent`** 把它挂进任务栏（`m_connot_insert_to_task_bar = !(::SetParent(this->m_hWnd, GetParentHwnd()));`）。而我们原来的实现是在 `CreateWindowExW` **创建的那一刻**就直接把 `Shell_TrayWnd` 传成 parent。于是照着这个顺序改了一版：先用 `WS_POPUP`、parent=0 创建独立窗口，再用 `SetWindowLongPtrW` 把 style 换成 `WS_CHILD`，再 `SetParent`，再 `SetWindowPos(..., SWP_FRAMECHANGED)`，再 `ShowWindow(SW_SHOW)`。重新编译、重启、重新截图验证——**还是完全不可见**，和之前一模一样。
   - 顺带记录：TrafficMonitor 自己判断"是否成功嵌入任务栏"的依据只是 `SetParent` 的返回值，跟我们遇到的情况（`SetParent`/`CreateWindowExW` 结构上都成功、`IsWindow`/`IsWindowVisible` 都为真、还能收到 `WM_PAINT`，但像素就是不出现在屏幕上）不是一回事——它的这个检测机制不会捕捉到我们碰到的这种失败模式，所以从它的源码里没能直接找到"为什么"。

**当前结论（如实记录，未解决）**：窗口创建、挂载、消息分发、绘图调用全部验证为正常执行且有真实证据（日志 + 截图），但绘制内容始终没有被合成到屏幕上，四种独立思路的修复尝试均未生效。怀疑与 DWM 在这台机器/这个 Windows 11 版本上，对挂在 `Shell_TrayWnd` 下的这一类子窗口的重定向表面（redirection surface）合成方式有关，但没有找到确切原因，也没有在 TrafficMonitor 数千行的 MFC 代码里找到能明确对应的那一处关键差异。

**尚未尝试、留给下一步的方向**：
- 不看 `TaskBarDlg.cpp` 默认路径，改为直接抄它"透明背景"分支的做法：无条件加 `WS_EX_LAYERED` 并配合 `UpdateLayeredWindow`/`SetLayeredWindowAttributes`，而不是依赖普通 `BeginPaint`/`EndPaint`。
- 检查我们的进程和 `explorer.exe` 之间是否存在 DPI-awareness 声明不一致（本轮测量脚本本身就因为这个问题走过弯路，不排除我们的 Tauri/WebView2 进程与 `explorer.exe` 的 DPI 感知级别不一致，导致跨进程 `SetParent` 之后 DWM 对子窗口的合成表面处理异常）。
- 翻 TrafficMonitor 在 GitHub 上关于 "Windows 11" "不显示" "任务栏" 的历史 issue/commit，找他们踩过的坑和具体修复提交，而不是只读当前主分支代码。
- 尝试 `RedrawWindow(hwnd, NULL, NULL, RDW_INVALIDATE | RDW_UPDATENOW | RDW_ERASE | RDW_ALLCHILDREN)` 替代/补充 `InvalidateRect`。

**用户决策未定**：给出三个选项（继续深挖 / 改用项目已有的 FloatBar 任务栏样式作为更稳的退路 / 放弃这个子功能只保留已经修好的托盘图标百分比显示），用户选择"无所谓"，随后要求把现状写清楚、交给 Codex 接手判断和继续，因此**没有替用户做最终选择**。

**当前遗留状态**：`apps/desktop-tauri/src-tauri/src/taskbar_widget.rs` 里的 `paint()` 保留着本轮加的 `tracing::info!("taskbar widget: paint hdc={hdc:#x}");` 诊断日志（未清理，供继续排查用）；`target_rect()` 当前锚点是"贴 `TrayNotifyWnd` 左侧"（第 3 步的版本）；`start()` 是"先建独立窗口再 `SetParent`"的版本（第 5 步的版本）。开发链仍在运行（`scripts/dev-windows.ps1` 启动的实例），具体 PID 会变，接手时以进程列表为准，不要假设本文档写的 PID 还活着。未提交、未推送。

### 第十六轮：Codex 接管任务栏状态条并改为可靠覆盖层（2026-07-29）

Claude 留下的四次 `SetParent` 嵌入尝试均已确认失败：窗口存在且绘制调用成功，但 Windows 11 的 XAML/DWM 合成链不显示跨进程子窗口像素。Codex 选择此前用户留出的独立窗口退路，重写 `taskbar_widget.rs` 的窗口生命周期：

1. 移除 `SetParent`、`WS_CHILD` 和跨进程子窗口路径。
2. 使用 `WS_POPUP | WS_EX_TOOLWINDOW | WS_EX_NOACTIVATE | WS_EX_TOPMOST` 创建独立窗口。
3. 复用 `TrayNotifyWnd` 左侧锚点和 DPI 换算，但将任务栏客户区坐标转换为屏幕坐标。
4. 继续用计时器重定位并重申置顶，保留设置开关、生命周期和 `set_lines` 数据通道。

验证：`cargo check` 通过；Tauri **307/307** 测试通过；开发链自动重编译并重新启动；日志确认 `mode=overlay` 与 `WM_PAINT`；在启用任务栏用量条的真实 Windows 会话中，以 `SetThreadDpiAwarenessContext(-4)` 进行 DPI 感知截图，确认任务栏区域实际显示 `Codex 3%` / `周额度 0%`。当前修改未提交、未推送。后续禁止把 `SetParent` 嵌入实现恢复为生产路径。

### 第十七轮：增加任务栏位置选项并收紧视觉样式（2026-07-29）

用户反馈覆盖层过于突兀，并要求设置中增加左侧位置。新增持久化字段
`taskbar_widget_position`，选项为“左侧”和“通知区左侧”；旧配置缺失时默认
“通知区左侧”。位置变化会通过 Settings bridge 立即调用原生 `set_position`，无需重启。

视觉上将整行矩形改为任务栏中部的 36 DIP 圆角胶囊，宽度保持 132 DIP，颜色按
Windows 任务栏明暗主题切换，字体缩小到 12 DIP。当前任务栏覆盖层仍是独立窗口，
Windows 的图标左对齐/居中设置不会改变这两个位置选项的含义。

验证：locale drift 676 keys；`pnpm build` 通过；全前端 33 文件 / 162 测试通过；
设置位置归一化测试已加入；开发链自动重编译并运行。当前修改未提交、未推送。
