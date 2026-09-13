import type { LocaleKey } from "../i18n/keys";
import type {
  PaceSnapshot,
  ProviderUsageSnapshot,
  RateWindowSnapshot,
  QuotaDisplayPreference,
  ResetDisplayPreference,
  SettingsSnapshot,
} from "../types/bridge";
import { getProviderBalance } from "./providerBalance";
import { getPaceChartSnapshot, getPaceEstimate } from "./paceBudget";

/**
 * One shared quota presentation layer (TASK-018).
 *
 * Every surface — floating bar, tray flyout, PopOut dashboard, native taskbar
 * strip — resolves what to show through this module, so the *data rules and
 * terminology* are identical everywhere even though each surface freely chooses
 * *which* windows and fields it displays.
 *
 * Two rules this module exists to enforce:
 *
 *  1. The number, the bar direction and the alarm colour always agree. The old
 *     code rendered `showAsUsed ? used : remaining` for the number but always
 *     graded the colour off `remainingPercent` against hardcoded 25/5 cutoffs,
 *     ignoring the user's configured thresholds. Risk is now graded once, from
 *     used-percent against the user's own high/critical thresholds, and the
 *     displayed number and fill both follow the chosen semantics — so a
 *     shrinking "remaining" bar turns red at exactly the point a growing "used"
 *     bar would.
 *  2. Nothing is invented. Missing forecasts, missing reset timestamps and
 *     balance-only providers each get an explicit state rather than a `0` or an
 *     em dash that reads like real data.
 */

// ── Component identity ───────────────────────────────────────────────

/**
 * The surfaces that own an independent presentation choice.
 *
 * `dashboard` covers both the tray flyout and the PopOut panel: they render the
 * same cards from the same snapshot, and TASK-018 defines three configurable
 * components, not four. If those two ever need to diverge, add a fourth id here
 * and a fourth pair of settings keys — do not overload this one.
 */
export type QuotaComponent = "floatBar" | "dashboard" | "taskbar";

/** Components that render reset times and therefore own a reset-time mode. */
export type ResetAwareComponent = QuotaComponent;

/**
 * What a component needs to render a percentage.
 *
 * Split out from the full context because the taskbar strip's body does not
 * print reset text. The taskbar reset preference is still resolved here for
 * the native tray/context-menu status row, so all three surfaces share the
 * same inheritance contract.
 */
export interface QuotaPercentContext {
  /** `true` → the number and bar mean "used"; `false` → "remaining". */
  showAsUsed: boolean;
  /** Used-percent at which the bar turns "high" (from settings). */
  highUsageThreshold: number;
  /** Used-percent at which the bar turns "critical" (from settings). */
  criticalUsageThreshold: number;
}

/** A percentage context plus the reset-time mode, for surfaces that show one. */
export interface QuotaDisplayContext extends QuotaPercentContext {
  /** `true` → countdown; `false` → absolute local wall-clock time. */
  resetTimeRelative: boolean;
}

const USED_KEYS = {
  floatBar: "floatBarShowAsUsed",
  dashboard: "dashboardShowAsUsed",
  taskbar: "taskbarShowAsUsed",
} as const satisfies Record<QuotaComponent, keyof SettingsSnapshot>;

const USAGE_MODE_KEYS = {
  floatBar: "floatBarQuotaDisplay",
  dashboard: "dashboardQuotaDisplay",
  taskbar: "taskbarQuotaDisplay",
} as const satisfies Record<QuotaComponent, keyof SettingsSnapshot>;

const RESET_KEYS = {
  floatBar: "floatBarResetTimeRelative",
  dashboard: "dashboardResetTimeRelative",
  taskbar: "taskbarResetTimeRelative",
} as const satisfies Record<ResetAwareComponent, keyof SettingsSnapshot>;

const RESET_MODE_KEYS = {
  floatBar: "floatBarResetDisplay",
  dashboard: "dashboardResetDisplay",
  taskbar: "taskbarResetDisplay",
} as const satisfies Record<ResetAwareComponent, keyof SettingsSnapshot>;

/** Return the persisted surface preference, defaulting old snapshots to a
 * concrete override so they keep their pre-inheritance behavior. */
export function quotaDisplayPreference(
  settings: SettingsSnapshot,
  component: QuotaComponent,
): QuotaDisplayPreference {
  const mode = settings[USAGE_MODE_KEYS[component]];
  if (mode === "follow" || mode === "used" || mode === "remaining") return mode;
  return settings[USED_KEYS[component]] === false ? "remaining" : "used";
}

/** Return the persisted reset preference, with the same old-snapshot fallback. */
export function resetDisplayPreference(
  settings: SettingsSnapshot,
  component: ResetAwareComponent,
): ResetDisplayPreference {
  const mode = settings[RESET_MODE_KEYS[component]];
  if (mode === "follow" || mode === "countdown" || mode === "absolute") return mode;
  return settings[RESET_KEYS[component]] === false ? "absolute" : "countdown";
}

/**
 * Read one component's percentage context out of the settings snapshot.
 *
 * Together with {@link quotaDisplayContext} this is the only place that maps a
 * component to its settings keys, so a surface cannot accidentally read another
 * surface's preference — the exact cross-contamination TASK-018 item H forbids.
 */
export function quotaPercentContext(
  settings: SettingsSnapshot,
  component: QuotaComponent,
): QuotaPercentContext {
  const preference = quotaDisplayPreference(settings, component);
  return {
    showAsUsed:
      preference === "follow"
        ? settings.showAsUsed ?? true
        : preference === "used",
    highUsageThreshold: normalizeThreshold(settings.highUsageThreshold, 70),
    criticalUsageThreshold: normalizeThreshold(settings.criticalUsageThreshold, 90),
  };
}

/** Read a reset-aware component's full presentation context. */
export function quotaDisplayContext(
  settings: SettingsSnapshot,
  component: ResetAwareComponent,
): QuotaDisplayContext {
  const preference = resetDisplayPreference(settings, component);
  return {
    ...quotaPercentContext(settings, component),
    resetTimeRelative:
      preference === "follow"
        ? settings.resetTimeRelative ?? true
        : preference === "countdown",
  };
}

function normalizeThreshold(value: number | undefined, fallback: number): number {
  return Number.isFinite(value) ? Math.min(100, Math.max(0, value as number)) : fallback;
}

// ── Data state ───────────────────────────────────────────────────────

/**
 * The lifecycle state of a piece of quota data.
 *
 * `unsupported` is not an error: it means this provider genuinely has no such
 * quota window (an API-balance account has no weekly percentage), and the UI
 * must say so instead of drawing an empty 0% bar.
 */
export type QuotaStatus = "loading" | "ready" | "stale" | "error" | "unsupported";

/** Snapshots older than this read as `stale` rather than live. */
export const STALE_AFTER_MS = 15 * 60 * 1000;

export function resolveProviderStatus(
  provider: Pick<ProviderUsageSnapshot, "error" | "updatedAt">,
  nowMs: number = Date.now(),
): QuotaStatus {
  if (provider.error) return "error";
  if (!provider.updatedAt) return "loading";
  const updatedMs = Date.parse(provider.updatedAt);
  if (!Number.isFinite(updatedMs)) return "loading";
  return nowMs - updatedMs > STALE_AFTER_MS ? "stale" : "ready";
}

// ── Percentage presentation ──────────────────────────────────────────

export type QuotaLevel = "normal" | "high" | "critical" | "exhausted";
export type QuotaSemantics = "used" | "remaining";

export interface QuotaPercentDisplay {
  /** What the number means, so callers pick the matching suffix label. */
  semantics: QuotaSemantics;
  /** Exact percentage in the chosen semantics, 0..100. */
  percent: number;
  /** `percent` rounded for text rendering. */
  rounded: number;
  /** Bar width 0..100, in the chosen semantics — always matches `percent`. */
  fillPercent: number;
  /** Risk grade. Identical in both semantics for the same underlying usage. */
  level: QuotaLevel;
  isExhausted: boolean;
  /** True for informational rows that must never render as a quota bar. */
  isInformational: boolean;
}

/**
 * Grade risk from used-percent against the user's configured thresholds.
 *
 * Kept independent of `showAsUsed` on purpose: consumption risk does not change
 * because the user prefers to read the remaining figure. Since the bar fill
 * follows the same semantics as the number, the colour and the bar always move
 * together in either mode.
 */
export function quotaLevel(
  usedPercent: number,
  ctx: Pick<QuotaPercentContext, "highUsageThreshold" | "criticalUsageThreshold">,
  isExhausted = false,
): QuotaLevel {
  if (isExhausted || usedPercent >= 100) return "exhausted";
  if (usedPercent >= ctx.criticalUsageThreshold) return "critical";
  if (usedPercent >= ctx.highUsageThreshold) return "high";
  return "normal";
}

export function quotaPercentDisplay(
  rate: RateWindowSnapshot,
  ctx: QuotaPercentContext,
): QuotaPercentDisplay {
  // Trust `usedPercent` as the single source and derive remaining from it. The
  // bridge sends both, but deriving here keeps the two mutually consistent even
  // if a provider reports a pair that does not add up to 100.
  const usedPercent = Number.isFinite(rate.usedPercent)
    ? Math.min(100, Math.max(0, rate.usedPercent))
    : 0;
  const remainingPercent = 100 - usedPercent;
  const percent = ctx.showAsUsed ? usedPercent : remainingPercent;

  return {
    semantics: ctx.showAsUsed ? "used" : "remaining",
    percent,
    rounded: Number(percent.toFixed(1)),
    fillPercent: percent,
    level: quotaLevel(usedPercent, ctx, rate.isExhausted),
    isExhausted: rate.isExhausted,
    isInformational: Boolean(rate.isInformational),
  };
}

/**
 * Whether a provider's primary window may be drawn as a percentage at all.
 *
 * `informational` covers two encodings that both look like a quota but are not:
 * a `RateWindow::informational` row (providers such as sub2api emit
 * "Subscription active" / "No quota data" this way) and the synthetic 0% windows
 * that balance providers use to smuggle a prepaid amount through
 * `resetDescription`. Both would otherwise render as a real "0% used" bar, which
 * TASK-018 forbids — a compact surface must show a non-quota state instead.
 */
export type PrimaryQuotaState = "quota" | "error" | "informational";

export function primaryQuotaState(
  provider: ProviderUsageSnapshot,
): PrimaryQuotaState {
  if (provider.error) return "error";
  if (provider.primary.isInformational) return "informational";
  // Balance providers keep a real quota window when the account has one; only
  // the windows this helper excludes are the synthetic carriers.
  return getProviderBalance(provider).excludeWindows.has("primary")
    ? "informational"
    : "quota";
}

/**
 * Locale key for the suffix that names what the percentage means.
 *
 * One mapping for every surface, so "used" and "remaining" are never worded
 * differently in two places (TASK-018 terminology consistency).
 */
export function quotaSemanticsLabelKey(semantics: QuotaSemantics): LocaleKey {
  return semantics === "used" ? "PanelUsedSuffix" : "PanelLeftSuffix";
}

// ── Reset time presentation ──────────────────────────────────────────

export type ResetDisplayKind =
  /** A live countdown; re-render at least once a minute. */
  | "relative"
  /** An absolute local wall-clock time. */
  | "absolute"
  /** The reset moment has passed and fresh data has not arrived. */
  | "expired"
  /** No timestamp; showing the provider's own words verbatim. */
  | "providerDescription"
  /** No timestamp and no description — nothing is known. */
  | "unknown";

export interface ResetDisplay {
  kind: ResetDisplayKind;
  /** Ready-to-render text. */
  text: string;
  /** True when the caller must keep a per-minute timer running. */
  ticking: boolean;
}

export interface ResetFormatOptions {
  resetsAt: string | null;
  /** The provider's own reset wording, used only when there is no timestamp. */
  resetDescription: string | null;
  relative: boolean;
  t: (key: LocaleKey) => string;
  nowMs?: number;
  /** Injectable for tests; defaults to the host timezone. */
  timeZone?: string;
  locale?: string;
}

/**
 * Resolve a window's reset display.
 *
 * A reset time is never inferred. With no parseable `resetsAt` the provider's
 * own description is passed through unchanged, and with neither we say the reset
 * time is unknown. An elapsed reset reads as "expired, waiting for refresh"
 * rather than a negative countdown or a "resetting now" that never resolves.
 */
export function formatResetDisplay(options: ResetFormatOptions): ResetDisplay {
  const { resetsAt, resetDescription, relative, t } = options;
  const nowMs = options.nowMs ?? Date.now();

  const target = resetsAt ? Date.parse(resetsAt) : Number.NaN;
  if (!Number.isFinite(target)) {
    const description = resetDescription?.trim();
    return description
      ? { kind: "providerDescription", text: description, ticking: false }
      : { kind: "unknown", text: t("QuotaResetUnknown"), ticking: false };
  }

  if (target <= nowMs) {
    return {
      kind: "expired",
      text: t("QuotaResetExpiredWaiting"),
      // Keep ticking: the display must flip back as soon as the window renews.
      ticking: true,
    };
  }

  if (relative) {
    const totalMinutes = Math.floor((target - nowMs) / 60_000);
    const days = Math.floor(totalMinutes / 1440);
    const hours = Math.floor((totalMinutes % 1440) / 60);
    const minutes = totalMinutes % 60;
    const text =
      days > 0
        ? t("ResetsInDaysHours").replace("{}", String(days)).replace("{}", String(hours))
        : t("ResetsInHoursMinutes")
            .replace("{}", String(hours))
            .replace("{}", String(minutes));
    return { kind: "relative", text, ticking: true };
  }

  return {
    kind: "absolute",
    text: formatAbsoluteReset(target, nowMs, options),
    // Absolute times still need a timer: crossing midnight turns "tomorrow"
    // into "today", and the window can expire while the view is open.
    ticking: true,
  };
}

/**
 * Absolute reset time in the user's local zone — which calendar day, and at
 * what time.
 *
 * Every branch names the day. An earlier version printed the time alone for a
 * same-day reset, on the theory that "today" is the common case and the header
 * row is narrow; in practice a bare "3:30 PM" does not say which day it falls
 * on. Today and tomorrow get words rather than a date, because "Aug 2" for
 * something four hours away reads like a distant deadline.
 *
 * No "Reset:" prefix. It was tried and dropped at the user's call: the string
 * only ever renders in a slot that already means "reset" — beside a quota
 * title, or in the floating bar's reset chip behind a reset icon — so the word
 * was pure repetition in the one place the row has no width to spare.
 */
function formatAbsoluteReset(
  targetMs: number,
  nowMs: number,
  options: Pick<
    ResetFormatOptions,
    "timeZone" | "locale" | "resetDescription" | "t"
  >,
): string {
  const { timeZone, locale, t } = options;
  const timeOnly: Intl.DateTimeFormatOptions = {
    hour: "numeric",
    minute: "2-digit",
    ...(timeZone ? { timeZone } : {}),
  };
  const withDate: Intl.DateTimeFormatOptions = {
    month: "short",
    day: "numeric",
    ...timeOnly,
  };

  try {
    const target = new Date(targetMs);
    const dayDelta = localDayIndex(targetMs, timeZone) - localDayIndex(nowMs, timeZone);
    if (dayDelta !== 0 && dayDelta !== 1) {
      return new Intl.DateTimeFormat(locale, withDate).format(target);
    }
    const time = new Intl.DateTimeFormat(locale, timeOnly).format(target);
    return fill(t(dayDelta === 0 ? "TodayAt" : "TomorrowAt"), time);
  } catch {
    // A bad timeZone/locale must not blank the row; fall back to the
    // provider's wording, then to the raw ISO-ish string.
    return options.resetDescription?.trim() || new Date(targetMs).toISOString();
  }
}

/**
 * Substitute a template's single `{}`.
 *
 * Appends when the placeholder is missing rather than returning the template
 * unchanged, so a translation that loses its `{}` degrades to "Reset: 3:30 PM"
 * instead of silently dropping the time it exists to carry.
 */
function fill(template: string, value: string): string {
  return template.includes("{}")
    ? template.replace("{}", value)
    : `${template} ${value}`.trim();
}

/**
 * The target's calendar day, as a day number in the display timezone.
 *
 * Formatting the y/m/d parts in that zone and re-packing them through
 * `Date.UTC` gives a day index that is stable across DST: subtracting two of
 * them counts calendar days, not 24-hour spans, which is what "today" and
 * "tomorrow" mean to a reader.
 */
function localDayIndex(ms: number, timeZone: string | undefined): number {
  const parts = new Intl.DateTimeFormat("en-US", {
    year: "numeric",
    month: "2-digit",
    day: "2-digit",
    ...(timeZone ? { timeZone } : {}),
  }).formatToParts(new Date(ms));
  const part = (type: Intl.DateTimeFormatPartTypes) =>
    Number(parts.find((p) => p.type === type)?.value);
  return Math.round(
    Date.UTC(part("year"), part("month") - 1, part("day")) / 86_400_000,
  );
}

// ── Forecast presentation ────────────────────────────────────────────

/**
 * A forecast is either usable or explicitly absent. `null` from the pace
 * helpers means "not enough data", and callers must render that as such rather
 * than substituting `0`, which would read as a real measurement of zero.
 */
export interface QuotaForecastDisplay {
  available: boolean;
  /** Projected end-of-window used-percent, or null when unavailable. */
  projectedUsedPercent: number | null;
  /** Signed lead/lag against an even burn rate, or null. */
  deltaPercent: number | null;
  /** True when the current rate lasts to the reset. Null when unknown. */
  lastsToReset: boolean | null;
  /** Seconds until exhaustion at the current rate, or null. */
  etaSeconds: number | null;
  /**
   * The pace stage as computed by shared Rust, when the provider supplied a
   * pace block. Preferred over re-deriving buckets from `deltaPercent` in TS:
   * the thresholds (+/-2, 6, 12) live in one place, `UsagePace::stage_for_delta`.
   * `null` for a forecast derived on the frontend from window timing alone.
   */
  stage: PaceSnapshot["stage"] | null;
  /** See `PaceSnapshot.speedMultiplierToReset`. */
  speedMultiplierToReset: number | null;
}

/**
 * Where to place the "expected by now" marker on a quota track.
 *
 * Mirrored into the displayed semantics, so a bar showing *remaining* puts the
 * marker at `100 - expected`. Without this the marker would sit on the opposite
 * side of the bar from the figure it annotates — the same used/remaining
 * mismatch item B exists to prevent, just expressed geometrically.
 *
 * Returns `null` when there is no forecast, so callers render no marker rather
 * than one parked at 0%.
 */
export function forecastMarkerPercent(
  forecast: QuotaForecastDisplay,
  ctx: Pick<QuotaPercentContext, "showAsUsed">,
): number | null {
  if (!forecast.available || forecast.projectedUsedPercent == null) return null;
  const expected = Math.min(100, Math.max(0, forecast.projectedUsedPercent));
  return ctx.showAsUsed ? expected : 100 - expected;
}

export const FORECAST_UNAVAILABLE: QuotaForecastDisplay = {
  stage: null,
  speedMultiplierToReset: null,
  available: false,
  projectedUsedPercent: null,
  deltaPercent: null,
  lastsToReset: null,
  etaSeconds: null,
};

/**
 * Normalize a bridge `PaceSnapshot` into a forecast display model.
 *
 * Returns the unavailable model for a missing pace, for informational rows, and
 * for windows with no real quota — a projection over a synthetic 0% window is
 * fabricated data, which item A.4 rules out.
 */
export function quotaForecastDisplay(
  pace: ProviderUsageSnapshot["pace"],
  rate: RateWindowSnapshot | null,
): QuotaForecastDisplay {
  // A projection needs a real percentage quota with a window and a reset
  // boundary. Balance carriers and informational rows have neither a meaningful
  // "used so far" nor an elapsed fraction, so projecting over them would invent
  // the number outright.
  if (!rate || rate.isInformational) return FORECAST_UNAVAILABLE;
  if (rate.windowMinutes == null || !rate.resetsAt) return FORECAST_UNAVAILABLE;

  if (pace) {
    if (!Number.isFinite(pace.expectedUsedPercent) || !Number.isFinite(pace.deltaPercent)) {
      return FORECAST_UNAVAILABLE;
    }
    return {
      available: true,
      projectedUsedPercent: pace.expectedUsedPercent,
      deltaPercent: pace.deltaPercent,
      lastsToReset: pace.willLastToReset,
      etaSeconds: Number.isFinite(pace.etaSeconds as number) ? pace.etaSeconds : null,
      stage: pace.stage,
      speedMultiplierToReset: Number.isFinite(
        pace.speedMultiplierToReset as number,
      )
        ? pace.speedMultiplierToReset
        : null,
    };
  }

  // Not every provider's bridge snapshot carries a pace block, but a window that
  // knows its length and its reset time already contains everything a linear
  // projection needs: how far through the window we are, and how much has been
  // spent. Deriving it here is what lets the merged weekly block work for those
  // providers instead of falsely reporting "not enough data".
  const chart = getPaceChartSnapshot(rate);
  const estimate = getPaceEstimate(rate);
  if (!chart || !estimate) return FORECAST_UNAVAILABLE;

  return {
    available: true,
    projectedUsedPercent: chart.elapsedPercent,
    deltaPercent: chart.usedPercent - chart.elapsedPercent,
    lastsToReset: estimate.lastsUntilReset,
    etaSeconds: estimate.hoursRemaining * 3600,
    // Derived on the frontend, so there is no Rust-computed stage to trust.
    stage: null,
    speedMultiplierToReset: null,
  };
}
