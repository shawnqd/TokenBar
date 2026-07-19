import { useCallback, useEffect, useState } from "react";
import type {
  DailyCostPoint,
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
import {
  isMeaningfulQuotaWindow,
  ProviderQuotaBlock,
  quotaWindowLabel,
} from "./ProviderQuotaBlock";

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
}

function OutputSpeedHighlight({
  speed,
  t,
}: {
  speed: ProviderOutputSpeed;
  t: (key: LocaleKey) => string;
}) {
  if (speed.tokensPerSecond == null || speed.tokensPerSecond <= 0) return null;
  return (
    <div className="menu-card__speed" data-status={speed.status}>
      <div>
        <span className="menu-card__speed-label">{t("OutputSpeedTitle")}</span>
        <div>
          <span className="menu-card__speed-value">{speed.tokensPerSecond.toFixed(1)}</span>
          <span className="menu-card__speed-unit">t/s</span>
        </div>
      </div>
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
  const { t } = useLocale();
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
          empty: t("PanelNoUsageToday"),
        }
      : period === "30d"
        ? {
            label: t("PanelThirtyDayUsage"),
            tokens: summary.thirtyDayTokens,
            cost: summary.thirtyDayCost,
            empty: t("PanelNoUsageThirtyDays"),
          }
        : {
            label: t("PanelSevenDayUsage"),
            tokens: summary.sevenDayTokens,
            cost: summary.sevenDayCost,
            empty: t("PanelNoUsageSevenDays"),
          };
  return (
    <section className="menu-card__group menu-card__local-usage">
      <div className="menu-card__local-period">
        <span className="menu-card__local-label">{lead.label}</span>
        {lead.tokens != null && lead.tokens > 0 ? (
          <>
            <div className="menu-card__local-token-value">
              <strong>{formatTokenCount(lead.tokens)}</strong>
              <span>{t("PanelTokenUnit")}</span>
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

      {summary.topModel && (
        <div className="menu-card__local-note">
          <strong>{t("PanelTopModelPrefix")}: {summary.topModel}</strong>
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
}: {
  title: string;
  snap: RateWindowSnapshot;
  exhaustedLabel: string;
  resetTimeRelative: boolean;
  showAsUsed: boolean;
  hero?: boolean;
  planLabel?: string | null;
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
      {paceView.kind === "forecast" && (
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

          {(hasMetrics || hasCost) && hasPace && <div className="menu-card__divider" />}

          {hasPace && provider.pace && (
            <section className="menu-card__group menu-card__pace">
              <div className="menu-card__pace-header">
                <span className="menu-card__group-title">{t("DetailPaceTitle")}</span>
                <span
                  className="menu-card__pace-label"
                  data-pace={paceCategory(provider.pace.stage)}
                >
                  {t(paceStageKey(provider.pace.stage))} (
                  {provider.pace.deltaPercent >= 0 ? "+" : ""}
                  {provider.pace.deltaPercent.toFixed(1)}%)
                </span>
              </div>
              <div className="menu-card__pace-bars">
                <div className="menu-card__pace-track" title={t("PanelExpected")}>
                  <div
                    className="menu-card__pace-fill menu-card__pace-fill--expected"
                    style={{ width: `${provider.pace.expectedUsedPercent.toFixed(1)}%` }}
                  />
                </div>
                <div className="menu-card__pace-track" title={t("PanelActual")}>
                  <div
                    className="menu-card__pace-fill"
                    data-pace={paceCategory(provider.pace.stage)}
                    style={{ width: `${provider.pace.actualUsedPercent.toFixed(1)}%` }}
                  />
                </div>
              </div>
              {provider.pace.etaSeconds != null && !provider.pace.willLastToReset && (
                <div className="menu-card__pace-eta">
                  ⚠{" "}
                  {t("DetailPaceRunsOutIn").replace(
                    "{}",
                    String(Math.round(provider.pace.etaSeconds / 3600)),
                  )}
                </div>
              )}
              {provider.pace.willLastToReset && (
                <div className="menu-card__pace-ok">
                  ✓ {t("DetailPaceWillLastToReset")}
                </div>
              )}
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
