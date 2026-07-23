import { useCallback, useEffect, useState } from "react";
import type {
  DailyCostPoint,
  Language,
  LocalUsagePeriod,
  PaceSnapshot,
  ProviderChartData,
  ProviderLocalUsageSummary,
  ProviderOutputSpeed,
  ProviderUsageSnapshot,
  RateWindowSnapshot,
} from "../types/bridge";
import { getProviderChartData } from "../lib/tauri";
import { useLocale } from "../hooks/useLocale";
import { useFormattedResetTime } from "../hooks/useFormattedResetTime";
import { formatRelativeUpdated } from "../lib/relativeTime";
import type { LocaleKey } from "../i18n/keys";
import { paceCategory } from "../surfaces/tray/paceCategory";
import { SimpleBarChart, StackedBarChart } from "./MiniBarChart";
import { providerSupportsChartData } from "../lib/providerCharts";
import { getPaceEstimate } from "../lib/paceBudget";
import { getProviderBalance } from "../lib/providerBalance";
import { ProviderBalanceBlock } from "./ProviderBalanceBlock";
import { ProviderIcon } from "./providers/ProviderIcon";
import {
  isMeaningfulQuotaWindow,
  ProviderQuotaBlock,
  quotaWindowLabel,
} from "./ProviderQuotaBlock";

/** Line icons for the pace ("进度") block — 1.5–2pt stroke, inherit currentColor. */
const paceIconProps = {
  width: 13,
  height: 13,
  viewBox: "0 0 24 24",
  fill: "none",
  stroke: "currentColor",
  strokeWidth: 2,
  strokeLinecap: "round" as const,
  strokeLinejoin: "round" as const,
  "aria-hidden": true,
};
const GaugeIcon = () => (
  <svg {...paceIconProps}>
    <path d="M4 15a8 8 0 1 1 16 0" />
    <path d="M12 15l4-3" />
  </svg>
);
const TrendIcon = () => (
  <svg {...paceIconProps}>
    <path d="M3 17l6-6 4 4 8-8" />
    <path d="M15 7h6v6" />
  </svg>
);
const CheckIcon = () => (
  <svg {...paceIconProps}>
    <circle cx="12" cy="12" r="9" />
    <path d="M8.5 12.5l2.5 2.5 4.5-5" />
  </svg>
);
const WarnIcon = () => (
  <svg {...paceIconProps}>
    <path d="M12 3l9 16H3z" />
    <path d="M12 10v4" />
    <circle cx="12" cy="17" r="0.6" fill="currentColor" stroke="none" />
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
  resetTimeRelative: boolean;
  showAsUsed?: boolean;
  compactMetrics?: boolean;
  onLayoutChange?: () => void;
  /** Most recent completed-response speed for Codex or Claude. */
  outputSpeed?: ProviderOutputSpeed | null;
  /** Which period the local-usage stats block leads with (Settings-driven). */
  localUsagePeriod?: LocalUsagePeriod;
  /** Suppress the local-usage stats block entirely (e.g. compact display mode). */
  hideLocalUsage?: boolean;
  /** Show the provider brand icon in the card header. Governed by the
   * "show provider icons" setting on the tray; defaults on elsewhere. */
  showProviderIcon?: boolean;
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
  const requestUrl = message.match(
    /^network error:\s*error sending request for url\s*\((https?:\/\/[^\s)]+)\)\s*$/i,
  );
  if (requestUrl) {
    return t("ProviderIssueNetworkRequestFailed") + "：" + requestUrl[1];
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

function formatApiEquivalentValue(amount: number): string {
  const cnyEstimate = amount * USD_TO_CNY_REFERENCE_RATE;
  return `${formatCurrency(amount, "USD")} · ¥${cnyEstimate.toFixed(2)}`;
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
  const isCodex = providerId === "codex";
  const historyDays = period === "today" ? 1 : period === "7d" ? 7 : 30;
  const visibleHistory = costHistory
    .slice(-historyDays)
    .filter((point) => point.value > 0);
  const maxCost = Math.max(...visibleHistory.map((point) => point.value), 0);

  // The flyout follows the selected range literally: one choice, one block.
  const lead =
    period === "today"
      ? {
          label: t("PanelTodayUsage"),
          tokens: summary.todayTokens,
          cost: summary.todayCost,
          topModel: summary.todayTopModel,
          empty: t("PanelNoUsageToday"),
        }
      : period === "30d"
        ? {
            label: t("PanelThirtyDayUsage"),
            tokens: summary.thirtyDayTokens,
            cost: summary.thirtyDayCost,
            topModel: summary.thirtyDayTopModel,
            empty: t("PanelNoUsageThirtyDays"),
          }
        : {
            label: t("PanelSevenDayUsage"),
            tokens: summary.sevenDayTokens,
            cost: summary.sevenDayCost,
            topModel: summary.sevenDayTopModel,
            empty: t("PanelNoUsageSevenDays"),
          };
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

      {isCodex && visibleHistory.length > 0 && (
        <div className="menu-card__local-chart" aria-label={t("PanelThirtyDayCostHistogram")}>
          {visibleHistory.map((point, index) => (
            <span
              key={`${point.date}-${index}`}
              style={{
                height: `${Math.max(4, Math.round((point.value / maxCost) * 64))}px`,
              }}
              title={`${point.date}: ${formatCurrency(point.value, "USD")}`}
            />
          ))}
        </div>
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

function paceStageKey(stage: PaceSnapshot["stage"]): LocaleKey {
  switch (stage) {
    case "on_track":
      return "DetailPaceOnTrack";
    case "slightly_ahead":
      return "DetailPaceSlightlyAhead";
    case "ahead":
      return "DetailPaceAhead";
    case "far_ahead":
      return "DetailPaceFarAhead";
    case "slightly_behind":
      return "DetailPaceSlightlyBehind";
    case "behind":
      return "DetailPaceBehind";
    case "far_behind":
      return "DetailPaceFarBehind";
    default:
      return "DetailPaceOnTrack";
  }
}

const WEEKLY_WINDOW_MINUTES = 7 * 24 * 60;

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

  const isWeeklyWindow =
    snap.windowMinutes != null && snap.windowMinutes >= WEEKLY_WINDOW_MINUTES;
  const estimate = isWeeklyWindow ? getPaceEstimate(snap) : null;
  if (estimate) return { kind: "forecast", estimate };

  if (snap.reservePercent != null) {
    return { kind: "reserve", percent: snap.reservePercent };
  }

  return { kind: "none" };
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
  resetTimeRelative,
  showAsUsed,
  hero = false,
  planLabel = null,
  suppressForecast = false,
}: {
  title: string;
  snap: RateWindowSnapshot;
  exhaustedLabel: string;
  resetTimeRelative: boolean;
  showAsUsed: boolean;
  hero?: boolean;
  planLabel?: string | null;
  /** When the card also renders a dedicated pace ("进度") block, the weekly
   * forecast box is redundant — its "hours remaining" just restates the reset
   * countdown and its "lasts until reset" note duplicates the pace status. Hide
   * it here so the runway information lives in exactly one place. */
  suppressForecast?: boolean;
}) {
  const { t } = useLocale();
  const paceView = getMetricPaceView(snap);
  const reserveDescription = formatReserveDescription(snap, t);
  const formatHours = (hours: number) => {
    if (hours < 1) return t("PanelForecastLessThanHour");
    const rounded = hours < 10 ? Math.round(hours * 10) / 10 : Math.round(hours);
    return `${rounded} ${t("PanelForecastHoursUnit")}`;
  };
  return (
    <ProviderQuotaBlock
      title={title}
      rate={snap}
      resetTimeRelative={resetTimeRelative}
      showAsUsed={showAsUsed}
      usedLabel={t("PanelUsedSuffix")}
      remainingLabel={t("PanelLeftSuffix")}
      exhaustedLabel={exhaustedLabel}
      hero={hero}
      planLabel={planLabel}
    >
      {paceView.kind === "forecast" && !suppressForecast && (
        <div className="menu-metric__forecast">
          <div className="menu-metric__forecast-row">
            <span className="menu-metric__forecast-label">
              {t("PanelUsageForecast")}
            </span>
            <strong className="menu-metric__forecast-value">
              ≈ {formatHours(paceView.estimate.hoursRemaining)}
            </strong>
          </div>
          <span className="menu-metric__forecast-note">
            {paceView.estimate.lastsUntilReset
              ? t("PanelForecastUntilReset")
              : t("PanelForecastPrefix")}
          </span>
        </div>
      )}
      {paceView.kind === "reserve" && (
        <div className="menu-metric__row menu-metric__reserve">
          <span className="menu-metric__pct">{Math.round(paceView.percent)}% {t("PanelReserveSuffix")}</span>
          {reserveDescription && (
            <span className="menu-metric__reset">{reserveDescription}</span>
          )}
        </div>
      )}
    </ProviderQuotaBlock>
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
  resetTimeRelative,
  showAsUsed = false,
  compactMetrics = false,
  onLayoutChange,
  outputSpeed = null,
  localUsagePeriod = "7d",
  hideLocalUsage = false,
  showProviderIcon = true,
}: MenuCardProps) {
  const { t } = useLocale();
  const [chartData, setChartData] = useState<ProviderChartData | null>(null);
  const [isChartDataLoading, setIsChartDataLoading] = useState(false);
  const formattedCostReset = useFormattedResetTime(
    provider.cost?.resetsAt ?? null,
    null,
    resetTimeRelative,
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
          requestAnimationFrame(() => onLayoutChange?.());
        }
      })
      .catch(() => {
        /* chart data is best-effort */
      })
      .finally(() => {
        if (!cancelled) {
          setIsChartDataLoading(false);
          requestAnimationFrame(() => onLayoutChange?.());
        }
      });
    return () => {
      cancelled = true;
    };
  }, [provider.providerId, provider.accountEmail, onLayoutChange]);

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
  // Compact is deliberately a summary row, not a nearly-identical detailed
  // card. Keep only the primary quota and omit secondary diagnostics below.
  const visibleMetrics = compactMetrics ? metrics.slice(0, 1) : metrics;

  // The weekly usage forecast is folded INTO the dedicated pace ("进度") block
  // below (rather than shown as its own box) whenever that block renders — see
  // `suppressForecast`. Compute the estimate here so the pace block can present
  // the runway hours next to the pace bar instead of duplicating them.
  const weeklyForecast = compactMetrics
    ? null
    : (() => {
        const weekly = metrics.find(
          (m) =>
            m.snap.windowMinutes != null &&
            m.snap.windowMinutes >= WEEKLY_WINDOW_MINUTES &&
            !m.snap.isExhausted,
        );
        return weekly ? getPaceEstimate(weekly.snap) : null;
      })();

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
  const cardClassName = [
    "menu-card",
    provider.error ? "menu-card--error" : null,
    hasDetails ? "menu-card--with-details" : "menu-card--header-only",
  ]
    .filter(Boolean)
    .join(" ");

  return (
    <article className={cardClassName}>
      <header className="menu-card__header">
        <div className="menu-card__title-row">
          {showProviderIcon && (
            <ProviderIcon
              providerId={provider.providerId}
              size={18}
              className="menu-card__provider-icon"
            />
          )}
          <div className="menu-card__name-group">
            <span className="menu-card__name">{provider.displayName}</span>
          </div>
          {!provider.error && (
            <span className="menu-card__subtitle menu-card__updated">
              {Number.isNaN(Date.parse(provider.updatedAt))
                ? provider.updatedAt
                : formatRelativeUpdated(Date.parse(provider.updatedAt), t)}
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

      {hasDetails && <div className="menu-card__divider" />}

      {hasDetails && (
        <div className="menu-card__content">
          {!provider.error && hasMetrics && (
            <section className="menu-card__group menu-card__metrics">
              {visibleMetrics.map((m, idx) => (
                <MetricRow
                  key={m.id}
                  title={m.label}
                  snap={m.snap}
                  exhaustedLabel={t("DetailWindowExhausted")}
                  resetTimeRelative={resetTimeRelative}
                  showAsUsed={showAsUsed}
                  hero={idx === 0}
                  planLabel={idx === 0 && !suppressPlanBadge ? planName : null}
                  suppressForecast={hasPace}
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

          {hasPace && provider.pace && (
            <section className="menu-card__group menu-card__pace">
              <div className="menu-card__pace-header">
                <span className="menu-card__pace-title">
                  <GaugeIcon />
                  {t("DetailPaceTitle")}
                </span>
                <span
                  className="menu-card__pace-chip"
                  data-pace={paceCategory(provider.pace.stage)}
                >
                  <TrendIcon />
                  {t(paceStageKey(provider.pace.stage))} (
                  {provider.pace.deltaPercent >= 0 ? "+" : ""}
                  {provider.pace.deltaPercent.toFixed(1)}%)
                </span>
              </div>
              {/* One track: the actual-usage fill, with a slim marker at the
                  "expected by now" position — cleaner than two stacked bars. */}
              <div className="menu-card__pace-track" title={t("PanelActual")}>
                <div
                  className="menu-card__pace-fill"
                  data-pace={paceCategory(provider.pace.stage)}
                  style={{ width: `${provider.pace.actualUsedPercent.toFixed(1)}%` }}
                />
                <span
                  className="menu-card__pace-marker"
                  style={{ left: `${provider.pace.expectedUsedPercent.toFixed(1)}%` }}
                  title={t("PanelExpected")}
                  aria-hidden
                />
              </div>
              {/* Runway line — the weekly usage forecast merged in: status on
                  the left, the estimated hours on the right. */}
              {(() => {
                const lasts = weeklyForecast
                  ? weeklyForecast.lastsUntilReset
                  : provider.pace.willLastToReset;
                const hours =
                  weeklyForecast?.hoursRemaining ??
                  (provider.pace.etaSeconds != null
                    ? provider.pace.etaSeconds / 3600
                    : null);
                const formatHours = (h: number) =>
                  h < 1
                    ? t("PanelForecastLessThanHour")
                    : `${h < 10 ? Math.round(h * 10) / 10 : Math.round(h)} ${t("PanelForecastHoursUnit")}`;
                return (
                  <div
                    className="menu-card__pace-runway"
                    data-state={lasts ? "ok" : "warn"}
                  >
                    <span className="menu-card__pace-runway-status">
                      {lasts ? <CheckIcon /> : <WarnIcon />}
                      {lasts
                        ? t("DetailPaceWillLastToReset")
                        : t("DetailPaceRunsOutIn")}
                    </span>
                    {hours != null && (
                      <strong className="menu-card__pace-runway-value">
                        ≈ {formatHours(hours)}
                      </strong>
                    )}
                  </div>
                );
              })()}
            </section>
          )}

          {(hasMetrics || hasCost || hasPace) && hasCharts && (
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
