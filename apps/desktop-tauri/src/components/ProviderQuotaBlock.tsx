import type { CSSProperties, ReactNode } from "react";
import { useResetDisplay } from "../hooks/useFormattedResetTime";
import type { LocaleKey } from "../i18n/keys";
import {
  quotaPercentDisplay,
  type QuotaDisplayContext,
} from "../lib/quotaDisplay";
import type { RateWindowSnapshot } from "../types/bridge";

export function isMeaningfulQuotaWindow(rate: RateWindowSnapshot): boolean {
  if (rate.isInformational) return Boolean(rate.resetDescription);
  return !(
    rate.usedPercent === 0 &&
    rate.windowMinutes == null &&
    rate.resetsAt == null &&
    !rate.resetDescription &&
    !rate.isExhausted
  );
}

export function quotaWindowLabel(
  raw: string | undefined,
  rate: RateWindowSnapshot,
  t: (key: LocaleKey) => string,
): string {
  // `kind` is decided once in Rust (`quota_cycle.rs`). This used to test
  // `windowMinutes >= 7 days`, an open-ended bound with no upper limit, so a
  // MONTHLY window was labelled "Weekly" here while the taskbar strip called the
  // same window "月".
  if (rate.kind === "weekly") {
    return t("ProviderWeeklyLabel");
  }
  const normalized = raw?.trim().toLowerCase();
  if (normalized === "weekly") return t("ProviderWeeklyLabel");
  if (normalized === "session" || normalized === "session (5h)") {
    return t("ProviderSessionLabel");
  }
  return raw?.trim() || t("ProviderSessionLabel");
}

export function ProviderQuotaBlock({
  title,
  rate,
  display,
  usedLabel,
  remainingLabel,
  exhaustedLabel,
  hero = false,
  planLabel,
  markerPercent = null,
  paceState = "on-pace",
  expectedLabel = null,
  children,
}: {
  title: string;
  rate: RateWindowSnapshot;
  /**
   * The owning component's presentation choice, from
   * `quotaDisplayContext(settings, component)`. Passed as one object rather
   * than loose booleans so a caller cannot supply the displayed semantics while
   * forgetting the thresholds that colour it.
   */
  display: QuotaDisplayContext;
  usedLabel: string;
  remainingLabel: string;
  exhaustedLabel: string;
  /** The card's headline quota (first visible metric) — adds a large
   * standalone percentage above the header and thickens/gradients the bar,
   * matching the dashboard-style hero card. Secondary windows stay plain. */
  hero?: boolean;
  /** Plan/tier name (e.g. "ChatGPT Plus"), embedded inside the fill —
   * hero only, ignored for secondary windows. */
  planLabel?: string | null;
  /**
   * "Expected by now" position on this window's own track, already mirrored into
   * the displayed semantics by `forecastMarkerPercent`. Supplied for the weekly
   * window so the forecast annotates the quota bar the card already draws,
   * instead of a second duplicate weekly bar further down (item A).
   */
  markerPercent?: number | null;
  /**
   * Whether the window is under- or over-spent for how far it has elapsed.
   * Colours the pace stripe: green for reserve, red for deficit — the same
   * mapping the macOS bar uses.
   */
  paceState?: "reserve" | "deficit" | "on-pace";
  /** Tooltip for the marker, e.g. the localized "expected" label. */
  expectedLabel?: string | null;
  children?: ReactNode;
}) {
  const percent = quotaPercentDisplay(rate, display);
  // Convert the pace position from a fraction of the TRACK to a fraction of the
  // FILL, since the fill is what carries the mask. Beyond the fill there is
  // nothing to interrupt.
  const notchInFill =
    markerPercent != null &&
    percent.fillPercent > 0 &&
    markerPercent <= percent.fillPercent
      ? (markerPercent / percent.fillPercent) * 100
      : null;
  const reset = useResetDisplay(
    rate.resetsAt,
    rate.resetDescription,
    display.resetTimeRelative,
  );
  const resetText = reset.text;
  if (rate.isInformational) {
    return (
      <div className="provider-quota provider-quota--informational">
        <div className="provider-quota__header">
          <span className="provider-quota__title">{title}</span>
        </div>
        {resetText && <div className="provider-quota__info">{resetText}</div>}
        {children}
      </div>
    );
  }
  const roundedPercent = percent.rounded;
  const suffixLabel = percent.semantics === "used" ? usedLabel : remainingLabel;

  return (
    <div className={`provider-quota${hero ? " provider-quota--hero" : ""}`}>
      {/* Header: quota label only. Every window then shows its percentage as a
          left-aligned number just below the title — the hero large, secondary
          windows a size down — so the figures line up on the same side. */}
      {/* Header: quota label on the left, reset countdown on the right. */}
      <div className="provider-quota__header">
        {hero ? (
          <span className="provider-quota__hero-label">{title}</span>
        ) : (
          <span className="provider-quota__title">{title}</span>
        )}
        {/* The window's reset information appears here and nowhere else in the
            block — item D forbids repeating it below the bar or in a forecast
            row. `data-reset-state` lets an expired window be styled distinctly
            without a second copy of the text. */}
        {resetText && (
          <span className="provider-quota__reset" data-reset-state={reset.kind}>
            {resetText}
          </span>
        )}
      </div>
      {hero ? (
        <div
          className="provider-quota__hero-pct"
          data-exhausted={rate.isExhausted || undefined}
        >
          {roundedPercent}%
        </div>
      ) : (
        <div
          className="provider-quota__sub-pct"
          data-exhausted={rate.isExhausted || undefined}
        >
          <span className="provider-quota__sub-pct-num">{roundedPercent}%</span>
          <span className="provider-quota__sub-pct-suffix">{suffixLabel}</span>
        </div>
      )}
      {/* The pace position is cut through the track+fill by a mask and a single
          coloured stripe is drawn in the gap — see `.provider-quota__track[data-pace]`.
          `data-pace` gates the mask so a window with no forecast keeps a plain
          unbroken bar rather than a notch parked at 0%. */}
      <div
        className="provider-quota__bar"
        style={
          markerPercent != null
            ? ({ "--pace-x": `${markerPercent.toFixed(1)}%` } as CSSProperties)
            : undefined
        }
      >
      <div
        className="provider-quota__track"
        data-pace={markerPercent != null ? paceState : undefined}
      >
        {/* The notch is expressed relative to the FILL's own box, because that is
            what the mask resolves against. When the pace position sits beyond
            the fill there is nothing to cut — the track is already bare there —
            so the mask is omitted entirely. */}
        <div
          className="provider-quota__fill"
          data-level={percent.level}
          data-notch={notchInFill != null ? "" : undefined}
          style={
            {
              width: `${percent.fillPercent}%`,
              ...(notchInFill != null
                ? { "--pace-fill-x": `${notchInFill.toFixed(2)}%` }
                : {}),
            } as CSSProperties
          }
        />
        {/* Notch first, then the stripe on top. Both sit INSIDE the track so its
            overflow:hidden clips them to the rounded rail, the same way the
            macOS canvas clips to the bar rect. */}
        {markerPercent != null && (
          <span
            className="provider-quota__pace"
            data-pace-state={paceState}
            title={expectedLabel ?? undefined}
            aria-hidden
          />
        )}
        {hero && planLabel && (
          /* Two identically-positioned copies of the label: the base copy is
             accent-colored (readable on the empty track); the second copy is
             white and lives inside a clip layer whose width tracks the fill,
             so exactly the covered part of the text renders white. */
          <>
            <span className="provider-quota__fill-label">✦ {planLabel}</span>
            <div
              className="provider-quota__fill-label-clip"
              style={{ width: `${percent.fillPercent}%` }}
              aria-hidden
            >
              <span className="provider-quota__fill-label">✦ {planLabel}</span>
            </div>
          </>
        )}
      </div>
      </div>
      {rate.isExhausted && (
        <div className="provider-quota__exhausted">{exhaustedLabel}</div>
      )}
      {children}
    </div>
  );
}
