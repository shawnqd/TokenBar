import { useMemo, useState, cloneElement, type ReactElement, type MouseEvent as ReactMouseEvent } from "react";
import { createPortal } from "react-dom";
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
import {
  fromBridge,
  isRealQuotaWindow,
  projectSurface,
  type DisplayState,
  type ProjectedWindow,
  type ProviderSnapshot,
} from "../../core";
import { useLocale } from "../../hooks/useLocale";
import { useResetDisplay } from "../../hooks/useFormattedResetTime";

import {
  forecastMarkerPercent,
  formatResetDisplay,
  quotaForecastDisplay,
  quotaPercentDisplay,
  type QuotaDisplayContext,
} from "../../lib/quotaDisplay";
import { isResetCreditsExtra, parseResetCreditsCount } from "../../lib/quotaWindows";
import { formatRelativeUpdated } from "../../lib/relativeTime";
import {
  getProviderBalance,
  balanceFromCost,
  type BalanceView,
  type BalanceWindow,
} from "../../lib/providerBalance";
import { localizeProviderError } from "../../lib/providerErrorText";
import { coreSnapshotToBridge } from "../../lib/trayProviders";
import { ProviderIcon } from "../../components/providers/ProviderIcon";
import {
  quotaWindowLabel,
} from "../../components/ProviderQuotaBlock";
import type { LocaleKey } from "../../i18n/keys";
import "./tray-v5.css";

/* ── Hover tip ─────────────────────────────────────────────────────────────
   A portal-rendered, fixed-position tip. The pure-CSS ::after variant was
   clipped by the panel's overflow:hidden chrome (flyout-body is a scroll
   container), so the tip must render outside that subtree: portal to <body>,
   anchored at the trigger's rect (clamped to the right edge), pointer-inert. */
function Tip({
  text,
  children,
  onlyOnOverflow = false,
}: {
  text: string;
  children: ReactElement;
  onlyOnOverflow?: boolean;
}) {
  const [anchor, setAnchor] = useState<{ left: number; top: number; isTop: boolean } | null>(null);
  return (
    <>
      {cloneElement(children, {
        onMouseEnter: (event: ReactMouseEvent<HTMLElement>) => {
          const el = event.currentTarget;
          if (onlyOnOverflow) {
            const isOverflowing =
              el.scrollWidth > el.clientWidth ||
              Array.from(el.querySelectorAll<HTMLElement>("*")).some(
                (child) => child.scrollWidth > child.clientWidth,
              );
            if (!isOverflowing) {
              setAnchor(null);
              return;
            }
          }
          const rect = el.getBoundingClientRect();
          const isTop = rect.top >= 70;
          const top = isTop ? rect.top : rect.bottom + 6;
          const maxLeft = Math.max(8, window.innerWidth - 280);
          const left = Math.max(8, Math.min(rect.left, maxLeft));
          setAnchor({ left, top, isTop });
        },
        onMouseLeave: () => setAnchor(null),
      })}
      {anchor != null &&
        createPortal(
          <div
            className="tray-tip"
            style={{
              left: anchor.left,
              top: anchor.top,
              transform: anchor.isTop ? "translateY(calc(-100% - 6px))" : "none",
            }}
          >
            {text}
          </div>,
          document.body,
        )}
    </>
  );
}

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
  // Date-only renewal markers have a reset instant and no measurable quota.
  // An unused one-time grant (ZCode 体验套餐: 0% used, no cycle length, ends_at
  // set, remaining in the description) is a real quota and must stay visible.
  if (isRealQuotaWindow(window)) return false;
  if (
    window.windowMinutes == null &&
    window.usedPercent === 0 &&
    window.resetsAt != null
  ) {
    return true;
  }
  return false;
}

interface InventoryLineInfo {
  count: number | null;
  earliestExpiry: string | null;
  expiries: string[];
}

function extractInventoryInfo(
  bridge: ProviderUsageSnapshot,
  core: ProviderSnapshot,
): InventoryLineInfo | null {
  const bridgeExtra = bridge.extraRateWindows?.find((extra) => isResetCreditsExtra(extra));
  const coreWindow = core.windows?.find(
    (w) => w.id === "reset-credits" || w.id.includes("reset-credit") || parseResetCreditsCount(w.resetDescription) != null,
  );

  if (!bridgeExtra && !coreWindow) return null;

  const expiries: string[] =
    (bridgeExtra?.inventoryExpiresAt && bridgeExtra.inventoryExpiresAt.length > 0)
      ? bridgeExtra.inventoryExpiresAt
      : (coreWindow?.inventoryExpiresAt ?? []);

  const desc =
    bridgeExtra?.window?.resetDescription ??
    coreWindow?.resetDescription ??
    null;

  const count =
    parseResetCreditsCount(desc) ??
    (expiries.length > 0 ? expiries.length : null);

  const earliestExpiry =
    expiries[0] ??
    bridgeExtra?.window?.resetsAt ??
    coreWindow?.resetsAt ??
    null;

  if (count == null && earliestExpiry == null) return null;
  return { count, earliestExpiry, expiries };
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
/** Titles that are literally just a cycle word collapse to the localized
 *  cycle label. Anything carrying model/window identity ("Gemini 5-hour",
 *  "Claude/GPT weekly", "Opus only", "Codex Spark Weekly") survives verbatim —
 *  collapsing those is what hid Antigravity's model groups from the tray. */
const CYCLE_ONLY_LABEL =
  /^(weekly|monthly|daily|hourly|session|session \(5h\)|rolling|tertiary|5h|5-hour)$/i;

function windowDisplayLabel(
  raw: string | undefined,
  snap: RateWindowSnapshot,
  t: (key: LocaleKey) => string,
): string {
  const trimmed = raw?.trim();
  if (trimmed && !CYCLE_ONLY_LABEL.test(trimmed)) return trimmed;
  return quotaWindowLabel(raw, snap, t);
}

function legacyWindowLabel(
  window: ProjectedWindow,
  bridge: ProviderUsageSnapshot,
  t: (key: LocaleKey) => string,
): string {
  const snap = toRateWindow(window);
  if (window.id === "primary") {
    return windowDisplayLabel(bridge.primaryLabel, snap, t);
  }
  if (window.id === "secondary") {
    return windowDisplayLabel(bridge.secondaryLabel, snap, t);
  }
  if (window.id === "modelSpecific") {
    return t("DetailWindowModelSpecific");
  }
  if (window.id === "tertiary") {
    return quotaWindowLabel("monthly", snap, t);
  }
  const extra = bridge.extraRateWindows?.find((row) => row.id === window.id);
  return windowDisplayLabel(extra?.title ?? window.label, snap, t);
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

/** Universal template-driven cycle label shortener:
 *  - Standalone: 周额度 → 周, 月度配额 → 月, 5小时额度 → 5h, 日额度 → 日.
 *  - Compound: [Model/Prefix] [Cycle] (e.g. "Claude/GPT weekly" → "Claude 周",
 *    "Gemini 5-hour" → "Gemini 5h", "GPT weekly" → "GPT 周", "Codex Spark Weekly" → "Codex Spark 周").
 *  Works across all providers and model families without individual hardcoded branches. */
function shortTileLabel(raw: string, language?: Language): string {
  const trimmed = raw.trim();
  const isZh =
    language === "chinese" ||
    language === "chinesetraditional" ||
    (typeof language === "string" && language.startsWith("zh"));

  // 1. Standalone cycle terms: Chinese terms stay Chinese; English terms translate if zh
  if (/^(?:5h|5-hour|5\s*小时(?:额度)?|5h\s*额度|session)$/i.test(trimmed)) return "5h";
  if (/^(?:周|周额度|周度(?:额度|配额|阶梯)?)$/.test(trimmed)) return "周";
  if (/^(?:weekly(?:\s+quota)?|week)$/i.test(trimmed)) return isZh ? "周" : "wk";
  if (/^(?:月|月额度|月度(?:额度|配额|总池)?)$/.test(trimmed)) return "月";
  if (/^(?:monthly(?:\s+quota)?|month)$/i.test(trimmed)) return isZh ? "月" : "mo";
  if (/^(?:日|日额度|日度(?:额度|配额)?|今日额度)$/.test(trimmed)) return "日";
  if (/^(?:daily(?:\s+quota)?|day)$/i.test(trimmed)) return isZh ? "日" : "day";
  if (/^(?:年|年额度|年度(?:额度|配额)?)$/.test(trimmed)) return "年";
  if (/^(?:yearly(?:\s+quota)?|year|annual)$/i.test(trimmed)) return isZh ? "年" : "yr";

  // 2. Compound: Prefix + Cycle, e.g. "Claude 5-hour", "Gemini weekly", "GPT weekly", "Codex Spark Weekly"
  const compoundMatch = trimmed.match(
    /^(.+?)[\s_-]+(5-hour|5h|session|weekly|week|周度?|monthly|month|月度?|daily|day|日度?)(?:[\s_-]*(?:额度|配额|阶梯|总池|quota|limit))?$/i,
  );
  if (compoundMatch) {
    let prefix = compoundMatch[1].trim();
    if (/^claude(?:\/gpt)?$/i.test(prefix)) prefix = "Claude";
    else if (/^gemini$/i.test(prefix)) prefix = "Gemini";
    else if (/^gpt$/i.test(prefix)) prefix = "GPT";

    const cycle = compoundMatch[2].toLowerCase();
    if (cycle === "5-hour" || cycle === "5h" || cycle === "session") {
      return `${prefix} 5h`;
    }
    if (cycle.startsWith("周")) return `${prefix} 周`;
    if (cycle === "weekly" || cycle === "week") {
      return isZh ? `${prefix} 周` : `${prefix} wk`;
    }
    if (cycle.startsWith("月")) return `${prefix} 月`;
    if (cycle === "monthly" || cycle === "month") {
      return isZh ? `${prefix} 月` : `${prefix} mo`;
    }
    if (cycle.startsWith("日")) return `${prefix} 日`;
    if (cycle === "daily" || cycle === "day") {
      return isZh ? `${prefix} 日` : `${prefix} day`;
    }
  }

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
        <Tip text={title} onlyOnOverflow>
          <span className="quota-row__label">{title}</span>
        </Tip>
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
          <Tip text={t("PanelExpected")}>
            <div
              className={`progress-notch progress-notch--${tone}`}
              style={{ left: `${marker}%` } as CSSProperties}
            />
          </Tip>
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
  // 悬浮浮窗:完整标签 + 重置时间(头部行会截断两者,浮窗给全量)。
  const headResetText =
    compactResetText(snap.resetsAt, language) ??
    (reset.kind !== "unknown" ? reset.text : "");
  const headTip =
    headResetText && !headResetText.includes("重置")
      ? `${label} · 重置 ${headResetText}`
      : headResetText
        ? `${label} · ${headResetText}`
        : label;
  // Full-width tiles (the secondary cycle): label + reset share the head line
  // (weight tells them apart), bar below — density-preview.html full row.
  if (fullWidth) {
    return (
      <div className="quota-tile quota-tile--full">
        <Tip text={headTip}>
          <div className="quota-tile__head">
            <span className="quota-tile__title">
              <span className="quota-tile__label">{shortTileLabel(label, language)}</span>
              <span className="quota-tile__reset" data-reset-state={reset.kind}>
                {compactResetText(snap.resetsAt, language) ?? (reset.kind !== "unknown" ? reset.text : "")}
              </span>
            </span>
            <strong className="quota-tile__val">{percent.rounded}%</strong>
          </div>
        </Tip>
        <div className="progress-bar progress-bar--tile">
          <div
            className="progress-fill"
            data-level={percent.level}
            style={{ width: `${percent.fillPercent}%` } as CSSProperties}
          />
          {notch != null && notchTone && (
            <Tip text={t("PanelExpected")}>
              <div
                className={`progress-notch progress-notch--${notchTone}`}
                style={{ left: `${notch}%` } as CSSProperties}
              />
            </Tip>
          )}
        </div>
      </div>
    );
  }
  // Half-width extras: label + pct on the head line, reset below the bar.
  return (
    <div className="quota-tile">
      <Tip text={headTip}>
        <div className="quota-tile__head">
          <span className="quota-tile__title">
            <span className="quota-tile__label">{shortTileLabel(label, language)}</span>
            <span className="quota-tile__reset" data-reset-state={reset.kind}>
              {compactResetText(snap.resetsAt, language) ?? (reset.kind !== "unknown" ? reset.text : "")}
            </span>
          </span>
          <strong className="quota-tile__val">{percent.rounded}%</strong>
        </div>
      </Tip>
      <div className="progress-bar progress-bar--tile">
        <div
          className="progress-fill"
          data-level={percent.level}
          style={{ width: `${percent.fillPercent}%` } as CSSProperties}
        />
        {notch != null && notchTone && (
          <Tip text={t("PanelExpected")}>
            <div
              className={`progress-notch progress-notch--${notchTone}`}
              style={{ left: `${notch}%` } as CSSProperties}
            />
          </Tip>
        )}
      </div>
    </div>
  );
}

/* ── Balance / status blocks (never fabricate) ────────────────────────── */

type BalanceStatus = {
  label: string;
  tone: "reserve" | "deficit" | "neutral";
};

function resolveBalanceStatus(
  balance: BalanceView,
  displayState: DisplayState,
  t: (key: LocaleKey) => string,
): BalanceStatus | null {
  if (
    balance.unavailable ||
    displayState === "error" ||
    displayState === "authRequired" ||
    displayState === "notConfigured"
  ) {
    return { label: t("StatusUnableToGetUsage"), tone: "deficit" };
  }
  if (displayState === "stale") return { label: t("TrayStatusStale"), tone: "neutral" };
  if (displayState === "loading" || displayState === "refreshing") {
    return { label: t("TrayLoading"), tone: "neutral" };
  }
  if (displayState === "unsupported" || displayState === "unknown") {
    return { label: t("TrayStatusError"), tone: "neutral" };
  }
  return null;
}

function BalanceBlock({
  balance,
  status,
  isCompact,
}: {
  balance: BalanceView | null;
  status: BalanceStatus | null;
  isCompact?: boolean;
}) {
  if (!balance) return null;
  const badge = status ?? { label: "", tone: "neutral" as const };
  const titleClean = balance.title.replace(/^💳\s*/, "");
  const fullText = balance.breakdown
    ? `${titleClean}: ${balance.amount} (${balance.breakdown})`
    : `${titleClean}: ${balance.amount}`;
  if (isCompact) {
    return (
      <div
        className="balance-compact-row balance-block"
        data-balance-kind={balance.kind}
        data-balance-state={badge.tone}
      >
        <Tip text={fullText} onlyOnOverflow>
          <div className="balance-compact-row__left balance-block__head">
            <svg
              width="12"
              height="12"
              viewBox="0 0 24 24"
              fill="none"
              stroke="currentColor"
              strokeWidth="2"
              strokeLinecap="round"
              strokeLinejoin="round"
              style={{ flexShrink: 0, marginRight: 4, opacity: 0.7 }}
            >
              <rect x="2" y="5" width="20" height="14" rx="2" />
              <line x1="2" y1="10" x2="22" y2="10" />
            </svg>
            <span className="balance-compact-row__label balance-block__label">{titleClean}:</span>
            <span
              className="balance-compact-row__amount balance-block__amount font-mono"
              data-unavailable={balance.unavailable ? "true" : undefined}
            >
              {balance.amount}
            </span>
            {balance.breakdown ? (
              <span className="balance-compact-row__sub balance-block__gift font-mono">({balance.breakdown})</span>
            ) : null}
          </div>
        </Tip>
        {badge.label && <span className={`soft-badge soft-badge--${badge.tone}`}>{badge.label}</span>}
      </div>
    );
  }
  return (
    <div
      className="balance-single-line balance-block"
      data-balance-kind={balance.kind}
      data-balance-state={badge.tone}
    >
      <Tip text={fullText} onlyOnOverflow>
        <div className="balance-single-line__left balance-block__head">
          <svg
            width="12"
            height="12"
            viewBox="0 0 24 24"
            fill="none"
            stroke="currentColor"
            strokeWidth="2"
            strokeLinecap="round"
            strokeLinejoin="round"
            className="balance-single-line__icon"
            style={{ flexShrink: 0, opacity: 0.7 }}
          >
            <rect x="2" y="5" width="20" height="14" rx="2" />
            <line x1="2" y1="10" x2="22" y2="10" />
          </svg>
          <span className="balance-single-line__label balance-block__label">
            {titleClean}
          </span>
          <span
            className="balance-single-line__amount balance-block__amount font-mono"
            data-unavailable={balance.unavailable ? "true" : undefined}
          >
            {balance.amount}
          </span>
          {balance.breakdown ? (
            <span className="balance-single-line__sub balance-block__gift font-mono">
              ({balance.breakdown})
            </span>
          ) : null}
        </div>
      </Tip>
      {badge.label && <span className={`soft-badge soft-badge--${badge.tone}`}>{badge.label}</span>}
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
  language?: Language,
): string | null {
  if (windows.length === 0) return null;
  const isZh =
    language === "chinese" ||
    language === "chinesetraditional" ||
    (typeof language === "string" && language.startsWith("zh")) ||
    windows.some((w) => /[\u4e00-\u9fa5]/.test(w.label));
  const parts = windows.map((w) => {
    let name = shortTileLabel(w.label, language);
    if (!name || name === w.label) {
      if (w.snap.kind === "weekly") name = isZh ? "周" : "wk";
      else if (w.snap.kind === "monthly") name = isZh ? "月" : "mo";
      else if (w.snap.kind === "daily") name = isZh ? "日" : "day";
      else if (w.snap.kind === "session") name = "5h";
    }
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
  statusHeadline,
  outputSpeedText,
  balanceText,
  balanceStatus,
  hero,
  condensedChip,
  showProviderIcon,
  hasOutputSpeed,
  quotaMissing,
}: {
  providerId: string;
  displayName: string;
  pace: PaceSnapshot | null;
  display: QuotaDisplayContext;
  hasStatus: boolean;
  statusHeadline?: string | null;
  outputSpeedText: string | null;
  balanceText: string | null;
  balanceStatus: BalanceStatus | null;
  hero: CardWindowView | null;
  condensedChip: string | null;
  showProviderIcon: boolean;
  hasOutputSpeed: boolean;
  quotaMissing: string | null;
}) {
  const { t, language } = useLocale();
  const isZh =
    language === "chinese" ||
    language === "chinesetraditional" ||
    (typeof language === "string" && language.startsWith("zh"));
  const name = displayName.split(" ")[0];

  const heroIsReal = hero != null && !hero.snap.isInformational;
  let metric: string | null = null;
  let subLabel = "";

  if (heroIsReal) {
    metric = `${quotaPercentDisplay(hero!.snap, display).rounded}%`;
    const short = shortTileLabel(hero!.label, language);
    subLabel = `· ${short}`;
  } else if (balanceText) {
    metric = balanceText;
    subLabel = isZh ? "· 余额" : "· Balance";
  } else if (hasStatus) {
    subLabel = isZh ? "· 状态" : "· Status";
    metric = statusHeadline
      ? `● ${statusHeadline.split(/[·\s(]/)[0]}`
      : (isZh ? "● 就绪" : "● Ready");
  }

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
    // No quota window: surface the balance state instead of a hard-coded
    // "normal" badge. A balance can be stale/unavailable and must say so.
    const breakdown = balanceText && balanceStatus
      ? `● ${balanceStatus.label}`
      : hasStatus
        ? "●"
        : null;
    paceBadge = breakdown ?? null;
    if (balanceStatus?.tone === "deficit") paceTone = "deficit";
    else if (balanceStatus?.tone === "neutral") paceTone = "onpace";
    else if (balanceStatus) paceTone = "reserve";
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
          {!hero && quotaMissing ? (
            <span className="soft-badge soft-badge--neutral minimal-streamlined__quota-missing">
              {quotaMissing}
            </span>
          ) : (
            <>
              {paceBadge ? <span className={`soft-badge soft-badge--${paceTone}`}>{paceBadge}</span> : null}
              {condensedChip ? <span className="soft-badge soft-badge--neutral">{condensedChip}</span> : null}
            </>
          )}
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

  const balanceInfo = useMemo(() => {
    const fromBridgeInfo = getProviderBalance(bridge);
    if (fromBridgeInfo.balance) return fromBridgeInfo;
    if (core.cost) {
      const balance = balanceFromCost(core.cost);
      if (balance) {
        return {
          balance,
          excludeWindows: new Set<BalanceWindow>(),
          suppressPlanBadge: false,
        };
      }
    }
    return fromBridgeInfo;
  }, [bridge, core.cost]);
  const balanceStatus = useMemo(
    () =>
      balanceInfo.balance
        ? resolveBalanceStatus(balanceInfo.balance, projection.displayState, t)
        : null,
    [balanceInfo.balance, projection.displayState, t],
  );
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
  // Three measurable cycles (OpenCode Go / density-preview): one hero on top
  // and two tiles side by side. Two cycles keep the HTML full-width secondary
  // under the hero. Four or more keep hero + full-width secondary + remaining
  // 2-col extras (Antigravity).
  const pairExtras = views.length === 3;
  const secondary = pairExtras ? null : (views[1] ?? null);
  const extraTiles = useMemo(() => {
    const rest = pairExtras ? views.slice(1) : views.slice(2);
    const count = rest.length;
    return rest.map((w, idx) => ({
      ...w,
      fullWidth: count === 1 || (count > 1 && idx === count - 1 && count % 2 === 1),
    }));
  }, [views, pairExtras]);

  const localUsage = core.error ? null : chartData?.localUsage ?? null;
  const usageLead = localUsage ? resolveLocalUsageLead(localUsagePeriod, localUsage) : null;
  const hasLocalUsageTokens = Boolean(
    hasLocalUsage && usageLead && usageLead.tokens != null && usageLead.tokens > 0,
  );

  // G8 honesty: when the quota read failed and no window survived (no hero),
  // the card says so instead of silently rendering a header-only shell. The
  // reason text is the shared provider-error localization; unknown text falls
  // through verbatim so a new backend category is never hidden.
  const quotaUnavailableText =
    !hero && core.error != null && core.error.trim() !== ""
      ? localizeProviderError(core.error, t)
      : null;

  const speedValid =
    outputSpeed != null && outputSpeed.tokensPerSecond != null && outputSpeed.tokensPerSecond > 0;
  const outputSpeedText = speedValid ? `${outputSpeed!.tokensPerSecond!.toFixed(1)} t/s` : null;
  const speedFallbackText =
    outputSpeed?.status === "generating" ? t("OutputSpeedGenerating") : "--";

  const inventoryInfo = useMemo(
    () => extractInventoryInfo(bridge, core),
    [bridge, core],
  );

  const isZh =
    language === "chinese" ||
    language === "chinesetraditional" ||
    (typeof language === "string" && language.startsWith("zh"));

  const inventoryExpireText = useMemo(() => {
    if (!inventoryInfo?.earliestExpiry) return null;
    const formatted = formatResetDisplay({
      resetsAt: inventoryInfo.earliestExpiry,
      resetDescription: null,
      relative: false,
      t,
      locale: localeCodeFor(language),
    }).text;
    return isZh ? `${formatted} 到期` : `Expires ${formatted}`;
  }, [inventoryInfo?.earliestExpiry, t, language, isZh]);

  const inventoryTipText = useMemo(() => {
    if (!inventoryInfo) return null;
    const count =
      inventoryInfo.count ??
      (inventoryInfo.expiries.length > 0 ? inventoryInfo.expiries.length : null);
    const expiries = inventoryInfo.expiries;
    const lines: string[] = [];

    if (count != null) {
      lines.push(
        isZh
          ? `${t("PanelResetCreditsTitle")} (${t("PanelResetCreditsRemaining")} ${count} ${t("PanelResetCreditsUnit")})`
          : `${t("PanelResetCreditsTitle")} (${count} ${t("PanelResetCreditsRemaining")})`,
      );
    } else {
      lines.push(t("PanelResetCreditsTitle"));
    }

    const totalRows = Math.max(count ?? 0, expiries.length);
    if (totalRows > 0) {
      for (let i = 0; i < totalRows; i++) {
        const iso = expiries[i] ?? (i === 0 ? inventoryInfo.earliestExpiry : null);
        const prefix = isZh ? `第 ${i + 1} 次` : `Credit ${i + 1}`;
        if (iso) {
          const formatted = formatResetDisplay({
            resetsAt: iso,
            resetDescription: null,
            relative: false,
            t,
            locale: localeCodeFor(language),
          }).text;
          const timeText = isZh ? `${formatted} 到期` : `Expires ${formatted}`;
          lines.push(`${prefix} · ${timeText}`);
        } else {
          lines.push(`${prefix} · ${isZh ? "未提供到期时间" : "No expiry provided"}`);
        }
      }
    } else if (inventoryInfo.earliestExpiry) {
      const formatted = formatResetDisplay({
        resetsAt: inventoryInfo.earliestExpiry,
        resetDescription: null,
        relative: false,
        t,
        locale: localeCodeFor(language),
      }).text;
      const timeText = isZh ? `${formatted} 到期` : `Expires ${formatted}`;
      lines.push(timeText);
    }

    return lines.join("\n");
  }, [inventoryInfo, isZh, t, language]);

  const balanceText = balanceInfo.balance ? balanceInfo.balance.amount : null;
  const statusHeadline =
    (core.trayStatusLabel?.trim() || projection.telemetry?.gatewayStatus || null);
  const hasStatus = statusHeadline != null;

  // Context buttons only exist in the detail view, gated by the core
  // capability flags (no provider-name branches).
  const canDashboard = detail && caps.supportsProviderDashboard;
  const canStatus = detail && caps.supportsStatusPage;
  const hasContext = canDashboard || canStatus;

  const resetChip =
    inventoryInfo?.count != null
      ? (isZh
          ? `${inventoryInfo.count}次重置${inventoryExpireText ? `·${inventoryExpireText.replace(/ 到期$/, "")}` : ""}`
          : `${inventoryInfo.count} credits${inventoryExpireText ? `·${inventoryExpireText.replace(/^Expires /, "")}` : ""}`)
      : null;

  const tileChip = condensedChipText(
    [secondary, ...extraTiles].filter((w): w is CardWindowView => w != null),
    display,
    language,
  );
  const balanceChip =
    hero && balanceText
      ? (isZh ? `余额 ${balanceText}` : `Bal ${balanceText}`)
      : null;
  const condensedChip = [tileChip, balanceChip, resetChip].filter(Boolean).join("·") || null;

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
          statusHeadline={statusHeadline}
          outputSpeedText={outputSpeedText}
          balanceText={balanceText}
          balanceStatus={balanceStatus}
          hero={hero}
          condensedChip={condensedChip}
          showProviderIcon={showProviderIcon}
          hasOutputSpeed={hasOutputSpeed}
          quotaMissing={quotaUnavailableText}
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

  const isRefreshing = core.displayState === "refreshing";
  const hasError = core.error != null && core.error.trim() !== "";

  const updatedRaw = core.updatedAt == null ||
    Number.isNaN(Date.parse(core.updatedAt))
    ? core.updatedAt ?? ""
    : formatRelativeUpdated(Date.parse(core.updatedAt), t);
  const updatedText =
    densityMode === "compact" ? updatedRaw.replace(/更新$/, "") : updatedRaw;

  let updatedContent: React.ReactNode = updatedText;
  let updatedClass = "card-header__updated";
  let updatedTitle: string | undefined = undefined;

  if (isRefreshing) {
    updatedClass += " is-refreshing";
    updatedContent = (
      <>
        <svg className="spin" width="10" height="10" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2.5" strokeLinecap="round" style={{ display: "inline-block", verticalAlign: "-1px", marginRight: 3 }}>
          <path d="M3 12a9 9 0 0 1 9-9 9.75 9.75 0 0 1 6.74 2.74L21 8"/>
          <path d="M21 3v5h-5"/>
          <path d="M21 12a9 9 0 0 1-9 9 9.75 9.75 0 0 1-6.74-2.74L3 16"/>
          <path d="M3 21v-5h5"/>
        </svg>
        {t("SummaryRefreshing")}
      </>
    );
  } else if (hasError) {
    updatedClass += " is-error";
    updatedTitle = `${t("StatusUnableToGetUsage")}：${localizeProviderError(core.error!, t)}`;
    updatedContent = (
      <>
        <svg width="10" height="10" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2.5" strokeLinecap="round" strokeLinejoin="round" style={{ display: "inline-block", verticalAlign: "-1px", marginRight: 3 }}>
          <circle cx="12" cy="12" r="10"/>
          <line x1="12" y1="8" x2="12" y2="12"/>
          <line x1="12" y1="16" x2="12.01" y2="16"/>
        </svg>
        {t("TrayStatusError")}
      </>
    );
  }

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
            <Tip text={core.displayName} onlyOnOverflow>
              <span className="card-header__name">{core.displayName}</span>
            </Tip>
            <span className={updatedClass} title={updatedTitle}>{updatedContent}</span>
          </div>
      </div>

      <div className={`card-zone${zoneTone}`}>
        {(hero || balanceInfo.balance || inventoryInfo || hasStatus) && (
          <div className="dual-pill-container quota-stage">
            <div className="dual-pill-top">
              {hero && (
                <>
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
                  {inventoryInfo && (
                    inventoryTipText ? (
                      <Tip text={inventoryTipText}>
                        <div className="quota-inventory-line">
                          <div className="quota-inventory-line__left">
                            <span className="quota-inventory-line__icon">
                              <svg width="11" height="11" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2.2" strokeLinecap="round" strokeLinejoin="round">
                                <path d="M3 12a9 9 0 0 1 9-9 9.75 9.75 0 0 1 6.74 2.74L21 8"/>
                                <path d="M21 3v5h-5"/>
                                <path d="M21 12a9 9 0 0 1-9 9 9.75 9.75 0 0 1-6.74-2.74L3 16"/>
                                <path d="M3 21v-5h5"/>
                              </svg>
                            </span>
                            <span>{t("PanelResetCreditsTitle").replace(/次数$/, "")}</span>
                            {inventoryInfo.count != null && (
                              <strong className="quota-inventory-line__badge">
                                {t("PanelResetCreditsRemaining")} {inventoryInfo.count} {t("PanelResetCreditsUnit")}
                              </strong>
                            )}
                          </div>
                          {inventoryExpireText && (
                            <span className="quota-inventory-line__expire">{inventoryExpireText}</span>
                          )}
                        </div>
                      </Tip>
                    ) : (
                      <div className="quota-inventory-line">
                        <div className="quota-inventory-line__left">
                          <span className="quota-inventory-line__icon">
                            <svg width="11" height="11" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2.2" strokeLinecap="round" strokeLinejoin="round">
                              <path d="M3 12a9 9 0 0 1 9-9 9.75 9.75 0 0 1 6.74 2.74L21 8"/>
                              <path d="M21 3v5h-5"/>
                              <path d="M21 12a9 9 0 0 1-9 9 9.75 9.75 0 0 1-6.74-2.74L3 16"/>
                              <path d="M3 21v-5h5"/>
                            </svg>
                          </span>
                          <span>{t("PanelResetCreditsTitle").replace(/次数$/, "")}</span>
                          {inventoryInfo.count != null && (
                            <strong className="quota-inventory-line__badge">
                              {t("PanelResetCreditsRemaining")} {inventoryInfo.count} {t("PanelResetCreditsUnit")}
                            </strong>
                          )}
                        </div>
                        {inventoryExpireText && (
                          <span className="quota-inventory-line__expire">{inventoryExpireText}</span>
                        )}
                      </div>
                    )
                  )}
                </>
              )}
              {!hero && balanceInfo.balance && (
                <BalanceBlock
                  balance={balanceInfo.balance}
                  status={balanceStatus}
                  isCompact={isCompact}
                />
              )}
              {!hero && !balanceInfo.balance && hasStatus && (
                <StatusBlock headline={statusHeadline!} />
              )}
              {!hero && !balanceInfo.balance && !hasStatus && inventoryInfo && (
                inventoryTipText ? (
                  <Tip text={inventoryTipText}>
                    <div className="quota-inventory-line">
                      <div className="quota-inventory-line__left">
                        <span className="quota-inventory-line__icon">
                          <svg width="11" height="11" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2.2" strokeLinecap="round" strokeLinejoin="round">
                            <path d="M3 12a9 9 0 0 1 9-9 9.75 9.75 0 0 1 6.74 2.74L21 8"/>
                            <path d="M21 3v5h-5"/>
                            <path d="M21 12a9 9 0 0 1-9 9 9.75 9.75 0 0 1-6.74-2.74L3 16"/>
                            <path d="M3 21v-5h5"/>
                          </svg>
                        </span>
                        <span>{t("PanelResetCreditsTitle").replace(/次数$/, "")}</span>
                        {inventoryInfo.count != null && (
                          <strong className="quota-inventory-line__badge">
                            {t("PanelResetCreditsRemaining")} {inventoryInfo.count} {t("PanelResetCreditsUnit")}
                          </strong>
                        )}
                      </div>
                      {inventoryExpireText && (
                        <span className="quota-inventory-line__expire">{inventoryExpireText}</span>
                      )}
                    </div>
                  </Tip>
                ) : (
                  <div className="quota-inventory-line">
                    <div className="quota-inventory-line__left">
                      <span className="quota-inventory-line__icon">
                        <svg width="11" height="11" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2.2" strokeLinecap="round" strokeLinejoin="round">
                          <path d="M3 12a9 9 0 0 1 9-9 9.75 9.75 0 0 1 6.74 2.74L21 8"/>
                          <path d="M21 3v5h-5"/>
                          <path d="M21 12a9 9 0 0 1-9 9 9.75 9.75 0 0 1-6.74-2.74L3 16"/>
                          <path d="M3 21v-5h5"/>
                        </svg>
                      </span>
                      <span>{t("PanelResetCreditsTitle").replace(/次数$/, "")}</span>
                      {inventoryInfo.count != null && (
                        <strong className="quota-inventory-line__badge">
                          {t("PanelResetCreditsRemaining")} {inventoryInfo.count} {t("PanelResetCreditsUnit")}
                        </strong>
                      )}
                    </div>
                    {inventoryExpireText && (
                      <span className="quota-inventory-line__expire">{inventoryExpireText}</span>
                    )}
                  </div>
                )
              )}
            </div>
            {hero && balanceInfo.balance && (
              <div className="dual-pill-bottom">
                <BalanceBlock
                  balance={balanceInfo.balance}
                  status={balanceStatus}
                  isCompact={isCompact}
                />
              </div>
            )}
          </div>
        )}

        {!hero && quotaUnavailableText && (
          <div className={`modular-section${sectionClass()} tray-card__quota-missing`}>
            <div className="tray-card__quota-missing-title">{t("StatusUnableToGetUsage")}</div>
            <div className="tray-card__quota-missing-reason">{quotaUnavailableText}</div>
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


        {/* Unified insight footer: speed / near usage / API-value subline.
            Detailed = HTML meta-well (filled, two centered halves + note);
            compact = dual chips. Only renders when the provider actually has
            one of these capabilities — no empty slot for providers we cannot
            measure. */}
        {hasOutputSpeed || hasLocalUsageTokens ? (
        <div className={`modular-section${isCompact ? sectionClass() : ""}`}>
          {!isCompact ? (
            <div className="meta-well">
              {hasOutputSpeed || hasLocalUsageTokens ? (
              <div className="meta-well__line">
                {hasOutputSpeed && (
                  <div className="meta-well__item">
                    <span className="meta-well__label">{t("TaskbarWidgetPreviewSpeed")}</span>
                    <span className="meta-well__value" data-slot="speed">{outputSpeedText ?? speedFallbackText}</span>
                  </div>
                )}
                {hasOutputSpeed && hasLocalUsageTokens && (
                  <span className="meta-well__rule" aria-hidden="true" />
                )}
                {hasLocalUsageTokens && (
                  <Tip
                    text={`${t(usageLead ? usageLead.labelKey : "PanelSevenDayUsage").replace(/使用$/, "")}: ${formatApproxTokens(usageLead!.tokens!, language)} (${formatTokenCount(usageLead!.tokens!)})`}
                    onlyOnOverflow
                  >
                    <div className="meta-well__item">
                      <span className="meta-well__label">{t(usageLead ? usageLead.labelKey : "PanelSevenDayUsage").replace(/使用$/, "")}</span>
                      <span className="meta-well__value" data-slot="usage">
                        {formatApproxTokens(usageLead!.tokens!, language)}
                      </span>
                    </div>
                  </Tip>
                )}
              </div>
              ) : null}
              {hasLocalUsageTokens && usageLead && usageLead.cost != null && (
                <Tip
                  text={`${t("PanelApiEquivalentValue")} ≈ ${formatApiEquivalentValue(usageLead.cost)}${usageLead.topModel ? ` · ${usageLead.topModel}` : ""}`}
                  onlyOnOverflow
                >
                  <div className="meta-well__note">
                    {t("PanelApiEquivalentValue")} ≈ {formatApiEquivalentValue(usageLead.cost)}
                    {usageLead.topModel ? ` · ${usageLead.topModel}` : ""}
                  </div>
                </Tip>
              )}
            </div>
          ) : (
            <div className="compact-chips-row">
              {hasOutputSpeed && (
                <span className="compact-chip">{t("TaskbarWidgetPreviewSpeed")} <strong>{outputSpeedText ?? speedFallbackText}</strong></span>
              )}
              {hasLocalUsageTokens && (
                <Tip
                  text={`${t(usageLead ? usageLead.labelKey : "PanelSevenDayUsage").replace(/使用$/, "")}: ${formatCompactTokens(usageLead!.tokens!, language) ?? formatTokenCount(usageLead!.tokens!)}${usageLead && usageLead.cost != null ? ` · ${t("PanelApiEquivalentValue")} ≈ ${formatApiEquivalentValue(usageLead.cost)}` : ""}`}
                  onlyOnOverflow
                >
                  <span className="compact-chip">{t(usageLead ? usageLead.labelKey : "PanelSevenDayUsage").replace(/使用$/, "")} <strong>
                    {formatCompactTokens(usageLead!.tokens!, language) ?? formatTokenCount(usageLead!.tokens!)}
                  </strong></span>
                </Tip>
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
