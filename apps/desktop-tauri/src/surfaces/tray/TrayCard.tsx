import { useMemo } from "react";
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
import { fromBridge, projectSurface, type ProjectedWindow, type ProviderSnapshot } from "../../core";
import { useLocale } from "../../hooks/useLocale";
import { useResetDisplay } from "../../hooks/useFormattedResetTime";

import {
  forecastMarkerPercent,
  quotaForecastDisplay,
  quotaPercentDisplay,
  type QuotaDisplayContext,
} from "../../lib/quotaDisplay";
import { formatRelativeUpdated } from "../../lib/relativeTime";
import { getProviderBalance } from "../../lib/providerBalance";
import { coreSnapshotToBridge } from "../../lib/trayProviders";
import { ProviderIcon } from "../../components/providers/ProviderIcon";
import {
  quotaWindowLabel,
} from "../../components/ProviderQuotaBlock";
import type { LocaleKey } from "../../i18n/keys";
import "./tray-v5.css";

/* ── Public contract (imported by the Settings tray-panel preview) ─────── */

export interface TrayCardProps {
  /** Unified core snapshot; the bridge shape is still accepted for callers
   *  outside the core read model (Settings page preview) and normalized via
   *  `fromBridge` internally. */
  provider: ProviderSnapshot | ProviderUsageSnapshot;
  /** detailed | compact | minimal — the overview obeys settings; the detail
   *  view is always "detailed" (drives hero / full-width secondary / 2-col). */
  densityMode: MenuBarDisplayMode;
  /** The owning surface's quota presentation:
   *  quotaDisplayContext(settings, "dashboard"). */
  display: QuotaDisplayContext;
  /** Most recent completed-response speed for providers that expose it,
   *  injected from the core enrichment read model. */
  outputSpeed?: ProviderOutputSpeed | null;
  /** Which period the insight "near usage" row leads with. */
  localUsagePeriod?: LocalUsagePeriod;
  /** Show the provider brand icon in the card header. */
  showProviderIcon?: boolean;
  /** True renders the capability-gated 控制台 / 状态监控 buttons. */
  detail?: boolean;
  /** Committed chart/local-usage enrichment result injected by the owning
   *  surface; null/absent hides the usage slot (capability-gated). */
  chartData?: ProviderChartData | null;
  onOpenExternalUsage?: (providerId: string) => void;
  onOpenExternalStatus?: (providerId: string) => void;
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

/* ── Window projection (core projectSurface + legacy exclusions) ─────── */

/** Accept both shapes so the Settings-page preview keeps compiling unchanged;
 *  the core snapshot is the primary read model for the tray flyout. */
function isCoreSnapshot(
  provider: ProviderSnapshot | ProviderUsageSnapshot,
): provider is ProviderSnapshot {
  return Array.isArray((provider as ProviderSnapshot).windows);
}

/** The projection filters balance/synthetic/informational rows itself; these
 *  two tray-only exclusions carry over from the legacy card: date-only renewal
 *  markers (no measurable cycle) and the old zen-balance / reset-credits
 *  carrier ids that the balance block renders instead of quota tiles. */
function isLegacyHiddenWindow(
  window: ProviderSnapshot["windows"][number],
): boolean {
  if (window.id === "zen-balance" || window.id === "reset-credits") return true;
  if (
    window.windowMinutes == null &&
    window.usedPercent === 0 &&
    window.resetsAt != null
  ) {
    return true;
  }
  return false;
}

function toRateWindow(window: ProjectedWindow): RateWindowSnapshot {
  return {
    usedPercent: window.usedPercent,
    remainingPercent: window.remainingPercent,
    kind: window.kind,
    windowMinutes: window.windowMinutes,
    resetsAt: window.resetsAt,
    resetDescription: window.resetDescription,
    isExhausted: window.isExhausted,
    isInformational: window.isInformational,
    reservePercent: null,
    reserveDescription: null,
  };
}

interface CardWindowView {
  id: string;
  label: string;
  snap: RateWindowSnapshot;
}

/** The projection keeps raw labels; the card re-applies the same localized
 *  label rules the legacy gatherWindows used so English slot names never leak
 *  into the UI and identical words render for the same cycle kinds. */
function legacyWindowLabel(
  window: ProjectedWindow,
  bridge: ProviderUsageSnapshot,
  t: (key: LocaleKey) => string,
): string {
  const snap = toRateWindow(window);
  if (window.id === "primary") {
    return quotaWindowLabel(bridge.primaryLabel, snap, t);
  }
  if (window.id === "secondary") {
    return quotaWindowLabel(bridge.secondaryLabel, snap, t);
  }
  if (window.id === "modelSpecific") {
    return t("DetailWindowModelSpecific");
  }
  if (window.id === "tertiary") {
    return quotaWindowLabel("monthly", snap, t);
  }
  const extra = bridge.extraRateWindows.find((row) => row.id === window.id);
  return quotaWindowLabel(extra?.title ?? window.label, snap, t);
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

/** Short cycle label used inside quota tiles over the full translated word:
 *  周额度 → 周, 月额度 → 月, 5 小时额度 → 5h. Exact matches only — we must not
 *  truncate an unrelated label that happens to share a first character. */
function shortTileLabel(raw: string): string {
  const trimmed = raw.trim();
  if (trimmed === "5h" || trimmed === "5 小时额度" || trimmed === "5h 额度") return "5h";
  if (trimmed === "周额度") return "周";
  if (trimmed === "月额度") return "月";
  return trimmed;
}

/** Map the app's UI language to a BCP-47 code for date/number formatting. */
function localeCodeFor(language: Language): string {
  switch (language) {
    case "chinese": return "zh-CN";
    case "chinesetraditional": return "zh-TW";
    case "japanese": return "ja-JP";
    case "korean": return "ko-KR";
    case "spanish": return "es-MX";
    default: return "en-US";
  }
}

/** Compact absolute reset for tiles — "8/24 7:59" in the UI language's locale,
 *  no day words and no "重置于" text, so label + reset share one line. */
function compactResetText(resetsAt: string | null, language: Language): string | null {
  if (!resetsAt) return null;
  const target = new Date(resetsAt);
  if (Number.isNaN(target.getTime())) return null;
  try {
    const date = new Intl.DateTimeFormat(localeCodeFor(language), {
      month: "numeric",
      day: "numeric",
      hour: "numeric",
      minute: "2-digit",
    }).format(target);
    return date;
  } catch {
    return target.toISOString().slice(0, 16).replace("T", " ");
  }
}

/** The hero/deficit pace badge tail — "· 够用到重置" (reserve) or
 *  "· 约可用 81h" (deficit), built once so every surface shares one wording.
 *
 *  The zh-CN/zh-TW strings carry the wordy "按当前速度" prefix and a "小时"
 *  unit; the compact forms strip the prefix and switch to a short "h" so the
 *  hero line fits. Other locales keep their intact FTL string ("Lasts to reset
 *  at this rate" / "At this rate, about 81h") — the substring trims are
 *  language-specific and must not run on translations that lack them. */
function paceRunwayText(
  tone: PaceTone,
  forecast: ReturnType<typeof quotaForecastDisplay>,
  t: (key: LocaleKey) => string,
  language: Language,
): string | null {
  const isCjk = language === "chinese" || language === "chinesetraditional";
  if (tone === "reserve" && forecast.lastsToReset) {
    const phrase = t("DetailPaceWillLastToReset");
    return ` · ${isCjk ? phrase.replace("按当前速度", "") : phrase}`;
  }
  if (tone === "deficit" && forecast.etaSeconds != null && forecast.etaSeconds > 0) {
    const etaHours = forecast.etaSeconds / 3600;
    const hours =
      etaHours < 1
        ? (isCjk ? t("PanelForecastLessThanHour").replace("小时", "h") : t("PanelForecastLessThanHour"))
        : `${etaHours < 10 ? Math.round(etaHours * 10) / 10 : Math.round(etaHours)}h`;
    const phrase = t("DetailPaceRunsOutIn");
    return ` · ${isCjk ? phrase.replace("按当前速度", "") : phrase} ${hours}`;
  }
  return null;
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
  const { t, language } = useLocale();
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
      const runway = paceRunwayText(tone, forecast, t, language);
      if (runway) badge += runway;
    }
  }
  return (
    <div className="quota-row">
      <div className="quota-row__head">
        <span className="quota-row__label">{title}</span>
        {reset.text && (
          <span className="quota-row__reset" data-reset-state={reset.kind}>
            {reset.text.includes("重置") ? reset.text : `重置：${reset.text}`}
          </span>
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
  pace,
  fullWidth = false,
}: {
  label: string;
  snap: RateWindowSnapshot;
  display: QuotaDisplayContext;
  pace: PaceSnapshot | null;
  fullWidth?: boolean;
}) {
  const { t, language } = useLocale();
  const percent = quotaPercentDisplay(snap, display);
  const reset = useResetDisplay(snap.resetsAt, snap.resetDescription, display.resetTimeRelative);
  // Weekly tile gets the bridge's weekly pace; other cycles project linearly.
  // Only the bar notch is drawn (no text badge) so the forecast is a visual
  // hint, not another line of copy. On-pace windows draw nothing — the notch
  // exists to call out 结余/超支, an invisible onpace marker is noise.
  const forecast = quotaForecastDisplay(snap.kind === "weekly" ? pace : null, snap);
  const notchTone = forecast.available ? paceToneOf(forecast) : null;
  const notch =
    notchTone !== null && notchTone !== "onpace" ? forecastMarkerPercent(forecast, display) : null;
  // Full-width tiles (the secondary cycle): label + reset share the head line
  // (weight tells them apart), bar below — density-preview.html full row.
  if (fullWidth) {
    return (
      <div className="quota-tile quota-tile--full">
        <div className="quota-tile__head">
          <span className="quota-tile__title">
            <span className="quota-tile__label" title={label}>{shortTileLabel(label)}</span>
            <span className="quota-tile__reset" data-reset-state={reset.kind}>
              {compactResetText(snap.resetsAt, language) ?? (reset.kind !== "unknown" ? reset.text : "")}
            </span>
          </span>
          <strong className="quota-tile__val">{percent.rounded}%</strong>
        </div>
        <div className="progress-bar progress-bar--tile">
          <div
            className="progress-fill"
            data-level={percent.level}
            style={{ width: `${percent.fillPercent}%` } as CSSProperties}
          />
          {notch != null && notchTone && (
            <div
              className={`progress-notch progress-notch--${notchTone}`}
              style={{ left: `${notch}%` } as CSSProperties}
              title={t("PanelExpected")}
            />
          )}
        </div>
      </div>
    );
  }
  // Half-width extras: label + pct on the head line, reset below the bar.
  return (
    <div className="quota-tile">
      <div className="quota-tile__head">
        <span className="quota-tile__title">
          <span className="quota-tile__label" title={label}>{shortTileLabel(label)}</span>
          <span className="quota-tile__reset" data-reset-state={reset.kind}>
            {compactResetText(snap.resetsAt, language) ?? (reset.kind !== "unknown" ? reset.text : "")}
          </span>
        </span>
        <strong className="quota-tile__val">{percent.rounded}%</strong>
      </div>
      <div className="progress-bar progress-bar--tile">
        <div
          className="progress-fill"
          data-level={percent.level}
          style={{ width: `${percent.fillPercent}%` } as CSSProperties}
        />
        {notch != null && notchTone && (
          <div
            className={`progress-notch progress-notch--${notchTone}`}
            style={{ left: `${notch}%` } as CSSProperties}
            title={t("PanelExpected")}
          />
        )}
      </div>
    </div>
  );
}

/* ── Balance / status blocks (never fabricate) ────────────────────────── */

function BalanceBlock({ provider, isCompact }: { provider: ProviderUsageSnapshot; isCompact?: boolean }) {
  const { balance } = getProviderBalance(provider);
  if (!balance) return null;
  if (isCompact) {
    return (
      <div className="balance-compact-row" data-balance-kind={balance.kind}>
        <div className="balance-compact-row__left">
          <span className="balance-compact-row__label">{balance.title}:</span>
          <span
            className="balance-compact-row__amount"
            data-unavailable={balance.unavailable ? "true" : undefined}
          >
            {balance.amount}
          </span>
          {balance.breakdown ? <span className="balance-compact-row__sub">{balance.breakdown}</span> : null}
        </div>
        <span className="soft-badge soft-badge--reserve">正常</span>
      </div>
    );
  }
  return (
    <div className="balance-block" data-balance-kind={balance.kind}>
      <div className="balance-block__head">
        <span>{balance.title}</span>
      </div>
      <div className="balance-block__body">
        <span
          className="balance-block__amount"
          data-unavailable={balance.unavailable ? "true" : undefined}
        >
          {balance.amount}
        </span>
        {balance.breakdown && (
          <span className="balance-block__sub soft-badge soft-badge--neutral">{balance.breakdown}</span>
        )}
      </div>
    </div>
  );
}

function StatusBlock({ headline }: { headline: string }) {
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

/** Condensed "周58%·月45%" chip text for the minimal tier's secondary +
 *  extra windows on one line; null when there is nothing to summarize. */
function condensedChipText(
  windows: CardWindowView[],
  display: QuotaDisplayContext,
): string | null {
  if (windows.length === 0) return null;
  const parts = windows.map((w) => {
    const name = w.snap.kind === "weekly" ? "周"
      : w.snap.kind === "monthly" ? "月"
      : w.snap.kind === "daily" ? "日"
      : w.snap.kind === "session" ? "5h"
      : w.label.trim();
    return `${name}${quotaPercentDisplay(w.snap, display).rounded}%`;
  });
  return parts.join("·");
}

function MinimalCard({
  providerId,
  displayName,
  pace,
  display,
  hasStatus,
  outputSpeedText,
  balanceText,
  hero,
  condensedChip,
  showProviderIcon,
  hasOutputSpeed,
}: {
  providerId: string;
  displayName: string;
  pace: PaceSnapshot | null;
  display: QuotaDisplayContext;
  hasStatus: boolean;
  outputSpeedText: string | null;
  balanceText: string | null;
  hero: CardWindowView | null;
  condensedChip: string | null;
  showProviderIcon: boolean;
  hasOutputSpeed: boolean;
}) {
  const { t } = useLocale();
  const name = displayName.split(" ")[0];
  // Sublabel: the hero window's short cycle name, or "余额-ish" when the only
  // reading is a balance amount we can show as the metric.
  const heroIsReal = hero != null && !hero.snap.isInformational;
  const metric = heroIsReal
    ? `${quotaPercentDisplay(hero!.snap, display).rounded}%`
    : balanceText
      ? balanceText
      : null;
  const subLabel = heroIsReal ? hero!.label : balanceText ? displayName.split(" ")[0] : "";
  const hasBar = heroIsReal;

  // Pace badge from the hero forecast.
  let paceBadge: string | null = null;
  let paceTone: PaceTone = "onpace";
  // Pace notch on the minimal bar (HTML renderMinimalStreamlined's notchLeft).
  let notchLeft: number | null = null;
  if (heroIsReal) {
    const forecast = quotaForecastDisplay(hero!.snap.kind === "weekly" ? pace : null, hero!.snap);
    const marker = forecastMarkerPercent(forecast, display);
    notchLeft = marker;
    paceTone = paceToneOf(forecast);
    if (forecast.available) {
      const delta = Math.abs(forecast.deltaPercent ?? 0);
      if (paceTone === "onpace") paceBadge = t("QuotaPaceOnPace");
      else paceBadge = `${delta.toFixed(1)}% ${paceTone === "reserve" ? t("QuotaPaceInReserve") : t("QuotaPaceInDeficit")}`;
    }
  } else {
    // No quota window: surface the balance breakdown or a bare healthy dot.
    const breakdown = hasStatus ? "●" : null;
    paceBadge = breakdown ?? null;
  }

  return (
    <div className="minimal-streamlined">
      <div className="minimal-streamlined__row1">
        <span className="minimal-streamlined__title">
          {showProviderIcon && (
            <ProviderIcon
              providerId={providerId}
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
          {notchLeft != null && (
            <div
              className={`progress-notch progress-notch--${paceTone}`}
              style={{ left: `${notchLeft}%` } as CSSProperties}
            />
          )}
        </div>
      )}
      <div className="minimal-streamlined__row2">
        <span className="minimal-streamlined__badges">
          {paceBadge ? <span className={`soft-badge soft-badge--${paceTone}`}>{paceBadge}</span> : null}
          {condensedChip ? <span className="soft-badge soft-badge--neutral">{condensedChip}</span> : null}
        </span>
        {hasOutputSpeed && outputSpeedText && (
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
  chartData = null,
  onOpenExternalUsage,
  onOpenExternalStatus,
}: TrayCardProps) {
  const { t, language } = useLocale();

  const core = useMemo(
    () => (isCoreSnapshot(provider) ? provider : fromBridge(provider)),
    [provider],
  );
  const bridge = useMemo(
    () => (isCoreSnapshot(provider) ? coreSnapshotToBridge(provider) : provider),
    [provider],
  );

  const displayWindows = useMemo(
    () => core.windows.filter((window) => !isLegacyHiddenWindow(window)),
    [core.windows],
  );
  const projection = useMemo(
    () =>
      projectSurface(
        { ...core, windows: displayWindows },
        { showAsUsed: display.showAsUsed },
      ),
    [core, displayWindows, display.showAsUsed],
  );
  const caps = projection.capabilities;
  const hasOutputSpeed = caps.supportsOutputSpeed;
  const hasLocalUsage = caps.supportsCharts || caps.supportsLocalCost;

  const balanceInfo = useMemo(() => getProviderBalance(bridge), [bridge]);
  const views = useMemo<CardWindowView[]>(
    () =>
      projection.layers.quota.map((window) => ({
        id: window.id,
        label: legacyWindowLabel(window, bridge, t),
        snap: toRateWindow(window),
      })),
    [projection.layers.quota, bridge, t],
  );
  const hero = views[0] ?? null;
  const secondary = views[1] ?? null;
  const extraTiles = useMemo(() => {
    const rest = views.slice(2);
    const count = rest.length;
    return rest.map((w, idx) => ({
      ...w,
      fullWidth: count === 1 || (count > 1 && idx === count - 1 && count % 2 === 1),
    }));
  }, [views]);

  const localUsage = core.error ? null : chartData?.localUsage ?? null;
  const usageLead = localUsage ? resolveLocalUsageLead(localUsagePeriod, localUsage) : null;

  const speedValid =
    outputSpeed != null && outputSpeed.tokensPerSecond != null && outputSpeed.tokensPerSecond > 0;
  const outputSpeedText = speedValid ? `${outputSpeed!.tokensPerSecond!.toFixed(1)} t/s` : null;

  const balanceText = balanceInfo.balance ? balanceInfo.balance.amount : null;
  const statusHeadline =
    (core.trayStatusLabel?.trim() || projection.telemetry?.gatewayStatus || null);
  const hasStatus = statusHeadline != null;

  // Context buttons only exist in the detail view, gated by the core
  // capability flags (no provider-name branches).
  const canDashboard = detail && caps.supportsProviderDashboard;
  const canStatus = detail && caps.supportsStatusPage;
  const hasContext = canDashboard || canStatus;

  const condensedChip = condensedChipText(
    [secondary, ...extraTiles].filter((w): w is CardWindowView => w != null),
    display,
  );

  // Minimal tier: no card-header / card-zone split — a two-row streamlined
  // card (spec 5.4).
  if (densityMode === "minimal") {
    return (
      <div className="tray-card tray-card--minimal" id={`card-${core.providerId}`}>
        <MinimalCard
          providerId={core.providerId}
          displayName={core.displayName}
          pace={core.pace}
          display={display}
          hasStatus={hasStatus}
          outputSpeedText={outputSpeedText}
          balanceText={balanceText}
          hero={hero}
          condensedChip={condensedChip}
          showProviderIcon={showProviderIcon}
          hasOutputSpeed={hasOutputSpeed}
        />
        {hasContext && (
          <div className={`context-actions${canDashboard && canStatus ? "" : " context-actions--single"}`}>
            {canDashboard && (
              <button type="button" className="context-btn" onClick={() => onOpenExternalUsage?.(core.providerId)}>
                <ChartIcon />{t("ActionUsageDashboard")}
              </button>
            )}
            {canStatus && (
              <button type="button" className="context-btn" onClick={() => onOpenExternalStatus?.(core.providerId)}>
                <StatBarsIcon />{t("ActionStatusPage")}
              </button>
            )}
          </div>
        )}
      </div>
    );
  }

  const updatedRaw = core.updatedAt == null ||
    Number.isNaN(Date.parse(core.updatedAt))
    ? core.updatedAt ?? ""
    : formatRelativeUpdated(Date.parse(core.updatedAt), t);
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
      id={`card-${core.providerId}`}
      data-detail={detail ? "true" : undefined}
    >
      <div className="card-header">
          <div className="card-header__left">
            {showProviderIcon && (
              <ProviderIcon providerId={core.providerId} size={20} className="card-header__icon" />
            )}
            <span className="card-header__name">{core.displayName}</span>
            <span className="card-header__updated">{updatedText}</span>
          </div>
      </div>

      <div className={`card-zone${zoneTone}`}>
        {hero && (
          <div className={`modular-section quota-stage${sectionClass()}`}>
            <HeroRow
              title={hero.label}
              snap={hero.snap}
              display={display}
              pace={core.pace}
              windowKind={hero.snap.kind}
            />
            {secondary && (
              <QuotaTile label={secondary.label} snap={secondary.snap} display={display} pace={core.pace} fullWidth />
            )}
          </div>
        )}

        {extraTiles.length > 0 && (
          <div className={`modular-section${sectionClass()}`}>
            <div className="tiles-grid-2col">
              {extraTiles.map((tile) => (
                <QuotaTile
                  key={tile.id}
                  label={tile.label}
                  snap={tile.snap}
                  display={display}
                  pace={core.pace}
                  fullWidth={tile.fullWidth}
                />
              ))}
            </div>
          </div>
        )}

        {balanceInfo.balance && (
          <div className={`modular-section${sectionClass()}`}>
            <BalanceBlock provider={bridge} isCompact={isCompact} />
          </div>
        )}

        {!hero && !balanceInfo.balance && hasStatus && (
          <div className={`modular-section${sectionClass()}`}>
            <StatusBlock headline={statusHeadline} />
          </div>
        )}

        {/* Unified insight footer: speed / near usage / API-value subline.
            Detailed = HTML meta-well (filled, two centered halves + note);
            compact = dual chips. Only renders when the provider actually has
            one of these capabilities — no empty slot for providers we cannot
            measure. */}
        {hasOutputSpeed || hasLocalUsage ? (
        <div className={`modular-section${isCompact ? sectionClass() : ""}`}>
          {!isCompact ? (
            <div className="meta-well">
              {hasOutputSpeed || hasLocalUsage ? (
              <div className="meta-well__line">
                {hasOutputSpeed && (
                  <div className="meta-well__item">
                    <span className="meta-well__label">{t("TaskbarWidgetPreviewSpeed")}</span>
                    <span className="meta-well__value" data-slot="speed">{outputSpeedText ?? ""}</span>
                  </div>
                )}
                {hasOutputSpeed && hasLocalUsage && (
                  <span className="meta-well__rule" aria-hidden="true" />
                )}
                {hasLocalUsage && (
                  <div className="meta-well__item">
                    <span className="meta-well__label">{t(usageLead ? usageLead.labelKey : "PanelSevenDayUsage").replace(/使用$/, "")}</span>
                    <span className="meta-well__value" data-slot="usage">
                      {usageLead && usageLead.tokens != null && usageLead.tokens > 0
                        ? formatApproxTokens(usageLead.tokens, language)
                        : ""}
                    </span>
                  </div>
                )}
              </div>
              ) : null}
              {hasLocalUsage && usageLead && usageLead.cost != null && (
                <div className="meta-well__note">
                  {t("PanelApiEquivalentValue")} ≈ {formatApiEquivalentValue(usageLead.cost)}
                  {usageLead.topModel ? ` · ${usageLead.topModel}` : ""}
                </div>
              )}
            </div>
          ) : (
            <div className="compact-chips-row">
              {hasOutputSpeed && (
                <span className="compact-chip">{t("TaskbarWidgetPreviewSpeed")} <strong>{outputSpeedText ?? ""}</strong></span>
              )}
              {hasLocalUsage && (
                <span className="compact-chip">{t(usageLead ? usageLead.labelKey : "PanelSevenDayUsage").replace(/使用$/, "")} <strong>
                  {usageLead && usageLead.tokens != null && usageLead.tokens > 0
                    ? formatCompactTokens(usageLead.tokens, language) ?? formatTokenCount(usageLead.tokens)
                    : ""}
                </strong></span>
              )}
            </div>
          )}
        </div>
        ) : null}
      </div>

      {hasContext && (
        <div className={`context-actions${canDashboard && canStatus ? "" : " context-actions--single"}`}>
          {canDashboard && (
            <button type="button" className="context-btn" onClick={() => onOpenExternalUsage?.(core.providerId)}>
              <ChartIcon />{t("ActionUsageDashboard")}
            </button>
          )}
          {canStatus && (
            <button type="button" className="context-btn" onClick={() => onOpenExternalStatus?.(core.providerId)}>
              <StatBarsIcon />{t("ActionStatusPage")}
            </button>
          )}
        </div>
      )}
    </div>
  );
}