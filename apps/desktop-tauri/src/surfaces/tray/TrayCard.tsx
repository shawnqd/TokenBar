import { useEffect, useMemo, useState } from "react";
import type { CSSProperties } from "react";
import type {
  Language,
  LocalUsagePeriod,
  MenuBarDisplayMode,
  PaceSnapshot,
  ProviderChartData,
  ProviderLocalUsageSummary,
  ProviderOutputSpeed,
  ProviderUsageSnapshot,
  RateWindowSnapshot,
} from "../../types/bridge";
import { useLocale } from "../../hooks/useLocale";
import { useResetDisplay } from "../../hooks/useFormattedResetTime";
import { getProviderChartData, openProviderDashboard, openProviderStatusPage } from "../../lib/tauri";
import {
  forecastMarkerPercent,
  quotaForecastDisplay,
  quotaPercentDisplay,
  type QuotaDisplayContext,
} from "../../lib/quotaDisplay";
import { formatRelativeUpdated } from "../../lib/relativeTime";
import { providerSupportsChartData } from "../../lib/providerCharts";
import { getProviderBalance } from "../../lib/providerBalance";
import { ProviderIcon } from "../../components/providers/ProviderIcon";
import {
  isMeaningfulQuotaWindow,
  quotaWindowLabel,
} from "../../components/ProviderQuotaBlock";
import type { LocaleKey } from "../../i18n/keys";
import { HAS_DASHBOARD, HAS_STATUS_PAGE } from "./providerCapabilities";
import "./tray-v5.css";

/* ── Public contract (imported by the Settings tray-panel preview) ─────── */

export interface TrayCardProps {
  provider: ProviderUsageSnapshot;
  /** detailed | compact | minimal — the overview obeys settings; the detail
   *  view is always "detailed" (drives hero / full-width secondary / 2-col). */
  densityMode: MenuBarDisplayMode;
  /** The owning surface's quota presentation:
   *  quotaDisplayContext(settings, "dashboard"). */
  display: QuotaDisplayContext;
  /** Most recent completed-response speed for providers that expose it. */
  outputSpeed?: ProviderOutputSpeed | null;
  /** Which period the insight "near usage" row leads with. */
  localUsagePeriod?: LocalUsagePeriod;
  /** Show the provider brand icon in the card header. */
  showProviderIcon?: boolean;
  /** True renders the capability-gated 控制台 / 状态监控 buttons. */
  detail?: boolean;
}

type PaceTone = "reserve" | "deficit" | "onpace";

const ON_PACE_DEAD_BAND = 2;

/** Collapse the shared forecast into the three tones the v5 badge and the
 *  hero colour use. Burning ahead of the clock is a deficit, lagging is a
 *  reserve, within +/-2 points is on pace. */
function paceToneOf(forecast: {
  stage: PaceSnapshot["stage"] | null;
  deltaPercent: number | null;
}): PaceTone {
  if (forecast.stage) {
    if (forecast.stage === "on_track") return "onpace";
    return forecast.stage.endsWith("ahead") ? "deficit" : "reserve";
  }
  const delta = forecast.deltaPercent;
  if (delta == null || Number.isNaN(delta) || Math.abs(delta) <= ON_PACE_DEAD_BAND) {
    return "onpace";
  }
  return delta > 0 ? "deficit" : "reserve";
}

/* ── Window projection (spec 8.3 hard rules) ──────────────────────────── */

interface TrayWindowView {
  id: string;
  label: string;
  snap: RateWindowSnapshot;
  /** Meaningful only for extra tiles: an odd last tile spans both columns. */
  fullWidth: boolean;
}

function windowBucket(kind: RateWindowSnapshot["kind"]): number {
  switch (kind) {
    case "session": return 0;
    case "daily": return 1;
    case "weekly": return 2;
    case "monthly": return 3;
    default: return 4;
  }
}

/** Real windows only: balance carriers (getProviderBalance) are excluded,
 *  informational rows and usageKnown=false windows are dropped, and nothing
 *  fabricated ever becomes a quota row. */
function gatherWindows(
  provider: ProviderUsageSnapshot,
  exclude: Set<"primary" | "secondary">,
  t: (key: LocaleKey) => string,
): TrayWindowView[] {
  const windows: TrayWindowView[] = [];
  const add = (
    id: string,
    snap: RateWindowSnapshot | null,
    label: string,
    usageKnown?: boolean,
  ) => {
    if (!snap) return;
    if (snap.isInformational) return;
    if (usageKnown === false) return;
    if (!isMeaningfulQuotaWindow(snap)) return;
    // Date-only markers are not quota windows: OpenCode Go's "Renews" row
    // (0% used, no cycle length, only a renewal timestamp) must never render
    // as a tile with a fake bar — a real cycle always has a length to measure.
    if (snap.windowMinutes == null && snap.usedPercent === 0) return;
    windows.push({ id, label, snap, fullWidth: false });
  };
  if (!exclude.has("primary")) {
    add("primary", provider.primary, quotaWindowLabel(provider.primaryLabel, provider.primary, t));
  }
  if (provider.secondary && !exclude.has("secondary")) {
    add("secondary", provider.secondary, quotaWindowLabel(provider.secondaryLabel, provider.secondary, t));
  }
  if (provider.modelSpecific) {
    add("model-specific", provider.modelSpecific, t("DetailWindowModelSpecific"));
  }
  if (provider.tertiary) {
    add("tertiary", provider.tertiary, quotaWindowLabel("monthly", provider.tertiary, t));
  }
  for (const extra of provider.extraRateWindows ?? []) {
    if (extra.id === "reset-credits") continue;
    add(`extra-${extra.id}`, extra.window, extra.title, extra.usageKnown);
  }
  return windows;
}

/** Session windows first by windowMinutes asc, then daily → weekly → monthly,
 *  then everything else kept in the provider's given order. */
function sortWindows(windows: TrayWindowView[]): TrayWindowView[] {
  const indexed = windows.map((w, index) => ({ w, index }));
  indexed.sort((a, b) => {
    const ba = windowBucket(a.w.snap.kind);
    const bb = windowBucket(b.w.snap.kind);
    if (ba !== bb) return ba - bb;
    if (ba === 0) {
      const am = a.w.snap.windowMinutes ?? Number.MAX_SAFE_INTEGER;
      const bm = b.w.snap.windowMinutes ?? Number.MAX_SAFE_INTEGER;
      if (am !== bm) return am - bm;
    }
    return a.index - b.index;
  });
  return indexed.map((entry) => entry.w);
}

/* ── Formatting helpers (reuse MenuCard's approach) ───────────────────── */

const USD_TO_CNY_REFERENCE_RATE = 7.2;

function formatCurrency(amount: number, code: string): string {
  try {
    return new Intl.NumberFormat("en-US", { style: "currency", currency: code }).format(amount);
  } catch {
    return `${code} ${amount.toFixed(2)}`;
  }
}

function formatTokenCount(value: number): string {
  return new Intl.NumberFormat("en-US", { maximumFractionDigits: 0 }).format(value);
}

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

function formatApproxTokens(value: number, language: Language): string {
  const compact = formatCompactTokens(value, language);
  const isRounded =
    compact != null && compact.replace(/[\s, ]/g, "") !== String(Math.round(value));
  return isRounded ? `≈ ${compact}` : formatTokenCount(value);
}

function formatApiEquivalentValue(amount: number): string {
  const cnyEstimate = amount * USD_TO_CNY_REFERENCE_RATE;
  return `${formatCurrency(amount, "USD")} · ¥${cnyEstimate.toFixed(2)}`;
}

interface LocalUsageLead {
  labelKey: LocaleKey;
  tokens: number | null;
  cost: number | null;
  topModel: string | null;
}

function resolveLocalUsageLead(
  period: LocalUsagePeriod,
  summary: ProviderLocalUsageSummary,
): LocalUsageLead {
  if (period === "today") {
    return {
      labelKey: "PanelTodayUsage",
      tokens: summary.todayTokens,
      cost: summary.todayCost,
      topModel: summary.todayTopModel,
    };
  }
  if (period === "30d") {
    return {
      labelKey: "PanelThirtyDayUsage",
      tokens: summary.thirtyDayTokens,
      cost: summary.thirtyDayCost,
      topModel: summary.thirtyDayTopModel,
    };
  }
  return {
    labelKey: "PanelSevenDayUsage",
    tokens: summary.sevenDayTokens,
    cost: summary.sevenDayCost,
    topModel: summary.sevenDayTopModel,
  };
}

/* ── SVG icon set (single open-stroked 24×24 family) ──────────────────── */

const iconCommon = {
  fill: "none",
  stroke: "currentColor",
  strokeWidth: 1.85,
  strokeLinecap: "round" as const,
  strokeLinejoin: "round" as const,
  "aria-hidden": true,
  viewBox: "0 0 24 24",
};
interface IconProps { width?: number; height?: number; }
const SpeedIcon = ({ width = 13, height = 13 }: IconProps) => (
  <svg {...iconCommon} width={width} height={height}>
    <circle cx="12" cy="12" r="9" />
    <path d="M12 7v5l3 2" />
  </svg>
);
const UsageIcon = ({ width = 13, height = 13 }: IconProps) => (
  <svg {...iconCommon} width={width} height={height}>
    <path d="M12 2L2 7l10 5 10-5-10-5z" />
    <path d="M2 17l10 5 10-5" />
    <path d="M2 12l10 5 10-5" />
  </svg>
);
const ChartIcon = ({ width = 12, height = 12 }: IconProps) => (
  <svg {...iconCommon} width={width} height={height}>
    <rect x="3" y="12" width="4" height="9" />
    <rect x="10" y="7" width="4" height="14" />
    <rect x="17" y="3" width="4" height="18" />
  </svg>
);
const StatBarsIcon = ({ width = 12, height = 12 }: IconProps) => (
  <svg {...iconCommon} width={width} height={height}>
    <path d="M22 12h-4l-3 9L9 3l-3 9H2" />
  </svg>
);

/* ── Hero row (own reset-hook call per window) ────────────────────────── */

function HeroRow({
  title,
  snap,
  display,
  pace,
  windowKind,
}: {
  title: string;
  snap: RateWindowSnapshot;
  display: QuotaDisplayContext;
  pace: PaceSnapshot | null;
  windowKind: RateWindowSnapshot["kind"];
}) {
  const { t } = useLocale();
  const reset = useResetDisplay(snap.resetsAt, snap.resetDescription, display.resetTimeRelative);
  const forecast = quotaForecastDisplay(windowKind === "weekly" ? pace : null, snap);
  const percent = quotaPercentDisplay(snap, display);
  const tone = paceToneOf(forecast);
  const marker = forecastMarkerPercent(forecast, display);
  const delta = Math.abs(forecast.deltaPercent ?? 0);
  let badge: string | null = null;
  if (forecast.available) {
    if (tone === "onpace") badge = t("QuotaPaceOnPace");
    else {
      badge = `${delta.toFixed(1)}% ${tone === "reserve" ? t("QuotaPaceInReserve") : t("QuotaPaceInDeficit")}`;
      if (tone === "reserve" && forecast.lastsToReset) badge += ` · ${t("DetailPaceWillLastToReset")}`;
    }
  }
  return (
    <div className="quota-row">
      <div className="quota-row__head">
        <span className="quota-row__label">{title}</span>
        {reset.text && (
          <span className="quota-row__reset" data-reset-state={reset.kind}>{reset.text}</span>
        )}
      </div>
      <div className="quota-row__hero-line">
        <span
          className="quota-row__hero-pct"
          data-tone={tone}
          data-exhausted={percent.isExhausted ? "true" : undefined}
        >
          {percent.rounded}%
        </span>
        {badge ? <span className={`soft-badge soft-badge--${tone}`}>{badge}</span> : null}
      </div>
      <div className="progress-bar">
        <div
          className="progress-fill"
          data-level={percent.level}
          style={{ width: `${percent.fillPercent}%` } as CSSProperties}
        />
        {marker != null && (
          <div
            className={`progress-notch progress-notch--${tone}`}
            style={{ left: `${marker}%` } as CSSProperties}
            title={t("PanelExpected")}
          />
        )}
      </div>
    </div>
  );
}

function QuotaTile({
  label,
  snap,
  display,
  fullWidth = false,
}: {
  label: string;
  snap: RateWindowSnapshot;
  display: QuotaDisplayContext;
  fullWidth?: boolean;
}) {
  const percent = quotaPercentDisplay(snap, display);
  return (
    <div className={`quota-tile${fullWidth ? " quota-tile--full" : ""}`}>
      <div className="quota-tile__head">
        <span className="quota-tile__label" title={label}>{label}</span>
        <strong className="quota-tile__val">{percent.rounded}%</strong>
      </div>
      <div className="progress-bar progress-bar--tile">
        <div
          className="progress-fill"
          data-level={percent.level}
          style={{ width: `${percent.fillPercent}%` } as CSSProperties}
        />
      </div>
    </div>
  );
}

/* ── Balance / status blocks (never fabricate) ────────────────────────── */

function BalanceBlock({ provider }: { provider: ProviderUsageSnapshot }) {
  const { balance } = getProviderBalance(provider);
  if (!balance) return null;
  return (
    <div className="balance-block" data-balance-kind={balance.kind}>
      <div className="balance-block__head">
        <span>{balance.title}</span>
        {balance.breakdown ? <span className="soft-badge soft-badge--neutral">{balance.breakdown}</span> : null}
      </div>
      <div className="balance-block__body">
        <span
          className="balance-block__amount"
          data-unavailable={balance.unavailable ? "true" : undefined}
        >
          {balance.amount}
        </span>
        {balance.breakdown && <span className="balance-block__sub">{balance.breakdown}</span>}
      </div>
    </div>
  );
}

function statusSource(provider: ProviderUsageSnapshot): string | null {
  const label = provider.trayStatusLabel?.trim();
  if (label) return label;
  if (provider.wayfinderUsage) return provider.wayfinderUsage.gatewayStatus;
  return null;
}

function StatusBlock({ provider }: { provider: ProviderUsageSnapshot }) {
  const headline = statusSource(provider);
  if (!headline) return null;
  return (
    <div className="status-block">
      <div className="status-headline">
        <span className="status-indicator-dot" />
        <span>{headline}</span>
      </div>
    </div>
  );
}

/* ── Minimal streamlined tier ─────────────────────────────────────────── */

function MinimalCard({
  provider,
  display,
  outputSpeedText,
  usageLead,
  language,
  balanceText,
  hero,
  showProviderIcon,
}: {
  provider: ProviderUsageSnapshot;
  display: QuotaDisplayContext;
  outputSpeedText: string | null;
  usageLead: LocalUsageLead | null;
  language: Language;
  balanceText: string | null;
  hero: TrayWindowView | null;
  showProviderIcon: boolean;
}) {
  const { t } = useLocale();
  const name = provider.displayName.split(" ")[0];
  // Sublabel: the hero window's short cycle name, or "余额-ish" when the only
  // reading is a balance amount we can show as the metric.
  const heroIsReal = hero != null && !hero.snap.isInformational;
  const metric = heroIsReal
    ? `${quotaPercentDisplay(hero!.snap, display).rounded}%`
    : balanceText
      ? balanceText
      : null;
  const subLabel = heroIsReal ? hero!.label : balanceText ? provider.displayName.split(" ")[0] : "";
  const hasBar = heroIsReal;

  // Pace badge from the hero forecast.
  let paceBadge: string | null = null;
  let paceTone: PaceTone = "onpace";
  if (heroIsReal) {
    const forecast = quotaForecastDisplay(hero!.snap.kind === "weekly" ? provider.pace : null, hero!.snap);
    paceTone = paceToneOf(forecast);
    if (forecast.available) {
      const delta = Math.abs(forecast.deltaPercent ?? 0);
      if (paceTone === "onpace") paceBadge = t("QuotaPaceOnPace");
      else paceBadge = `${delta.toFixed(1)}% ${paceTone === "reserve" ? t("QuotaPaceInReserve") : t("QuotaPaceInDeficit")}`;
    }
  } else {
    // No quota window: surface the balance breakdown or a bare healthy dot.
    const breakdown = provider.wayfinderUsage ? "●" : null;
    paceBadge = breakdown ?? null;
  }

  return (
    <div className="minimal-streamlined">
      <div className="minimal-streamlined__row1">
        <span className="minimal-streamlined__title">
          {showProviderIcon && (
            <ProviderIcon
              providerId={provider.providerId}
              size={14}
              className="minimal-streamlined__icon"
            />
          )}
          <span>{name}</span>
          {subLabel ? <span className="minimal-streamlined__sublabel">{subLabel}</span> : null}
        </span>
        {metric && (
          <span
            className="minimal-streamlined__metric"
            data-tone={paceTone === "deficit" ? "deficit" : undefined}
          >
            {metric}
          </span>
        )}
      </div>
      {hasBar && hero && (
        <div className="minimal-streamlined__bar-wrap">
          <div className="progress-fill" style={{ width: `${quotaPercentDisplay(hero.snap, display).fillPercent}%` } as CSSProperties} />
        </div>
      )}
      <div className="minimal-streamlined__row2">
        <span className="minimal-streamlined__badges">
          {paceBadge ? <span className={`soft-badge soft-badge--${paceTone} soft-badge--mini`}>{paceBadge}</span> : null}
          {usageLead && usageLead.tokens != null && usageLead.tokens > 0 ? (
            <span className="soft-badge soft-badge--neutral soft-badge--mini">
              {t(usageLead.labelKey)} {formatApproxTokens(usageLead.tokens, language)}
            </span>
          ) : null}
        </span>
        {outputSpeedText && (
          <span className="minimal-streamlined__speed">{outputSpeedText}</span>
        )}
      </div>
    </div>
  );
}

/* ── Main component ───────────────────────────────────────────────────── */

export default function TrayCard({
  provider,
  densityMode,
  display,
  outputSpeed = null,
  localUsagePeriod = "7d",
  showProviderIcon = true,
  detail = false,
}: TrayCardProps) {
  const { t, language } = useLocale();
  const [chartData, setChartData] = useState<ProviderChartData | null>(null);
  const [isChartDataLoading, setIsChartDataLoading] = useState(false);

  const supportsChart = providerSupportsChartData(provider.providerId);
  useEffect(() => {
    if (!supportsChart) {
      setChartData(null);
      setIsChartDataLoading(false);
      return;
    }
    let cancelled = false;
    setChartData(null);
    setIsChartDataLoading(true);
    getProviderChartData(provider.providerId, provider.accountEmail ?? undefined)
      .then((data) => {
        if (!cancelled) setChartData(data);
      })
      .catch(() => undefined)
      .finally(() => {
        if (!cancelled) setIsChartDataLoading(false);
      });
    return () => { cancelled = true; };
  }, [provider.providerId, provider.accountEmail, supportsChart]);

  const balanceInfo = useMemo(() => getProviderBalance(provider), [provider]);
  const windows = useMemo(
    () => sortWindows(gatherWindows(provider, balanceInfo.excludeWindows, t)),
    [provider, balanceInfo.excludeWindows, t],
  );
  const hero = windows[0] ?? null;
  const secondary = windows[1] ?? null;
  const extraTiles = useMemo(() => {
    const rest = windows.slice(2);
    const count = rest.length;
    return rest.map((w, idx) => ({
      ...w,
      fullWidth: count > 1 && idx === count - 1 && count % 2 === 1,
    }));
  }, [windows]);

  const localUsage = provider.error ? null : chartData?.localUsage ?? null;
  const usageLead =
    localUsage && !isChartDataLoading
      ? resolveLocalUsageLead(localUsagePeriod, localUsage)
      : null;

  const speedValid =
    outputSpeed != null && outputSpeed.tokensPerSecond != null && outputSpeed.tokensPerSecond > 0;
  const outputSpeedText = speedValid ? `${outputSpeed!.tokensPerSecond!.toFixed(1)} t/s` : null;

  const balanceText = balanceInfo.balance ? balanceInfo.balance.amount : null;
  const hasStatus = statusSource(provider) != null;

  // Context buttons only exist in the detail view, gated per capability.
  const canDashboard = detail && HAS_DASHBOARD.has(provider.providerId);
  const canStatus = detail && HAS_STATUS_PAGE.has(provider.providerId);
  const hasContext = canDashboard || canStatus;

  // Minimal tier: no card-header / card-zone split — a two-row streamlined
  // card (spec 5.4).
  if (densityMode === "minimal") {
    return (
      <div className="tray-card tray-card--minimal" id={`card-${provider.providerId}`}>
        <MinimalCard
          provider={provider}
          display={display}
          outputSpeedText={outputSpeedText}
          usageLead={usageLead}
          language={language}
          balanceText={balanceText}
          hero={hero}
          showProviderIcon={showProviderIcon}
        />
        {hasContext && (
          <div className={`context-actions${canDashboard && canStatus ? "" : " context-actions--single"}`}>
            {canDashboard && (
              <button type="button" className="context-btn" onClick={() => void openProviderDashboard(provider.providerId)}>
                <ChartIcon />{t("ActionUsageDashboard")}
              </button>
            )}
            {canStatus && (
              <button type="button" className="context-btn" onClick={() => void openProviderStatusPage(provider.providerId)}>
                <StatBarsIcon />{t("ActionStatusPage")}
              </button>
            )}
          </div>
        )}
      </div>
    );
  }

  const updatedRaw = Number.isNaN(Date.parse(provider.updatedAt))
    ? provider.updatedAt
    : formatRelativeUpdated(Date.parse(provider.updatedAt), t);
  const updatedText =
    densityMode === "compact" ? updatedRaw.replace(/更新$/, "") : updatedRaw;

  const isCompact = densityMode === "compact";
  const zoneTone = isCompact ? " card-zone--compact" : "";

  // Bordered sections: hero baseline block first (no border), then balance /
  // status / insights each get the hairline top edge when preceded by content.
  let hasBlock = false;
  const sectionClass = () => {
    const cls = hasBlock ? " modular-section--bordered" : "";
    hasBlock = true;
    return cls;
  };

  return (
    <div
      className={`tray-card tray-card--${densityMode}`}
      id={`card-${provider.providerId}`}
      data-detail={detail ? "true" : undefined}
    >
      <div className="card-header">
          <div className="card-header__left">
            {showProviderIcon && (
              <ProviderIcon providerId={provider.providerId} size={20} className="card-header__icon" />
            )}
            <span className="card-header__name">{provider.displayName}</span>
          </div>
          <span className="card-header__updated">{updatedText}</span>
      </div>

      <div className={`card-zone${zoneTone}`}>
        {hero && (
          <div className={`modular-section${sectionClass()}`}>
            <HeroRow
              title={hero.label}
              snap={hero.snap}
              display={display}
              pace={provider.pace}
              windowKind={hero.snap.kind}
            />
            {secondary && (
              <div className="quota-tile quota-tile--full">
                <QuotaTile label={secondary.label} snap={secondary.snap} display={display} fullWidth />
              </div>
            )}
          </div>
        )}

        {extraTiles.length > 0 && (
          <div className={`modular-section${sectionClass()}`}>
            <div className="tiles-grid-2col">
              {extraTiles.map((tile) => (
                <div className={`quota-tile${tile.fullWidth ? " quota-tile--full" : ""}`} key={tile.id}>
                  <QuotaTile label={tile.label} snap={tile.snap} display={display} fullWidth={tile.fullWidth} />
                </div>
              ))}
            </div>
          </div>
        )}

        {!hero && balanceInfo.balance && (
          <div className={`modular-section${sectionClass()}`}>
            <BalanceBlock provider={provider} />
          </div>
        )}

        {!hero && !balanceInfo.balance && hasStatus && (
          <div className={`modular-section${sectionClass()}`}>
            <StatusBlock provider={provider} />
          </div>
        )}

        {/* Unified insight footer: speed / near usage / API-value subline. */}
        <div className={`modular-section${sectionClass()}`}>
          {!isCompact ? (
            <div className="insight-section">
              <div className="insight-row">
                <span className="insight-row__label"><SpeedIcon />{t("OutputSpeedTitle")}</span>
                <span className="insight-row__value" data-slot="speed">{outputSpeedText ?? ""}</span>
              </div>
              <div className="insight-row">
                <span className="insight-row__label"><UsageIcon />{t(usageLead ? usageLead.labelKey : "PanelSevenDayUsage")}</span>
                <span className="insight-row__value" data-slot="usage">
                  {usageLead && usageLead.tokens != null && usageLead.tokens > 0
                    ? formatApproxTokens(usageLead.tokens, language)
                    : ""}
                </span>
              </div>
              {usageLead && usageLead.cost != null && (
                <div className="insight-subline">
                  {t("PanelApiEquivalentValue")} ≈ {formatApiEquivalentValue(usageLead.cost)}
                </div>
              )}
            </div>
          ) : (
            <div className="compact-chips-row">
              <span className="compact-chip"><SpeedIcon /> {t("OutputSpeedTitle")} <strong>{outputSpeedText ?? ""}</strong></span>
              <span className="compact-chip"><UsageIcon /> {t(usageLead ? usageLead.labelKey : "PanelSevenDayUsage")} <strong>
                {usageLead && usageLead.tokens != null && usageLead.tokens > 0
                  ? formatCompactTokens(usageLead.tokens, language) ?? formatTokenCount(usageLead.tokens)
                  : ""}
              </strong></span>
            </div>
          )}
        </div>
      </div>

      {hasContext && (
        <div className={`context-actions${canDashboard && canStatus ? "" : " context-actions--single"}`}>
          {canDashboard && (
            <button type="button" className="context-btn" onClick={() => void openProviderDashboard(provider.providerId)}>
              <ChartIcon />{t("ActionUsageDashboard")}
            </button>
          )}
          {canStatus && (
            <button type="button" className="context-btn" onClick={() => void openProviderStatusPage(provider.providerId)}>
              <StatBarsIcon />{t("ActionStatusPage")}
            </button>
          )}
        </div>
      )}
    </div>
  );
}
