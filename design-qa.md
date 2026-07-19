# Local Usage Card Design QA

- Source visual truth: `C:\Users\13701\AppData\Local\Temp\codex-clipboard-7950f22a-50be-45e7-a33e-b020a8373224.png`
- Implementation screenshot: `C:\Users\13701\Documents\Man Worker\TokenBar\design\qa\local-usage-implementation-final.png`
- Combined focused comparison: `C:\Users\13701\Documents\Man Worker\TokenBar\design\qa\local-usage-comparison.png`
- Viewport: 329 x 824 logical pixels
- State: Windows tray panel, light theme, Codex card scrolled to the local usage section; live local data loaded

## Full-view comparison evidence

The implementation screenshot verifies the redesigned section inside the real fixed-height tray panel. The fixed provider switcher and footer remain visible while the card body scrolls. The local usage card stays within the existing rounded, elevated panel system and does not alter adjacent quota, forecast, output-speed, or Claude sections.

## Focused region comparison evidence

The combined comparison verifies the reference hierarchy directly against the implemented card: period label first, full Token count as the dominant line, and estimated API value as the secondary line. A focused comparison was required because those typography and wrapping details are too small to judge reliably in the full tray screenshot.

## Findings

- Fonts and typography: passed. The Token count is the strongest element, the unit is subordinate, and the estimate uses the existing secondary-text style. The empty state remains explicit instead of displaying a dash.
- Spacing and layout rhythm: passed. Today and 30-day periods are separated by one divider and use the same vertical rhythm. The section follows the current card padding and radius tokens.
- Colors and visual tokens: passed. Existing light/dark theme variables are used; no new hard-coded surface or text colors were introduced.
- Image quality and asset fidelity: not applicable. The reference and implementation contain no image assets or non-standard icons in this section.
- Copy and content: passed. “最新令牌” was removed, today and 30-day usage are named accurately, USD and approximate CNY values share one line, and the estimate disclaimer is shortened.
- Interaction: passed. Scrolling the real tray panel exposes the complete section while the fixed header and footer remain available.

## Comparison history

1. First pass: P2 at the 329-pixel tray width. The API-equivalent line reached the right edge and risked clipping.
   - Fix: reduced only the tray-specific estimate text to 11px and enabled safe wrapping without changing the desktop card scale.
   - Post-fix evidence: `design\qa\local-usage-implementation-final.png` and `design\qa\local-usage-comparison.png` show the full USD and CNY values inside the card.
2. Second pass: no actionable P0, P1, or P2 differences remained. The extra 30-day block, top-model line, and local-estimate note are intentional product data not represented in the compact visual reference.

## Implementation checklist

- [x] Replace the four-cell cost/token grid with period-based hierarchy.
- [x] Use the existing daily token total as “today usage”.
- [x] Show full Token counts with grouping separators.
- [x] Show USD and approximate CNY API value together.
- [x] Add explicit empty states and loading skeletons.
- [x] Verify the narrow tray viewport and localized labels.

final result: passed
