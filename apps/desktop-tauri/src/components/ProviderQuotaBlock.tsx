import type { ReactNode } from "react";
import { useFormattedResetTime } from "../hooks/useFormattedResetTime";
import type { LocaleKey } from "../i18n/keys";
import type { RateWindowSnapshot } from "../types/bridge";

type UsageLevel = "normal" | "high" | "critical" | "exhausted";
const WEEKLY_WINDOW_MINUTES = 7 * 24 * 60;

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
  if (rate.windowMinutes != null && rate.windowMinutes >= WEEKLY_WINDOW_MINUTES) {
    return t("ProviderWeeklyLabel");
  }
  const normalized = raw?.trim().toLowerCase();
  if (normalized === "weekly") return t("ProviderWeeklyLabel");
  if (normalized === "session" || normalized === "session (5h)") {
    return t("ProviderSessionLabel");
  }
  return raw?.trim() || t("ProviderSessionLabel");
}

function levelOf(remainingPercent: number, exhausted: boolean): UsageLevel {
  if (exhausted) return "exhausted";
  if (remainingPercent <= 5) return "critical";
  if (remainingPercent <= 25) return "high";
  return "normal";
}

export function ProviderQuotaBlock({
  title,
  rate,
  resetTimeRelative,
  showAsUsed,
  usedLabel,
  remainingLabel,
  exhaustedLabel,
  hero = false,
  planLabel,
  children,
}: {
  title: string;
  rate: RateWindowSnapshot;
  resetTimeRelative: boolean;
  showAsUsed: boolean;
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
  children?: ReactNode;
}) {
  const usedPercent = Number.isFinite(rate.usedPercent)
    ? Math.max(0, rate.usedPercent)
    : 0;
  const remainingPercent = Math.max(0, 100 - usedPercent);
  const displayPercent = showAsUsed ? usedPercent : remainingPercent;
  const fillPercent = Math.min(100, showAsUsed ? usedPercent : remainingPercent);
  const resetText = useFormattedResetTime(
    rate.resetsAt,
    rate.resetDescription,
    resetTimeRelative,
  );
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
  const percentText = (
    <span
      className="provider-quota__percent"
      data-exhausted={rate.isExhausted || undefined}
    >
      {Math.round(displayPercent)}% {showAsUsed ? usedLabel : remainingLabel}
    </span>
  );

  return (
    <div className={`provider-quota${hero ? " provider-quota--hero" : ""}`}>
      {/* Every window shares the same header: quota label on the left, reset
          time on the right. */}
      <div className="provider-quota__header">
        {hero ? (
          <span className="provider-quota__hero-label">{title}</span>
        ) : (
          <span className="provider-quota__title">{title}</span>
        )}
        {resetText && <span className="provider-quota__reset">{resetText}</span>}
      </div>
      {hero && (
        <div
          className="provider-quota__hero-pct"
          data-exhausted={rate.isExhausted || undefined}
        >
          {Math.round(displayPercent)}%
        </div>
      )}
      <div className="provider-quota__track">
        <div
          className="provider-quota__fill"
          data-level={levelOf(remainingPercent, rate.isExhausted)}
          style={{ width: `${fillPercent}%` }}
        />
        {hero && planLabel && (
          /* Two identically-positioned copies of the label: the base copy is
             accent-colored (readable on the empty track); the second copy is
             white and lives inside a clip layer whose width tracks the fill,
             so exactly the covered part of the text renders white. */
          <>
            <span className="provider-quota__fill-label">✦ {planLabel}</span>
            <div
              className="provider-quota__fill-label-clip"
              style={{ width: `${fillPercent}%` }}
              aria-hidden
            >
              <span className="provider-quota__fill-label">✦ {planLabel}</span>
            </div>
          </>
        )}
      </div>
      {/* Secondary windows keep one meta row under the bar carrying the
          percentage (right-aligned); the hero's percentage is the big
          number above the bar, so it has no meta row. */}
      {!hero && <div className="provider-quota__meta">{percentText}</div>}
      {rate.isExhausted && (
        <div className="provider-quota__exhausted">{exhaustedLabel}</div>
      )}
      {children}
    </div>
  );
}
