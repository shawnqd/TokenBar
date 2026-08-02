# design/

Static visual references for the Windows tray flyout. These are **specs, not
build inputs** — nothing here is imported by the app. They exist so a styling
decision can be reviewed at the flyout's real size before it is written into
`apps/desktop-tauri/src/styles.css`.

Open them directly in a browser.

## Files

| File | What it is |
| --- | --- |
| `floatbar-reference.html` | The original design spec: the three density tiers (详细 / 紧凑 / 极简) as a 660px presentation card. The source of truth for **structure and proportion** — every `ref NN` comment in the "Reference adaptation" block of `styles.css` points back to a value in this file. |
| `pace-copy-options.html` | Wording candidates for the weekly pace line and the runway line, rendered at flyout size in both states. Settled on **A4 + B2**. |
| `pace-color-options.html` | Colour candidates for the 「用超」 text, with measured contrast ratios against the real panel background. Settled on **O4** (`#a04000` light / `#ffa552` dark). |
| `icon-options.html` | Icon-set candidates for the five insight rows. Settled per-row (**S2/S3/S2/S2/S2**): speedometer, stacked layers, clock, check-in-circle, exclamation-in-circle. |
| `style-options.html` | Style **directions** for the detailed card — surface and colour only, with layout/type/copy frozen. Six variants (A 现状 / B 品牌暖调 / C 数据高亮 / D 分层立体 / E 强调导轨 / F 渐变主块) in both themes, each with its measured contrast. Settled on **D 分层立体**; the page carries the full implementation spec (token deltas + four constraints) in its 「已选定」 block, which is the authority for that change. |

## Why these render at 328px

The flyout is 328 logical px wide. `floatbar-reference.html` is drawn at 660px,
so a straight copy of its pixel values does not transfer. Two scales bridge the
gap, both documented in `styles.css`:

- **geometry** (padding, radii, bar heights, dividers) — `k_geo = 328 / 660 ≈ 0.497`
- **type** (every font-size) — `k_type = 10 / 17 ≈ 0.588`, anchored so the
  reference's smallest text lands on a readable 10px

Font weights are copied verbatim and never scaled.

## Keeping them useful

When a styling question comes up, prefer editing the relevant page here and
reviewing it first — it is much cheaper than a rebuild-and-screenshot cycle,
and it captures the reasoning next to the options. Record the chosen option in
this table so the next change starts from the decision, not from scratch.

Panel background values used by these pages (composited, not eyeballed):

- light — `#e9e9eb` (`--app-bg: #f5f5f7` under `--surface-elevated: rgba(0,0,0,.05)`)
- dark — `#272729` (`--app-bg: #1c1c1e` under `--surface-elevated: rgba(255,255,255,.05)`)
