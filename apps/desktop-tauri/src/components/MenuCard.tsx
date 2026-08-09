import { Fragment, useCallback, useEffect, useState } from "react";
import type {
  DailyCostPoint,
  Language,
  LocalUsagePeriod,
  MenuBarDisplayMode,
  PaceSnapshot,
  ProviderChartData,
  ProviderLocalUsageSummary,
  ProviderOutputSpeed,
  ProviderUsageSnapshot,
  RateWindowSnapshot,
} from "../types/bridge";
import { BarChart } from "./charts/BarChart";
import { getProviderChartData } from "../lib/tauri";
import { useLocale } from "../hooks/useLocale";
import { useFormattedResetTime } from "../hooks/useFormattedResetTime";
import {
  FORECAST_UNAVAILABLE,
  forecastMarkerPercent,
  quotaForecastDisplay,
  quotaPercentDisplay,
  type QuotaDisplayContext,
  type QuotaForecastDisplay,
} from "../lib/quotaDisplay";
import { formatRelativeUpdated } from "../lib/relativeTime";
import type { LocaleKey } from "../i18n/keys";
import { paceCategory } from "../surfaces/tray/paceCategory";
import { SimpleBarChart, StackedBarChart } from "./MiniBarChart";
import { providerSupportsChartData } from "../lib/providerCharts";
import { getPaceEstimate } from "../lib/paceBudget";
import { dashboardShowsQuotaWindow } from "../lib/dashboardProviders";
import { getProviderBalance } from "../lib/providerBalance";
import { ProviderBalanceBlock } from "./ProviderBalanceBlock";
import { ProviderIcon } from "./providers/ProviderIcon";
import {
  isMeaningfulQuotaWindow,
  ProviderQuotaBlock,
  quotaWindowLabel,
} from "./ProviderQuotaBlock";

/**
 * Card line icons. One family: open stroked paths on a 24×24 grid with
 * `fill: none; stroke-width: 1.8; stroke-linecap/linejoin: round`, colour
 * always inherited from the surrounding row via `currentColor`.
 *
 * These replaced the glyphs lifted from design/floatbar-reference.html's
 * sprite, whose semantics did not survive the move — most visibly its
 * `#icon-support` (a headphone set, meaning customer support) had been
 * standing in for "够用到重置". Candidates and the chosen set are recorded in
 * design/icon-options.html. Each name below describes what it DRAWS, so a
 * future swap does not leave a `BoltIcon` rendering a speedometer.
 */
const paceIconProps = {
  width: 13,
  height: 13,
  viewBox: "0 0 24 24",
  fill: "none",
  stroke: "currentColor",
  strokeWidth: 1.8,
  strokeLinecap: "round" as const,
  strokeLinejoin: "round" as const,
  "aria-hidden": true,
};
/** Speedometer — output speed (t/s). */
const SpeedIcon = () => (
  <svg {...paceIconProps}>
    <path d="M4.5 17.5a8.5 8.5 0 1 1 15 0" />
    <path d="M12 15.5l4-4.5" />
  </svg>
);
/** Stacked layers — accumulated token usage. */
const UsageIcon = () => (
  <svg {...paceIconProps}>
    <path d="M12 3l8.5 4.5L12 12 3.5 7.5 12 3z" />
    <path d="M3.5 12.5L12 17l8.5-4.5" />
  </svg>
);
/** The enclosing ring shared by PaceIcon / CheckIcon / WarnIcon.
 *
 *  Was hand-written as two arcs, `M12 21a9 9 0 1 0 0-18 9 9 0 0 0 0 18z`. Its
 *  chord is exactly the diameter, which is the degenerate case for the
 *  large-arc flag (both halves are the same semicircle), and the closing `z`
 *  adds a zero-length segment that still takes a line join. A `<circle>` states
 *  the same shape from an exact centre and radius, with no degenerate input for
 *  the rasteriser to resolve. */
const RingPath = () => <circle cx="12" cy="12" r="9" />;
/** Clock — the weekly pace line compares elapsed TIME against usage, so a
 *  time glyph reads truer here than a generic rising arrow. */
const PaceIcon = () => (
  <svg {...paceIconProps}>
    <RingPath />
    <path d="M12 7.5V12l3.5 2" />
  </svg>
);
/** Rising arrow — kept only for the legacy card's pace STAGE badge, where the
 *  glyph marks a direction rather than the pace concept itself. */
const TrendIcon = () => (
  <svg {...paceIconProps}>
    <path d="M4 16l5-5 4 3 7-7" />
    <path d="M15 7h5v5" />
  </svg>
);
/** Check in a circle — quota lasts to reset. */
const CheckIcon = () => (
  <svg {...paceIconProps}>
    <RingPath />
    <path d="M8.5 12.3l2.4 2.4 4.6-5" />
  </svg>
);
/** Exclamation in a circle — quota is tight. Shares the circle with
 *  CheckIcon so the healthy and warning states read as one pair. */
const WarnIcon = () => (
  <svg {...paceIconProps}>
    <RingPath />
    <path d="M12 7.8v5" />
    <circle cx="12" cy="16.3" r="0.55" fill="currentColor" stroke="none" />
  </svg>
);

/** Small copy-to-clipboard button matching macOS CopyIconButton (doc.on.doc → checkmark). */
function CopyIconButton({ text }: { text: string }) {
  const { t } = useLocale();
  const [copied, setCopied] = useState(false);
  const handleCopy = useCallback(() => {
    navigator.clipboard.writeText(text).catch(() => {});
    setCopied(true);
    setTimeout(() => setCopied(false), 900);
  }, [text]);
  return (
    <button
      type="button"
      className="menu-card__copy-btn"
      onClick={handleCopy}
      aria-label={copied ? t("PanelCopied") : t("ActionCopyError")}
      title={copied ? t("PanelCopied") : t("ActionCopyError")}
    >
      {copied ? "✓" : (
        <svg width="12" height="12" viewBox="0 0 16 16" fill="none" xmlns="http://www.w3.org/2000/svg">
          <rect x="5" y="5" width="9" height="9" rx="1.5" stroke="currentColor" strokeWidth="1.5"/>
          <path d="M11 3V2.5A1.5 1.5 0 009.5 1H2.5A1.5 1.5 0 001 2.5v7A1.5 1.5 0 002.5 11H3" stroke="currentColor" strokeWidth="1.5"/>
        </svg>
      )}
    </button>
  );
}

interface MenuCardProps {
  provider: ProviderUsageSnapshot;
  /**
   * The owning surface's quota presentation choice, from
   * `quotaDisplayContext(settings, component)`. The card never reads settings
   * itself, so the same card renders correctly for the dashboard, the settings
   * preview, or any future surface with its own preference.
   */
  display: QuotaDisplayContext;
  compactMetrics?: boolean;
  /** Most recent completed-response speed for Codex or Claude. */
  outputSpeed?: ProviderOutputSpeed | null;
  /** Which period the local-usage stats block leads with (Settings-driven). */
  localUsagePeriod?: LocalUsagePeriod;
  /** Suppress the local-usage stats block entirely (e.g. compact display mode). */
  hideLocalUsage?: boolean;
  /** Show the provider brand icon in the card header. Governed by the
   * "show provider icons" setting on the tray; defaults on elsewhere. */
  showProviderIcon?: boolean;
  /**
   * Tray-flyout-only density tier ("detailed" | "compact" | "minimal").
   * When set, the card renders the density-aware two-zone layout (see
   * `styles.css` `.menu-surface--tray .menu-card__zone*`) instead of the
   * legacy content below. Leave unset (the default) for every other call
   * site (Settings preview card, PopOut dashboard) — their rendering is
   * governed entirely by `compactMetrics`/`hideLocalUsage` as before and
   * must not change when this prop is introduced.
   */
  densityMode?: MenuBarDisplayMode;
  /**
   * Which quota-window cycles to render, from the owning surface's settings.
   * Undefined or empty shows every window, which is what the card did before
   * this prop existed.
   *
   * Passed only by the two dashboard surfaces. The Settings preview card and
   * the provider detail pane deliberately do not filter: they are there to show
   * what a provider *reports*, and hiding half of it behind the dashboard's
   * display preference would make them useless for diagnosis.
   */
  quotaWindows?: string[];
}

const SPARK_W = 68;
const SPARK_H = 26;

/** Polyline path over a series, normalized to the actual min/max with a little
 * vertical padding so a near-flat series sits centered instead of hugging an
 * edge. Returns null when there aren't enough points to draw a line. */
function buildSparklinePath(values: number[]): string | null {
  if (values.length < 2) return null;
  const min = Math.min(...values);
  const max = Math.max(...values);
  const pad = 3;
  const span = max - min;
  const x = (i: number) => (i / (values.length - 1)) * SPARK_W;
  const y = (v: number) =>
    span === 0
      ? SPARK_H / 2
      : SPARK_H - pad - ((v - min) / span) * (SPARK_H - pad * 2);
  return values.map((v, i) => `${i === 0 ? "M" : "L"}${x(i).toFixed(1)} ${y(v).toFixed(1)}`).join(" ");
}

function OutputSpeedHighlight({
  speed,
  t,
}: {
  speed: ProviderOutputSpeed;
  t: (key: LocaleKey) => string;
}) {
  if (speed.tokensPerSecond == null || speed.tokensPerSecond <= 0) return null;
  // Oldest → newest so the line reads left-to-right in time order.
  const series = [...(speed.recentSamples ?? [])]
    .sort((a, b) => a.completedAtMs - b.completedAtMs)
    .map((s) => s.tokensPerSecond)
    .filter((v) => Number.isFinite(v) && v > 0);
  const sparkPath = buildSparklinePath(series);
  return (
    <div className="menu-card__speed" data-status={speed.status}>
      <div>
        <span className="menu-card__speed-label">{t("OutputSpeedTitle")}</span>
        <div>
          <span className="menu-card__speed-value">{speed.tokensPerSecond.toFixed(1)}</span>
          <span className="menu-card__speed-unit">t/s</span>
        </div>
      </div>
      {sparkPath && (
        <svg
          className="menu-card__speed-spark"
          viewBox={`0 0 ${SPARK_W} ${SPARK_H}`}
          width={SPARK_W}
          height={SPARK_H}
          fill="none"
          aria-hidden="true"
        >
          <path d={sparkPath} stroke="var(--accent)" strokeWidth="1.75" strokeLinecap="round" strokeLinejoin="round" />
        </svg>
      )}
    </div>
  );
}

/**
 * The tray flyout and the startup dashboard render raw provider errors through
 * MenuCard, independently of the Provider settings detail pane. Translate the
 * stable error categories here while leaving the original diagnostic available
 * through the Copy button.
 */
function localizeProviderError(message: string, t: (key: LocaleKey) => string): string {
  const lower = message.toLowerCase();
  // Say *why*, not *where*. This used to print the failed URL and nothing
  // else, which was the least useful half of the error and — on Claude — put
  // the account's organization id on screen. The reason now survives from Rust
  // (see `describe_network_error`), and a timeout or a refused connection is
  // what actually tells the user whether to retry or to check the network.
  const network = message.match(/^network error:\s*(.+)$/is);
  if (network) {
    const detail = network[1].trim();
    if (/^timeout:/i.test(detail)) return t("ProviderIssueNetworkTimeout");
    if (/^connect:/i.test(detail)) return t("ProviderIssueNetworkConnectionFailed");
    // Anything else: name it as a request failure and append the cause chain
    // with the URL stripped, so the message stays diagnostic without carrying
    // an account identifier. The unedited original is still on the Copy button.
    const withoutUrl = detail.replace(/\s*\(https?:\/\/[^\s)]+\)/g, "").trim();
    return withoutUrl
      ? t("ProviderIssueNetworkRequestFailed") + "：" + withoutUrl
      : t("ProviderIssueNetworkRequestFailed");
  }
  if (lower.startsWith("network error:")) {
    return t("ProviderIssueNetworkConnectionFailed");
  }
  if (
    lower.includes("oauth credentials not found") ||
    lower.includes("sign-in was not found") ||
    lower.includes("sign-in expired") ||
    lower === "authentication required"
  ) {
    return t("ProviderIssueSignInRequired");
  }
  return message;
}

/** Format a reserve description from raw pace data at render time. */
function formatReserveDescription(
  snap: RateWindowSnapshot,
  t: (key: LocaleKey) => string,
): string | null {
  if (snap.reservePercent == null) return null;
  if (snap.reserveWillLastToReset) {
    return t("PanelReserveLastsUntilReset");
  }
  const eta = snap.reserveEtaSeconds;
  if (eta == null) return null;
  const h = Math.floor(eta / 3600);
  if (h >= 24) {
    return t("PanelReserveRunsOutInDaysHours")
      .replace("{}", String(Math.floor(h / 24)))
      .replace("{}", String(h % 24));
  }
  return t("PanelReserveRunsOutInHours").replace("{}", String(h));
}

function formatCurrency(amount: number, code: string): string {
  try {
    return new Intl.NumberFormat("en-US", {
      style: "currency",
      currency: code,
    }).format(amount);
  } catch {
    return `${code} ${amount.toFixed(2)}`;
  }
}

const USD_TO_CNY_REFERENCE_RATE = 7.2;

function formatTokenCount(value: number): string {
  return new Intl.NumberFormat("en-US", {
    maximumFractionDigits: 0,
  }).format(value);
}

// Standard Chinese myriad units — 万 (1e4) up to 亿 (1e8), matching how large
// token counts are read at a glance (568,759 → 56.9万, 50,000,000 → 5000万).
// Traditional Chinese uses the same words in traditional forms.
const CJK_COMPACT_UNITS: Record<"chinese" | "chinesetraditional", [number, string][]> = {
  chinese: [
    [1e8, "亿"],
    [1e4, "万"],
  ],
  chinesetraditional: [
    [1e8, "億"],
    [1e4, "萬"],
  ],
};

/**
 * Compact, human-scannable form of a large token count (e.g. 568,759 → "56.9万",
 * 50,000,000 → "5000万", 120,000,000 → "1.2亿"). Returns null when the value is
 * small enough that the raw number already reads cleanly, so the caller can skip
 * the approximation. Chinese uses 万/亿; other locales fall back to the standard
 * locale-aware compact notation (569K, 57万, 57만…).
 */
function formatCompactTokens(value: number, language: Language): string | null {
  if (language === "chinese" || language === "chinesetraditional") {
    if (value < 1e4) return null;
    for (const [threshold, unit] of CJK_COMPACT_UNITS[language]) {
      if (value >= threshold) {
        const scaled = value / threshold;
        const text =
          scaled >= 100 ? Math.round(scaled).toString() : scaled.toFixed(1).replace(/\.0$/, "");
        return `${text}${unit}`;
      }
    }
    return null;
  }
  if (value < 1000) return null;
  const localeCode =
    language === "japanese"
      ? "ja-JP"
      : language === "korean"
        ? "ko-KR"
        : language === "spanish"
          ? "es-MX"
          : "en-US";
  return new Intl.NumberFormat(localeCode, {
    notation: "compact",
    maximumFractionDigits: 1,
  }).format(value);
}

/**
 * The token figure as the card shows it: the rounded form ("≈ 11.2億") when
 * there is one, the exact count otherwise.
 *
 * The guard matters because `Intl`'s compact notation returns the plain
 * digits for values below a language's first compact unit — Japanese renders
 * 1200 as "1200", not "1.2千". Printing "≈ 1200" would put an approximation
 * marker on an exact number, and drop the thousands separator while doing it.
 * So the compact string is only used when it actually differs from the digits
 * it came from.
 */
function formatApproxTokens(value: number, language: Language): string {
  const compact = formatCompactTokens(value, language);
  const isRounded =
    compact != null &&
    compact.replace(/[\s, ]/g, "") !== String(Math.round(value));
  return isRounded ? `≈ ${compact}` : formatTokenCount(value);
}

function formatApiEquivalentValue(amount: number): string {
  const cnyEstimate = amount * USD_TO_CNY_REFERENCE_RATE;
  return `${formatCurrency(amount, "USD")} · ¥${cnyEstimate.toFixed(2)}`;
}

interface LocalUsageLead {
  label: string;
  tokens: number | null;
  cost: number | null;
  topModel: string | null;
  empty: string;
}

/** Picks the period-appropriate label/tokens/cost/topModel out of a local-usage
 * summary — shared by the legacy `LocalUsageBlock` and the tray density-tier
 * insight row so both read the same Settings-driven `localUsagePeriod`. */
function resolveLocalUsageLead(
  period: LocalUsagePeriod,
  summary: ProviderLocalUsageSummary,
  t: (key: LocaleKey) => string,
): LocalUsageLead {
  if (period === "today") {
    return {
      label: t("PanelTodayUsage"),
      tokens: summary.todayTokens,
      cost: summary.todayCost,
      topModel: summary.todayTopModel,
      empty: t("PanelNoUsageToday"),
    };
  }
  if (period === "30d") {
    return {
      label: t("PanelThirtyDayUsage"),
      tokens: summary.thirtyDayTokens,
      cost: summary.thirtyDayCost,
      topModel: summary.thirtyDayTopModel,
      empty: t("PanelNoUsageThirtyDays"),
    };
  }
  return {
    label: t("PanelSevenDayUsage"),
    tokens: summary.sevenDayTokens,
    cost: summary.sevenDayCost,
    topModel: summary.sevenDayTopModel,
    empty: t("PanelNoUsageSevenDays"),
  };
}

export function LocalUsageBlock({
  providerId,
  summary,
  costHistory,
  period = "7d",
}: {
  providerId: string;
  summary: ProviderLocalUsageSummary;
  costHistory: DailyCostPoint[];
  period?: LocalUsagePeriod;
}) {
  const { t, language } = useLocale();
  const historyDays = period === "today" ? 1 : period === "7d" ? 7 : 30;
  const visibleHistory = costHistory
    .slice(-historyDays)
    .filter((point) => point.value > 0);

  // The flyout follows the selected range literally: one choice, one block.
  const lead = resolveLocalUsageLead(period, summary, t);
  const compactTokens =
    lead.tokens != null ? formatCompactTokens(lead.tokens, language) : null;
  return (
    <section className="menu-card__group menu-card__local-usage">
      <div className="menu-card__local-period">
        <span className="menu-card__local-label">{lead.label}</span>
        {lead.tokens != null && lead.tokens > 0 ? (
          <>
            <div className="menu-card__local-token-value">
              <strong>{formatTokenCount(lead.tokens)}</strong>
              <span>{t("PanelTokenUnit")}</span>
              {compactTokens && (
                <span className="menu-card__local-token-approx">≈ {compactTokens}</span>
              )}
            </div>
            {lead.cost != null && (
              <div className="menu-card__local-equivalent">
                {t("PanelApiEquivalentValue")} ≈ {formatApiEquivalentValue(lead.cost)}
              </div>
            )}
          </>
        ) : (
          <strong className="menu-card__local-empty">{lead.empty}</strong>
        )}
      </div>

      {/* The same `BarChart` the 最近输出速度 and 费用 tabs draw. This was a
          hand-rolled row of `<span>` blocks with a hardcoded orange and a fixed
          8px width, which is why one tab of a three-tab panel looked unrelated
          to its neighbours.

          The `isCodex` gate is gone too, and that was the larger defect: every
          other provider's 用量 tab drew *nothing at all*. Whether there is
          history to plot is the only thing that should decide it. */}
      {visibleHistory.length > 0 && (
        <BarChart
          data={visibleHistory.map((point) => ({
            label: point.date,
            value: point.value,
          }))}
          height={90}
          color="var(--accent)"
          valueFormatter={(value) => formatCurrency(value, "USD")}
          ariaLabel={t("PanelThirtyDayCostHistogram")}
        />
      )}

      {lead.topModel && (
        <div className="menu-card__local-note">
          <strong>{t("PanelTopModelPrefix")}: {lead.topModel}</strong>
        </div>
      )}
    </section>
  );
}

/**
 * Chart data is loaded asynchronously from the local history store. Keep the
 * space and the reading rhythm stable while that request is in flight rather
 * than letting the lower half of the card appear a moment later.
 */
function LocalUsageSkeleton() {
  const { t } = useLocale();

  return (
    <section
      className="menu-card__group menu-card__local-usage menu-card__local-usage--loading"
      aria-busy="true"
      aria-label={t("ProviderStatusLoading")}
    >
      <div className="menu-card__local-skeleton" aria-hidden="true">
        {[0].map((index) => (
          <div className="menu-card__local-period" key={index}>
            <span className="menu-card__skeleton menu-card__skeleton--label" />
            <strong className="menu-card__skeleton menu-card__skeleton--value" />
            <span className="menu-card__skeleton menu-card__skeleton--equivalent" />
          </div>
        ))}
      </div>
      <div className="menu-card__local-note" aria-hidden="true">
        <span className="menu-card__skeleton menu-card__skeleton--note" />
      </div>
    </section>
  );
}

function WayfinderUsageBlock({
  usage,
}: {
  usage: NonNullable<ProviderUsageSnapshot["wayfinderUsage"]>;
}) {
  const amount = (value: number) =>
    usage.priced ? `${value.toFixed(4)} ${usage.unit.toUpperCase()}` : "—";
  return (
    <section className="menu-card__group menu-card__local-usage">
      <div className="menu-card__local-grid">
        <div><span className="menu-card__local-label">网关状态</span><strong>{usage.gatewayStatus}</strong></div>
        <div><span className="menu-card__local-label">模型</span><strong>{usage.modelCount}</strong></div>
        <div><span className="menu-card__local-label">请求</span><strong>{usage.requests.toLocaleString()}</strong></div>
        <div><span className="menu-card__local-label">Token</span><strong>{usage.tokens.toLocaleString()}</strong></div>
      </div>
      <div className="menu-card__cost-line">近 {usage.periodDays} 天节省：{amount(usage.saved)}（{usage.savedPercent.toFixed(1)}%）</div>
      {(usage.offline || usage.dryRun || usage.missingKeys.length > 0) && (
        <div className="menu-card__local-note">
          {usage.offline && <span>离线模式 </span>}
          {usage.dryRun && <span>演练模式 </span>}
          {usage.missingKeys.length > 0 && <span>缺少密钥：{usage.missingKeys.join(", ")}</span>}
        </div>
      )}
    </section>
  );
}

function displayPlanName(planName: string | null): string | null {
  if (!planName) return null;
  const normalized = planName.trim().toLowerCase();
  // Raw claude.ai tier ids leak through both bare ("default_claude_ai") and
  // wrapped by older backends ("Claude (default_claude_ai)") — cached
  // snapshots can still carry the wrapped form after the Rust-side fix.
  if (normalized.includes("default_claude_ai")) return "Claude AI";
  return planName;
}

// `paceStageKey` lived here to caption the compact tier's pace chip with a
// stage word ("略微领先" …). That chip was the divergent forecast language item
// 4 removes, and `paceStateOf` — which the shared forecast row uses — collapses
// the same stages into the three states the row actually renders. The
// DetailPace* keys stay in the locale files for the Providers detail view.

/** Runway line shared by the legacy pace section and the tray density-tier
 * pace row — the weekly usage forecast merged in: status (icon + text) on
 * the left, the estimated hours on the right. */
function PaceRunway({
  pace,
  weeklyForecast,
  t,
}: {
  pace: PaceSnapshot;
  weeklyForecast: ReturnType<typeof getPaceEstimate>;
  t: (key: LocaleKey) => string;
}) {
  const lasts = weeklyForecast
    ? weeklyForecast.lastsUntilReset
    : pace.willLastToReset;
  const hours =
    weeklyForecast?.hoursRemaining ??
    (pace.etaSeconds != null ? pace.etaSeconds / 3600 : null);
  const formatHours = (h: number) =>
    h < 1
      ? t("PanelForecastLessThanHour")
      : `${h < 10 ? Math.round(h * 10) / 10 : Math.round(h)} ${t("PanelForecastHoursUnit")}`;
  return (
    <div className="menu-card__pace-runway" data-state={lasts ? "ok" : "warn"}>
      <span className="menu-card__pace-runway-status">
        {lasts ? <CheckIcon /> : <WarnIcon />}
        {lasts ? t("DetailPaceWillLastToReset") : t("DetailPaceRunsOutIn")}
      </span>
      {hours != null && (
        <strong className="menu-card__pace-runway-value">
          ≈ {formatHours(hours)}
        </strong>
      )}
    </div>
  );
}


interface MetricEntry {
  id: string;
  label: string;
  snap: RateWindowSnapshot;
}

type MetricPaceView =
  | { kind: "forecast"; estimate: NonNullable<ReturnType<typeof getPaceEstimate>> }
  | { kind: "reserve"; percent: number }
  | { kind: "none" };

function getMetricPaceView(snap: RateWindowSnapshot): MetricPaceView {
  if (snap.isExhausted) return { kind: "none" };

  const estimate = isWeeklyWindow(snap) ? getPaceEstimate(snap) : null;
  if (estimate) return { kind: "forecast", estimate };

  if (snap.reservePercent != null) {
    return { kind: "reserve", percent: snap.reservePercent };
  }

  return { kind: "none" };
}

/**
 * True when this window is the weekly one the pace snapshot describes.
 *
 * `kind` is decided once in Rust (`quota_cycle.rs`). This used to test
 * `windowMinutes >= 7 days` — an open-ended bound with no upper limit — so a
 * MONTHLY window also answered true and collected the weekly forecast, while the
 * taskbar strip, using closed bands, called the same window monthly.
 */
function isWeeklyWindow(snap: RateWindowSnapshot): boolean {
  return snap.kind === "weekly";
}

/**
 * The weekly forecast, rendered inside the weekly quota block (item A).
 *
 * Previously the card drew the weekly quota bar here and a second, visually
 * identical "pace" bar further down with its own actual/expected track — two
 * weekly blocks describing the same window. The forecast now annotates the quota
 * bar the block already draws: the expected position becomes a marker on that
 * track, and this component contributes only the words.
 *
 * When there is no usable forecast it says so. Rendering `0` or an em dash here
 * would read as "you are 0% ahead", which is a measurement the data does not
 * support.
 */
/**
 * Under- or over-spent for how far the window has elapsed.
 *
 * `deltaPercent` is actual minus expected, so a positive delta means burning
 * faster than the clock — a deficit — and negative means reserve. Same mapping
 * as the macOS card, and shared by the bar stripe and the label so the colour
 * and the words can never disagree.
 *
 * The dead band is +/-2 points, copied from macOS `UsagePace.stage(for:)` where
 * `absDelta <= 2` is `onTrack`. An earlier version treated anything that did not
 * round to 0 as off-pace, so a 0.6-point drift already shouted "in reserve" and
 * "On pace" was effectively unreachable.
 */
const ON_PACE_DEAD_BAND = 2;

function paceStateOf(
  forecast: QuotaForecastDisplay,
): "reserve" | "deficit" | "on-pace" {
  // Prefer the stage shared Rust already computed. `UsagePace::stage_for_delta`
  // owns the +/-2, 6, 12 thresholds; re-deriving them here once drifted and made
  // "on pace" unreachable, so this collapses its seven stages rather than
  // recomputing the buckets.
  if (forecast.stage) {
    if (forecast.stage === "on_track") return "on-pace";
    return forecast.stage.endsWith("ahead") ? "deficit" : "reserve";
  }
  // Frontend-derived forecast: no Rust stage, so apply the same dead band.
  const delta = forecast.deltaPercent;
  if (delta == null || Math.abs(delta) <= ON_PACE_DEAD_BAND) return "on-pace";
  return delta > 0 ? "deficit" : "reserve";
}

function WeeklyForecast({
  forecast,
  t,
}: {
  forecast: QuotaForecastDisplay;
  t: (key: LocaleKey) => string;
}) {
  if (!forecast.available || forecast.projectedUsedPercent == null) {
    return (
      <div className="menu-metric__forecast menu-metric__forecast--unavailable">
        <span className="menu-metric__forecast-note">
          {t("QuotaForecastUnavailable")}
        </span>
      </div>
    );
  }

  const state = paceStateOf(forecast);
  const delta = Math.abs(forecast.deltaPercent ?? 0);
  const lasts = forecast.lastsToReset ?? true;
  const hours =
    forecast.etaSeconds != null && forecast.etaSeconds > 0
      ? forecast.etaSeconds / 3600
      : null;
  const formatHours = (h: number) =>
    h < 1
      ? t("PanelForecastLessThanHour")
      : `${h < 10 ? Math.round(h * 10) / 10 : Math.round(h)} ${t("PanelForecastHoursUnit")}`;

  // Left / right exactly as macOS `UsagePaceText.weeklyDetail` composes them.
  //
  // Left is the verdict: "On pace" / "N% in reserve" / "N% in deficit". The
  // elapsed-percent yardstick is deliberately NOT repeated — the punched stripe
  // on the bar above IS that number, which is the point of the merge.
  //
  // Right follows `detailRightLabel`: when the window lasts to reset it says so
  // and shows NO duration. A duration appears only when it will run out, as
  // "runs out in <time>". Printing hours in both cases (the earlier version)
  // made the healthy case read like a countdown to exhaustion.
  const stateText =
    state === "on-pace"
      ? t("QuotaPaceOnPace")
      : `${delta.toFixed(1)}% ${
          state === "reserve" ? t("QuotaPaceInReserve") : t("QuotaPaceInDeficit")
        }`;
  const runwayText = lasts
    ? t("DetailPaceWillLastToReset")
    : hours != null
      ? `${t("DetailPaceRunsOutIn")} ${formatHours(hours)}`
      : t("DetailPaceRunsOutIn");

  return (
    <div className="menu-metric__forecast">
      <span className="menu-metric__forecast-state" data-pace-state={state}>
        {stateText}
      </span>
      <span className="menu-metric__forecast-runway" data-lasts={lasts ? "yes" : "no"}>
        {lasts ? <CheckIcon /> : <WarnIcon />} {runwayText}
      </span>
    </div>
  );
}

/**
 * Single metric row inside the card — mirrors upstream `MetricRow`:
 *   • title (body / medium)
 *   • UsageProgressBar (capsule, 6pt)
 *   • HStack: "N% used"  ··  reset countdown (right-aligned, secondary)
 */
function MetricRow({
  title,
  snap,
  exhaustedLabel,
  display,
  hero = false,
  planLabel = null,
  pace = null,
  showForecast = false,
}: {
  title: string;
  snap: RateWindowSnapshot;
  exhaustedLabel: string;
  display: QuotaDisplayContext;
  hero?: boolean;
  planLabel?: string | null;
  /** The provider's weekly pace snapshot, or null when it has none. */
  pace?: PaceSnapshot | null;
  /**
   * Render the merged weekly forecast inside this block. Set only for the weekly
   * window, because that is the window `provider.pace` actually measures — the
   * bridge selects it by window length, not by slot.
   */
  showForecast?: boolean;
}) {
  const { t } = useLocale();
  const reserveDescription = formatReserveDescription(snap, t);
  // A forecast is only meaningful for the weekly window, and only when the
  // window itself carries a real quota — projecting over a balance provider's
  // synthetic 0% window would fabricate a number (item A.4).
  const forecast = showForecast
    ? quotaForecastDisplay(pace, snap)
    : FORECAST_UNAVAILABLE;
  const marker = forecastMarkerPercent(forecast, display);
  const reservePercent =
    !snap.isExhausted && snap.reservePercent != null ? snap.reservePercent : null;

  return (
    <ProviderQuotaBlock
      title={title}
      rate={snap}
      display={display}
      usedLabel={t("PanelUsedSuffix")}
      remainingLabel={t("PanelLeftSuffix")}
      exhaustedLabel={exhaustedLabel}
      hero={hero}
      planLabel={planLabel}
      markerPercent={marker}
      paceState={paceStateOf(forecast)}
      expectedLabel={t("PanelExpected")}
    >
      {showForecast && !snap.isInformational && !snap.isExhausted && (
        <WeeklyForecast forecast={forecast} t={t} />
      )}
      {reservePercent != null && (
        <div className="menu-metric__row menu-metric__reserve">
          <span className="menu-metric__pct">{Math.round(reservePercent)}% {t("PanelReserveSuffix")}</span>
          {reserveDescription && (
            <span className="menu-metric__reset">{reserveDescription}</span>
          )}
        </div>
      )}
    </ProviderQuotaBlock>
  );
}

/** The compact reference card keeps the secondary quota as a weekly summary
 * row (label + percentage + track), instead of collapsing it into a plain
 * text line. This mirrors the reference HTML's shorter vertical rhythm. */
function CompactSecondaryQuota({
  title,
  rate,
  display,
  pace = null,
  showForecast = false,
}: {
  title: string;
  rate: RateWindowSnapshot;
  display: QuotaDisplayContext;
  /** The provider's weekly pace snapshot, or null when it has none. */
  pace?: PaceSnapshot | null;
  /**
   * Render the merged weekly forecast under this row. Compact usually carries
   * the weekly window here rather than in the hero, so this is where the tier
   * meets item 4's "same forecast language in all three tiers" requirement.
   */
  showForecast?: boolean;
}) {
  const { t } = useLocale();
  const percent = quotaPercentDisplay(rate, display);

  return (
    <div className="menu-card__compact-secondary">
      <div className="menu-card__compact-secondary-header">
        <span>{title}</span>
        <strong>{percent.rounded}%</strong>
      </div>
      <div className="menu-card__compact-secondary-track">
        <span style={{ width: `${percent.fillPercent}%` }} />
      </div>
      {showForecast && !rate.isInformational && !rate.isExhausted && (
        <WeeklyForecast forecast={quotaForecastDisplay(pace, rate)} t={t} />
      )}
    </div>
  );
}

/**
 * Provider card — direct mirror of SwiftUI `UsageMenuCardView`.
 *
 * Layout (top to bottom):
 *   1. Header
 *        – HStack: providerName (headline/semibold)  ··  updated time (footnote/secondary, right)
 *        – error block (only when the provider failed)
 *   2. Divider (1pt)
 *   3. VStack(spacing: 12)
 *        – Metrics group VStack(spacing: 12) of MetricRow
 *        – (Divider) Cost group: title (body/medium) + session line + month line (footnote)
 *        – (Divider) Pace group (Tauri-only addition; placed last)
 *        – (Divider) Charts group (Tauri-only addition; placed last)
 *
 * Padding: upstream v0.32.2 uses wider horizontal card padding and slightly
 * taller header/content vertical padding so account/plan rows can breathe.
 */
export default function MenuCard({
  provider,
  display,
  compactMetrics = false,
  outputSpeed = null,
  localUsagePeriod = "7d",
  hideLocalUsage = false,
  showProviderIcon = true,
  densityMode,
  quotaWindows,
}: MenuCardProps) {
  const { t, language } = useLocale();
  const [chartData, setChartData] = useState<ProviderChartData | null>(null);
  const [isChartDataLoading, setIsChartDataLoading] = useState(false);
  const formattedCostReset = useFormattedResetTime(
    provider.cost?.resetsAt ?? null,
    null,
    display.resetTimeRelative,
  );

  useEffect(() => {
    if (!providerSupportsChartData(provider.providerId)) {
      setChartData(null);
      setIsChartDataLoading(false);
      return;
    }
    let cancelled = false;
    setChartData(null);
    setIsChartDataLoading(true);
    getProviderChartData(
      provider.providerId,
      provider.accountEmail ?? undefined,
    )
      .then((data) => {
        if (!cancelled) {
          setChartData(data);
        }
      })
      .catch(() => {
        /* chart data is best-effort */
      })
      .finally(() => {
        if (!cancelled) {
          setIsChartDataLoading(false);
        }
      });
    return () => {
      cancelled = true;
    };
  }, [provider.providerId, provider.accountEmail]);

  const isWayfinder = provider.providerId === "wayfinder";
  const planName = isWayfinder ? null : displayPlanName(provider.planName);

  // Balance-type providers (DeepSeek, MiMo, MiMo API) encode a prepaid balance
  // or API-key status as ad-hoc strings inside quota-shaped windows. This helper
  // normalizes every encoding into one structured view, tells us which windows
  // to skip so we don't render synthetic 0% bars, and whether the plan badge is
  // really a balance echo that should be hidden. See lib/providerBalance.
  const { balance, excludeWindows, suppressPlanBadge } = getProviderBalance(provider);

  const resetCreditsWindow = provider.extraRateWindows?.find(
    (extra) => extra.id === "reset-credits",
  );
  const resetCreditsAvailable = (() => {
    const description = resetCreditsWindow?.window.resetDescription ?? "";
    const match = description.match(/^(\d+)\s+reset credits? available$/i);
    return match ? Number(match[1]) : null;
  })();

  const metrics: MetricEntry[] = [];
  const wayfinderUsage = isWayfinder ? provider.wayfinderUsage ?? null : null;
  if (!isWayfinder && !excludeWindows.has("primary") && isMeaningfulQuotaWindow(provider.primary))
    metrics.push({
      id: "primary",
      label: quotaWindowLabel(provider.primaryLabel, provider.primary, t),
      snap: provider.primary,
    });
  if (
    provider.secondary &&
    !excludeWindows.has("secondary") &&
    isMeaningfulQuotaWindow(provider.secondary)
  )
    metrics.push({
      id: "secondary",
      label: quotaWindowLabel(provider.secondaryLabel, provider.secondary, t),
      snap: provider.secondary,
    });
  if (provider.modelSpecific && isMeaningfulQuotaWindow(provider.modelSpecific))
    metrics.push({
      id: "model-specific",
      label: t("DetailWindowModelSpecific"),
      snap: provider.modelSpecific,
    });
  if (provider.tertiary && isMeaningfulQuotaWindow(provider.tertiary))
    metrics.push({
      id: "tertiary",
      label: t("DetailWindowTertiary"),
      snap: provider.tertiary,
    });
  for (const extra of provider.extraRateWindows ?? []) {
    if (extra.id === "reset-credits") continue;
    if (!isMeaningfulQuotaWindow(extra.window)) continue;
    metrics.push({
      id: `extra-${extra.id}`,
      label: extra.title,
      snap: extra.window,
    });
  }
  // The surface's own quota-window filter (item H). Applied before the compact
  // slice so "compact shows the first window" means the first window the user
  // asked to see, not the first one the provider happens to publish.
  //
  // Never allowed to empty the card: a filter that matches nothing here leaves
  // a provider with a name, a plan badge and no readings at all, and nothing on
  // the card explains why. Falling back to the unfiltered list is the same
  // choice `resolveDashboardProviderIds` makes for the same reason.
  const filteredMetrics = (() => {
    const kept = metrics.filter((metric) =>
      dashboardShowsQuotaWindow(metric.snap.kind, quotaWindows),
    );
    return kept.length > 0 ? kept : metrics;
  })();
  // Compact is deliberately a summary row, not a nearly-identical detailed
  // card. Keep only the primary quota and omit secondary diagnostics below.
  const visibleMetrics = compactMetrics
    ? filteredMetrics.slice(0, 1)
    : filteredMetrics;

  // Which visible row owns the weekly forecast (item A). `provider.pace` is
  // computed by the bridge against the weekly window specifically, so the
  // forecast must attach to that row and to no other — attaching it to whatever
  // sits in `primary` is the bug that once made Claude's 5-hour usage render as
  // its weekly pace. Compact rows are a summary and get no forecast at all.
  const weeklyMetricId = compactMetrics
    ? null
    // `filteredMetrics`, not `metrics`: the forecast has to attach to a row
    // that is actually drawn, and the weekly window may have been filtered out.
    : (filteredMetrics.find((m) => isWeeklyWindow(m.snap))?.id ?? null);

  const hasCostHistory =
    !compactMetrics &&
    chartData !== null && chartData.costHistory.some((point) => point.value > 0);
  const hasCreditsHistory =
    !compactMetrics && chartData !== null && chartData.creditsHistory.length > 0;
  const hasUsageBreakdown =
    !compactMetrics && chartData !== null && chartData.usageBreakdown.length > 0;
  const hasCharts = hasCostHistory || hasCreditsHistory || hasUsageBreakdown;
  const localUsage =
    compactMetrics || hideLocalUsage || provider.error ? null : chartData?.localUsage ?? null;
  const showLocalUsagePlaceholder =
    !compactMetrics &&
    !hideLocalUsage &&
    !provider.error &&
    providerSupportsChartData(provider.providerId) &&
    isChartDataLoading;
  const localCostHistory = chartData?.costHistory ?? [];
  const hasMetrics = visibleMetrics.length > 0;
  const hasResetCredits = !compactMetrics && resetCreditsAvailable != null;
  const hasCost = !compactMetrics && !!provider.cost;
  const hasPace = !compactMetrics && !!provider.pace;
  const hasDetails =
    !provider.error &&
    (hasMetrics ||
      hasResetCredits ||
      !!balance ||
      hasCost ||
      hasPace ||
      hasCharts ||
      !!localUsage ||
      showLocalUsagePlaceholder ||
      !!wayfinderUsage);
  const displayError = provider.error ? localizeProviderError(provider.error, t) : null;

  // ── Tray flyout density tiers (detailed/compact/minimal) ────────────
  // Only active when the caller passes `densityMode` (TrayPanel's overview
  // list + single-provider detail view). Every other call site (Settings
  // preview card, PopOut) leaves this undefined and renders the legacy
  // `.menu-card__content` below exactly as before — none of the variables
  // in this block affect their output.
  const primaryMetric = metrics[0] ?? null;
  const secondaryMetric = metrics[1] ?? null;
  const hasPaceForDensity = !provider.error && !!provider.pace;

  // Which quota row inside THIS tier renders the weekly forecast.
  //
  // Detailed draws every metric, so the weekly row always owns it. Compact
  // draws two rows and minimal draws one, so the weekly window may not be
  // drawn at all — then no row owns the forecast and `densityOrphanForecast`
  // below renders it standalone. Package item 4: the three tiers share one
  // forecast language, and none of them may go blank on pace.
  const densityForecastRowId = (() => {
    if (!weeklyMetricId) return null;
    if (densityMode === "detailed") return weeklyMetricId;
    const drawn =
      densityMode === "compact"
        ? [primaryMetric?.id, secondaryMetric?.id]
        : [primaryMetric?.id];
    return drawn.includes(weeklyMetricId) ? weeklyMetricId : null;
  })();
  const densityLocalUsageLead =
    !provider.error && chartData?.localUsage
      ? resolveLocalUsageLead(localUsagePeriod, chartData.localUsage, t)
      : null;
  const hasDensityOutputSpeed =
    !provider.error &&
    !!outputSpeed &&
    outputSpeed.tokensPerSecond != null &&
    outputSpeed.tokensPerSecond > 0;
  const hasDensityUsage =
    !!densityLocalUsageLead &&
    densityLocalUsageLead.tokens != null &&
    densityLocalUsageLead.tokens > 0;
  const hasDensityContent =
    !provider.error &&
    (metrics.length > 0 ||
      !!balance ||
      hasPaceForDensity ||
      hasDensityUsage ||
      hasDensityOutputSpeed ||
      !!wayfinderUsage);

  const effectiveHasDetails = densityMode ? hasDensityContent : hasDetails;
  const cardClassName = [
    "menu-card",
    densityMode ? `menu-card--${densityMode}` : null,
    provider.error ? "menu-card--error" : null,
    effectiveHasDetails ? "menu-card--with-details" : "menu-card--header-only",
  ]
    .filter(Boolean)
    .join(" ");

  // Detailed/compact/minimal share one card header; only the "updated"
  // timestamp differs — compact/minimal drop the trailing "更新" (Chinese
  // "…updated") suffix and use a slightly smaller font. Locales whose
  // `UpdatedMinutesAgo`-family strings don't end in "更新" (i.e. every
  // non-Chinese locale) are unaffected — the replace is a no-op there.
  const useShortUpdatedTime = densityMode === "compact" || densityMode === "minimal";
  const rawUpdatedText = Number.isNaN(Date.parse(provider.updatedAt))
    ? provider.updatedAt
    : formatRelativeUpdated(Date.parse(provider.updatedAt), t);
  const updatedText = useShortUpdatedTime
    ? rawUpdatedText.replace(/更新$/, "")
    : rawUpdatedText;
  const providerIconSize = densityMode === "detailed" ? 22 : 18;

  const densityPrimaryRow = primaryMetric ? (
    <MetricRow
      title={primaryMetric.label}
      snap={primaryMetric.snap}
      exhaustedLabel={t("DetailWindowExhausted")}
      display={display}
      hero
      // The reference FloatBar keeps the plan name out of the quota bar;
      // detailed/compact/minimal all reserve that space for the percentage.
      planLabel={null}
      pace={provider.pace}
      // All three density tiers share the same forecast *language*. Detailed
      // shows the full runway row inside the weekly quota block; compact and
      // minimal also attach the forecast when the hero row *is* the weekly
      // window so they never go blank on pace (TASK-021 item 4). Compact/
      // minimal still reduce surrounding density elsewhere.
      showForecast={primaryMetric.id === densityForecastRowId}
    />
  ) : balance ? (
    <ProviderBalanceBlock balance={balance} showTitle={false} />
  ) : null;

  const densitySecondaryPercent = (rate: RateWindowSnapshot): number =>
    quotaPercentDisplay(rate, display).rounded;

  // Detailed shows every remaining metric window as its own hero-less row
  // (matches the legacy metrics section — a provider with 3+ quota windows,
  // e.g. Copilot's extra budget window, still shows all of them in detail).
  // Compact/minimal are intentionally summaries: compact adds one plain
  // (no-bar) secondary row when present; minimal shows only the hero row.
  const zone1 = (() => {
    if (densityMode === "detailed") {
      return (
        <section className="menu-card__zone menu-card__zone--quota menu-card__metrics">
          {metrics.length > 0
            ? metrics.map((m, idx) => (
                <MetricRow
                  key={m.id}
                  title={m.label}
                  snap={m.snap}
                  exhaustedLabel={t("DetailWindowExhausted")}
                  display={display}
                  hero={idx === 0}
                  planLabel={null}
                  pace={provider.pace}
                  showForecast={m.id === weeklyMetricId}
                />
              ))
            : balance && <ProviderBalanceBlock balance={balance} showTitle={false} />}
          {!provider.error && hasResetCredits && (
            <>
              {/* The reference card separates the reset-credits pill from the
                  quota windows above it with the same hairline it uses between
                  quota windows (see the `.divider` before `.reset-credits` in
                  design/floatbar-reference.html). */}
              <div className="menu-card__divider" />
              <section className="menu-card__reset-credits" aria-label={t("PanelResetCreditsTitle")}>
                <span>{t("PanelResetCreditsTitle")}</span>
                <strong>
                  {t("PanelResetCreditsRemaining")} {resetCreditsAvailable} {t("PanelResetCreditsUnit")}
                </strong>
              </section>
            </>
          )}
        </section>
      );
    }
    if (densityMode === "compact") {
      return (
        <section className="menu-card__zone menu-card__zone--quota menu-card__metrics">
          {densityPrimaryRow}
          {secondaryMetric && (
            <CompactSecondaryQuota
              title={secondaryMetric.label}
              rate={secondaryMetric.snap}
              display={display}
              pace={provider.pace}
              showForecast={secondaryMetric.id === densityForecastRowId}
            />
          )}
        </section>
      );
    }
    // minimal — bare metrics block, no tinted zone background (kept
    // deliberately lighter-weight than the detailed/compact zones). The hero
    // row still carries forecast when it is the weekly window (see
    // densityPrimaryRow.showForecast).
    return <section className="menu-card__metrics">{densityPrimaryRow}</section>;
  })();

  const densityInsightsZone = (() => {
    if (densityMode !== "detailed") return null;
    const speedNode = hasDensityOutputSpeed && outputSpeed ? (
      <div className="menu-card__insight-row">
        <span className="menu-card__insight-label">
          <SpeedIcon />
          {t("OutputSpeedTitle")}
        </span>
        <span className="menu-card__insight-value">
          {outputSpeed.tokensPerSecond!.toFixed(1)}
          <span className="menu-card__insight-unit"> t/s</span>
        </span>
      </div>
    ) : null;
    const usageNode = hasDensityUsage && densityLocalUsageLead ? (
      <div className="menu-card__insight-block">
        <div className="menu-card__insight-row">
          <span className="menu-card__insight-label">
            <UsageIcon />
            {densityLocalUsageLead.label}
          </span>
          {/* Only the rounded figure ("≈ 11.2亿"). The exact count was here
              too, as the reference card prints it, but at nine digits it
              carried no information anyone reads at a glance while dominating
              the row. It falls back to the exact count when the language has
              no compact form to render. No "Token" unit word either — the
              reference omits it, and it made the value wide enough to
              ellipsize the label beside it. The speed row above does keep its
              "t/s", which the reference shows. */}
          <span className="menu-card__insight-value">
            {formatApproxTokens(densityLocalUsageLead.tokens!, language)}
          </span>
        </div>
        {densityLocalUsageLead.cost != null && (
          <div className="menu-card__local-equivalent">
            {t("PanelApiEquivalentValue")} ≈ {formatApiEquivalentValue(densityLocalUsageLead.cost)}
          </div>
        )}
        {densityLocalUsageLead.topModel && (
          <div className="menu-card__local-note">
            {t("PanelTopModelPrefix")}: {densityLocalUsageLead.topModel}
          </div>
        )}
      </div>
    ) : null;
    // The density tiers used to repeat the pace summary and its own weekly
    // actual/expected track here. Both now live inside the weekly quota row, so
    // this zone carries only speed, local usage and Wayfinder.
    const wayfinderNode = wayfinderUsage ? <WayfinderUsageBlock usage={wayfinderUsage} /> : null;
    const nodes = [speedNode, usageNode, wayfinderNode].filter(
      (node): node is React.ReactElement => node !== null,
    );
    if (nodes.length === 0) return null;
    return (
      <section className="menu-card__zone menu-card__zone--insights">
        {nodes.map((node, idx) => (
          <Fragment key={idx}>
            {idx > 0 && <div className="menu-card__divider" />}
            {node}
          </Fragment>
        ))}
      </section>
    );
  })();

  const densityChipRow = (() => {
    if (densityMode !== "compact") return null;
    const chips: React.ReactElement[] = [];
    if (hasDensityOutputSpeed && outputSpeed) {
      chips.push(
        <div className="menu-card__chip" key="speed">
          <span className="menu-card__chip-value">
            <SpeedIcon />
            {outputSpeed.tokensPerSecond!.toFixed(1)}
          </span>
          <span className="menu-card__chip-caption">t/s</span>
        </div>,
      );
    }
    if (hasDensityUsage && densityLocalUsageLead) {
      const approx = formatCompactTokens(densityLocalUsageLead.tokens!, language);
      chips.push(
        <div className="menu-card__chip" key="usage">
          <span className="menu-card__chip-value">
            {approx ?? formatTokenCount(densityLocalUsageLead.tokens!)}
          </span>
          <span className="menu-card__chip-caption">{densityLocalUsageLead.label}</span>
        </div>,
      );
    }
    // No pace chip. It used to print a bare signed delta ("+12.3%" + stage
    // caption) — a second, divergent way of saying what the weekly forecast
    // row now says in the language all three tiers share (item 4: 同一套预测
    // 视觉语言). Detailed dropped its duplicate pace summary for the same
    // reason; see the note in `densityInsightsZone`.
    if (chips.length === 0) return null;
    return <div className="menu-card__chip-row">{chips}</div>;
  })();

  const densityMinimalSummaryLine = (() => {
    if (densityMode !== "minimal") return null;
    const segments: React.ReactElement[] = [];
    if (secondaryMetric) {
      segments.push(
        <span className="menu-card__minimal-segment" key="secondary">
          {secondaryMetric.label} {densitySecondaryPercent(secondaryMetric.snap)}%
        </span>,
      );
    }
    if (hasDensityOutputSpeed && outputSpeed) {
      segments.push(
        <span className="menu-card__minimal-segment" key="speed">
          <SpeedIcon /> {outputSpeed.tokensPerSecond!.toFixed(1)}t/s
        </span>,
      );
    }
    // No bare pace segment here either — same reason as the compact chip row.
    if (segments.length === 0) return null;
    return <div className="menu-card__minimal-line">{segments}</div>;
  })();

  // The weekly window is not drawn as a quota row in this tier (minimal shows
  // only the hero, so a provider whose weekly sits in `secondary` — Claude and
  // Codex both do — would otherwise lose the forecast entirely, which is
  // exactly the "详细模式有预测、其他模式只剩空白" case item 4 forbids).
  //
  // It names the window it measures, because without the name it reads as a
  // statement about the hero row above — the misattribution `weeklyMetricId`
  // exists to prevent. The one exception is minimal's summary line, which
  // already prints that window's label and percentage right above.
  const densityOrphanForecast = (() => {
    if (!densityMode || densityMode === "detailed") return null;
    if (!weeklyMetricId || densityForecastRowId) return null;
    const weekly = filteredMetrics.find((m) => m.id === weeklyMetricId);
    if (!weekly || weekly.snap.isInformational || weekly.snap.isExhausted) return null;
    const namedAbove = densityMode === "minimal" && secondaryMetric?.id === weeklyMetricId;
    return (
      <div className="menu-card__orphan-forecast">
        {!namedAbove && (
          <span className="menu-card__orphan-forecast-label">{weekly.label}</span>
        )}
        <WeeklyForecast forecast={quotaForecastDisplay(provider.pace, weekly.snap)} t={t} />
      </div>
    );
  })();

  return (
    <article className={cardClassName}>
      <header className="menu-card__header">
        <div className="menu-card__title-row">
          {showProviderIcon && (
            <ProviderIcon
              providerId={provider.providerId}
              size={providerIconSize}
              className="menu-card__provider-icon"
            />
          )}
          <div className="menu-card__name-group">
            <span className="menu-card__name">{provider.displayName}</span>
          </div>
          {!provider.error && (
            <span
              className={`menu-card__subtitle menu-card__updated${useShortUpdatedTime ? " menu-card__updated--short" : ""}`}
            >
              {updatedText}
            </span>
          )}
        </div>
        {provider.error && (
          <div className="menu-card__error-block">
            <div className="menu-card__error-text">{displayError}</div>
            <CopyIconButton text={provider.error} />
          </div>
        )}
      </header>

      {/* The reference card has no rule under the name/updated row — its
          header is separated from the quota panel by whitespace alone, not
          a divider. That only applies to the new density layout; the legacy
          (non-densityMode) layout below keeps its original hairline. */}
      {effectiveHasDetails && !densityMode && <div className="menu-card__divider" />}

      {effectiveHasDetails && densityMode && (
        <div className="menu-card__content">
          {zone1}
          {densityInsightsZone}
          {densityChipRow}
          {densityMinimalSummaryLine}
          {densityOrphanForecast}
        </div>
      )}

      {effectiveHasDetails && !densityMode && (
        <div className="menu-card__content">
          {!provider.error && hasMetrics && (
            <section className="menu-card__group menu-card__metrics">
              {visibleMetrics.map((m, idx) => (
                <MetricRow
                  key={m.id}
                  title={m.label}
                  snap={m.snap}
                  exhaustedLabel={t("DetailWindowExhausted")}
                  display={display}
                  hero={idx === 0}
                  planLabel={idx === 0 && !suppressPlanBadge ? planName : null}
                  pace={provider.pace}
                  showForecast={m.id === weeklyMetricId}
                />
              ))}
            </section>
          )}

          {!provider.error && hasResetCredits && (
            <section className="menu-card__reset-credits" aria-label={t("PanelResetCreditsTitle")}>
              <span>{t("PanelResetCreditsTitle")}</span>
              <strong>
                {t("PanelResetCreditsRemaining")} {resetCreditsAvailable} {t("PanelResetCreditsUnit")}
              </strong>
            </section>
          )}

          {!provider.error && outputSpeed && (
            <OutputSpeedHighlight speed={outputSpeed} t={t} />
          )}

          {!provider.error && balance && (
            <ProviderBalanceBlock balance={balance} className="menu-card__group" />
          )}

          {!provider.error && wayfinderUsage && (
            <WayfinderUsageBlock usage={wayfinderUsage} />
          )}

          {(hasMetrics || !!balance) && (localUsage || showLocalUsagePlaceholder) && (
            <div className="menu-card__divider" />
          )}

          {showLocalUsagePlaceholder ? (
            <LocalUsageSkeleton />
          ) : localUsage ? (
            <LocalUsageBlock
              providerId={provider.providerId}
              summary={localUsage}
              costHistory={localCostHistory}
              period={localUsagePeriod}
            />
          ) : null}

          {(hasMetrics || !!balance || !!localUsage || showLocalUsagePlaceholder) && hasCost && (
            <div className="menu-card__divider" />
          )}

          {hasCost && provider.cost && (
            <section className="menu-card__group menu-card__cost">
              <div className="menu-card__group-title">
                {t("DetailCostTitle")} — {provider.cost.period}
              </div>
              <div className="menu-card__cost-line">
                {t("DetailCostUsed")}:{" "}
                {provider.cost.formattedUsed ||
                  formatCurrency(provider.cost.used, provider.cost.currencyCode)}
                {provider.cost.limit != null && (
                  <>
                    {" / "}
                    {provider.cost.formattedLimit ||
                      formatCurrency(provider.cost.limit, provider.cost.currencyCode)}
                  </>
                )}
              </div>
              {provider.cost.remaining != null && (
                <div className="menu-card__cost-line menu-card__cost-line--muted">
                  {t("DetailCostRemaining")}:{" "}
                  {formatCurrency(provider.cost.remaining, provider.cost.currencyCode)}
                </div>
              )}
              {formattedCostReset && (
                <div className="menu-card__cost-line menu-card__cost-line--muted">
                  {t("DetailCostResets")}: {formattedCostReset}
                </div>
              )}
            </section>
          )}

          {/* The standalone "进度" section used to live here with its own
              actual/expected track. That was a second weekly block describing
              the same window as the weekly quota row above it, which item A
              forbids — the forecast now annotates that row's own bar instead. */}

          {(hasMetrics || hasCost) && hasCharts && (
            <div className="menu-card__divider" />
          )}

          {hasCharts && (
            <section className="menu-card__group menu-card__charts">
              {hasCostHistory && (
                <SimpleBarChart
                  points={chartData!.costHistory}
                  label={t("DetailChartCost")}
                  color="var(--accent)"
                  formatValue={(v) => `$${v.toFixed(2)}`}
                />
              )}
              {hasCreditsHistory && (
                <SimpleBarChart
                  points={chartData!.creditsHistory}
                  label={t("DetailChartCredits")}
                  color="var(--provider-status-ok)"
                  formatValue={(v) => v.toFixed(1)}
                />
              )}
              {hasUsageBreakdown && (
                <StackedBarChart
                  points={chartData!.usageBreakdown}
                  label={t("DetailChartUsageBreakdown")}
                  height={56}
                />
              )}
            </section>
          )}
        </div>
      )}
    </article>
  );
}
