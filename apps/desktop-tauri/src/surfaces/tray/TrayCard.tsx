import { useEffect, useMemo, useState } from "react";
import type { CSSProperties } from "react";
import type {
  Language,
  LocalUsagePeriod,
  MenuBarDisplayMode,
  PaceSnapshot,
  ProviderCapabilitiesSnapshot,
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
import { providerCapabilities as resolveCapabilities } from "../../lib/providerCapabilities";
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
    if (snap.windowMinutes == null && snap.usedPercent === 0 && snap.resetsAt != null) return;
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
    if (extra.id === "reset-credits" || extra.id === "zen-balance") continue;
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

/** Condensed "周58%·月45%" chip text for the minimal tier's secondary +
 *  extra windows on one line; null when there is nothing to summarize. */
function condensedChipText(
  windows: TrayWindowView[],
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
  provider,
  display,
  outputSpeedText,
  balanceText,
  hero,
  condensedChip,
  showProviderIcon,
  caps,
}: {
  provider: ProviderUsageSnapshot;
  display: QuotaDisplayContext;
  outputSpeedText: string | null;
  balanceText: string | null;
  hero: TrayWindowView | null;
  condensedChip: string | null;
  showProviderIcon: boolean;
  caps: ProviderCapabilitiesSnapshot;
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
  // Pace notch on the minimal bar (HTML renderMinimalStreamlined's notchLeft).
  let notchLeft: number | null = null;
  if (heroIsReal) {
    const forecast = quotaForecastDisplay(hero!.snap.kind === "weekly" ? provider.pace : null, hero!.snap);
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
        {caps.outputSpeed && outputSpeedText && (
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
  // Stable, provider-level capability flags (backend-reported; deterministic
  // fallback for older snapshots). Slot visibility must depend on these, not
  // on whether chart data has finished loading — otherwise a card flickers its
  // speed/usage rows in and out as fetches settle.
  const caps = resolveCapabilities(provider);
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
  const opencodeGo = provider.providerId === "opencodego";
  const hero = windows[0] ?? null;
  const secondary = opencodeGo ? null : (windows[1] ?? null);
  const extraTiles = useMemo(() => {
    const rest = windows.slice(opencodeGo ? 1 : 2);
    const count = rest.length;
    return rest.map((w, idx) => ({
      ...w,
      fullWidth: count === 1 || (count > 1 && idx === count - 1 && count % 2 === 1),
    }));
  }, [windows, opencodeGo]);

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

  const condensedChip = condensedChipText(
    [secondary, ...extraTiles].filter((w): w is TrayWindowView => w != null),
    display,
  );

  // Minimal tier: no card-header / card-zone split — a two-row streamlined
  // card (spec 5.4).
  if (densityMode === "minimal") {
    return (
      <div className="tray-card tray-card--minimal" id={`card-${provider.providerId}`}>
        <MinimalCard
          provider={provider}
          display={display}
          outputSpeedText={outputSpeedText}
          balanceText={balanceText}
          hero={hero}
          condensedChip={condensedChip}
          showProviderIcon={showProviderIcon}
          caps={caps}
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
              pace={provider.pace}
              windowKind={hero.snap.kind}
            />
            {secondary && (
              <QuotaTile label={secondary.label} snap={secondary.snap} display={display} pace={provider.pace} fullWidth />
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
                  pace={provider.pace}
                  fullWidth={tile.fullWidth}
                />
              ))}
            </div>
          </div>
        )}

        {balanceInfo.balance && (
          <div className={`modular-section${sectionClass()}`}>
            <BalanceBlock provider={provider} isCompact={isCompact} />
          </div>
        )}

        {!hero && !balanceInfo.balance && hasStatus && (
          <div className={`modular-section${sectionClass()}`}>
            <StatusBlock provider={provider} />
          </div>
        )}

        {/* Unified insight footer: speed / near usage / API-value subline.
            Detailed = HTML meta-well (filled, two centered halves + note);
            compact = dual chips. Only renders when the provider actually has
            one of these capabilities — no empty slot for providers we cannot
            measure. */}
        {caps.outputSpeed || caps.localUsage ? (
        <div className={`modular-section${isCompact ? sectionClass() : ""}`}>
          {!isCompact ? (
            <div className="meta-well">
              {caps.outputSpeed || caps.localUsage ? (
              <div className="meta-well__line">
                {caps.outputSpeed && (
                  <div className="meta-well__item">
                    <span className="meta-well__label">{t("TaskbarWidgetPreviewSpeed")}</span>
                    <span className="meta-well__value" data-slot="speed">{outputSpeedText ?? ""}</span>
                  </div>
                )}
                {caps.outputSpeed && caps.localUsage && (
                  <span className="meta-well__rule" aria-hidden="true" />
                )}
                {caps.localUsage && (
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
              {caps.localUsage && usageLead && usageLead.cost != null && (
                <div className="meta-well__note">
                  {t("PanelApiEquivalentValue")} ≈ {formatApiEquivalentValue(usageLead.cost)}
                  {usageLead.topModel ? ` · ${usageLead.topModel}` : ""}
                </div>
              )}
            </div>
          ) : (
            <div className="compact-chips-row">
              {caps.outputSpeed && (
                <span className="compact-chip">{t("TaskbarWidgetPreviewSpeed")} <strong>{outputSpeedText ?? ""}</strong></span>
              )}
              {caps.localUsage && (
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
