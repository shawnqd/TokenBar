import { useEffect, useMemo, useState } from "react";
import { useLocale } from "../../../hooks/useLocale";
import { useResetDisplay } from "../../../hooks/useFormattedResetTime";
import { getProviderDetailBalance } from "../../../lib/providerBalance";
import {
  quotaPercentDisplay,
  type QuotaDisplayContext,
} from "../../../lib/quotaDisplay";
import { providerCapabilities } from "../../../lib/providerCapabilities";
import { defaultChartLoader } from "../../../core/chartAccess";
import {
  isMeaningfulQuotaWindow,
  quotaWindowLabel,
} from "../../../components/ProviderQuotaBlock";
import type {
  DailyCostPoint,
  LocalUsagePeriod,
  ProviderChartData,
  ProviderDetail,
  RateWindowSnapshot,
} from "../../../types/bridge";
import { V5Seg } from "./v5Controls";

type RangeKey = "7d" | "30d" | "q" | "y";

const RANGE_DAYS: Record<RangeKey, number> = {
  "7d": 7,
  "30d": 30,
  q: 90,
  y: 365,
};

const RANGE_OPTIONS = [
  { value: "7d", label: "7 天" },
  { value: "30d", label: "30 天" },
  { value: "q", label: "季度" },
  { value: "y", label: "年" },
];

const USD_TO_CNY = 7.2;

function formatApiEquivalent(amount: number): string {
  return `$${amount.toFixed(2)} · ¥${(amount * USD_TO_CNY).toFixed(2)}`;
}

function usageKnown(rate: RateWindowSnapshot): boolean {
  return !rate.isInformational && Number.isFinite(rate.usedPercent);
}

function QuotaBarRow({
  rate,
  label,
  display,
  hero,
}: {
  rate: RateWindowSnapshot;
  label: string;
  display: QuotaDisplayContext;
  hero: boolean;
}) {
  const reset = useResetDisplay(
    rate.resetsAt,
    rate.resetDescription,
    display.resetTimeRelative,
  );
  const known = usageKnown(rate);
  const shown = known ? quotaPercentDisplay(rate, display) : null;
  const resetText =
    reset && reset.kind !== "unknown" ? reset.text : null;
  return (
    <div>
      <div className="s5-tc-row">
        <span className="s5-tc-tag">
          {label}
          {resetText ? ` · ${resetText}` : ""}
        </span>
        {hero && shown ? (
          <span className="s5-pill">
            {display.showAsUsed ? "已用" : "剩余"} {shown.rounded}%
          </span>
        ) : shown ? (
          <b>{shown.rounded}%</b>
        ) : null}
      </div>
      {shown ? (
        <div className="s5-bar">
          <b style={{ width: `${shown.fillPercent}%` }} />
        </div>
      ) : (
        <div className="s5-chart-empty">没有可用额度数字</div>
      )}
    </div>
  );
}

function rangeFromSettings(period: LocalUsagePeriod | undefined): RangeKey {
  if (period === "30d") return "30d";
  return "7d";
}

function takeLastDays(points: DailyCostPoint[], days: number): DailyCostPoint[] {
  const sliced = points.slice(-days);
  const maxBars = 14;
  if (sliced.length <= maxBars) return sliced;
  const bucket = Math.ceil(sliced.length / maxBars);
  const out: DailyCostPoint[] = [];
  for (let i = 0; i < sliced.length; i += bucket) {
    const group = sliced.slice(i, i + bucket);
    out.push({
      date: group[group.length - 1].date,
      value: group.reduce((sum, point) => sum + point.value, 0) / group.length,
    });
  }
  return out;
}

function formatCount(value: number): string {
  return new Intl.NumberFormat("en-US").format(Math.round(value));
}

function formatCompact(value: number): string {
  if (value >= 1e8) return `${(value / 1e8).toFixed(1)}亿`;
  if (value >= 1e4) return `${(value / 1e4).toFixed(1)}万`;
  return formatCount(value);
}

export default function ProviderUsageCard({
  detail,
  display,
  localUsagePeriod,
  chartLoader,
}: {
  detail: ProviderDetail | null;
  display: QuotaDisplayContext;
  localUsagePeriod: LocalUsagePeriod;
  /**
   * Capability-aware chart loader injected by the caller. When not supplied,
   * the card falls back to the shared Tauri read — the same read the tray
   * and taskbar strips use (single backend call, no per-provider branching).
   */
  chartLoader?: (providerId: string, accountEmail?: string) => Promise<ProviderChartData>;
}) {
  const { t } = useLocale();
  const [charts, setCharts] = useState<ProviderChartData | null>(null);
  const [range, setRange] = useState<RangeKey>(() =>
    rangeFromSettings(localUsagePeriod),
  );

  useEffect(() => {
    if (!detail) {
      setCharts(null);
      return;
    }
    let cancelled = false;
    (chartLoader ?? defaultChartLoader)(detail.id, detail.email ?? undefined)
      .then((data) => {
        if (!cancelled) setCharts(data);
      })
      .catch(() => {
        if (!cancelled) setCharts(null);
      });
    return () => {
      cancelled = true;
    };
  }, [detail, chartLoader]);

  const days = RANGE_DAYS[range];
  const series = useMemo(
    () => takeLastDays(charts?.costHistory ?? [], days),
    [charts, days],
  );

  if (!detail) {
    return (
      <div className="s5-pd-card">
        <div className="s5-chart-empty">正在读取用量</div>
      </div>
    );
  }

  const balance = getProviderDetailBalance(detail);
  const bars: { key: string; label: string; rate: RateWindowSnapshot }[] = [];
  if (
    detail.session &&
    !balance.excludeWindows.has("session") &&
    isMeaningfulQuotaWindow(detail.session)
  ) {
    bars.push({
      key: "session",
      label: quotaWindowLabel("session", detail.session, t),
      rate: detail.session,
    });
  }
  if (
    detail.weekly &&
    !balance.excludeWindows.has("weekly") &&
    isMeaningfulQuotaWindow(detail.weekly)
  ) {
    bars.push({
      key: "weekly",
      label: quotaWindowLabel("weekly", detail.weekly, t),
      rate: detail.weekly,
    });
  }
  if (detail.modelSpecific && isMeaningfulQuotaWindow(detail.modelSpecific)) {
    bars.push({
      key: "model",
      label: t("DetailWindowModelSpecific"),
      rate: detail.modelSpecific,
    });
  }
  if (detail.tertiary && isMeaningfulQuotaWindow(detail.tertiary)) {
    bars.push({
      key: "tertiary",
      label: quotaWindowLabel("monthly", detail.tertiary, t),
      rate: detail.tertiary,
    });
  }
  for (const extra of detail.extraRateWindows ?? []) {
    if (!isMeaningfulQuotaWindow(extra.window)) continue;
    bars.push({ key: extra.id, label: extra.title, rate: extra.window });
  }

  const max = Math.max(0, ...series.map((point) => point.value));
  const caps = providerCapabilities({ providerId: detail.id, capabilities: undefined });
  const local = charts?.localUsage;
  const tokenLead =
    range === "30d" || range === "q" || range === "y"
      ? local?.thirtyDayTokens
      : local?.sevenDayTokens;
  const tokenCost =
    range === "30d" || range === "q" || range === "y"
      ? local?.thirtyDayCost
      : local?.sevenDayCost;
  const topModel =
    range === "30d" || range === "q" || range === "y"
      ? local?.thirtyDayTopModel
      : local?.sevenDayTopModel;
  const rangeLabel =
    range === "30d"
      ? "近 30 天使用"
      : range === "q"
        ? "本季使用"
        : range === "y"
          ? "近一年使用"
          : "近 7 天使用";

  return (
    <>
      <div className="s5-pd-card">
        <div className="s5-pd-card-h">
          <h4>额度</h4>
        </div>
        {balance.balance ? (
          <>
            <div className="s5-tc-tag">{balance.balance.title}</div>
            <div className="s5-tc-pct">
              {balance.balance.unavailable ? "—" : balance.balance.amount}
            </div>
            {balance.balance.breakdown ? (
              <div className="s5-insight">
                <span>{balance.balance.breakdown}</span>
              </div>
            ) : null}
          </>
        ) : null}
        {bars.length === 0 && !balance.balance ? (
          <div className="s5-chart-empty">还没有额度</div>
        ) : null}
        {bars.map((bar, index) => (
          <QuotaBarRow
            key={bar.key}
            rate={bar.rate}
            label={bar.label}
            display={display}
            hero={index === 0}
          />
        ))}
      </div>

      {!caps.localUsage ? null : (
      <div className="s5-pd-card">
        <div className="s5-pd-card-h">
          <h4>近期用量</h4>
          <V5Seg
            value={range}
            options={RANGE_OPTIONS}
            onChange={(value) => setRange(value as RangeKey)}
          />
        </div>

        {series.length > 0 && max > 0 ? (
          <div className="s5-chart-demo" aria-hidden>
            {series.map((point, index) => (
              <i
                key={`${point.date}-${index}`}
                style={{ height: `${Math.max(6, (point.value / max) * 100)}%` }}
              />
            ))}
          </div>
        ) : (
          <div className="s5-chart-empty">还没有可展示的用量</div>
        )}

        {tokenLead != null && tokenLead > 0 ? (
          <div className="s5-insight">
            <span>{rangeLabel}</span>
            <strong>≈ {formatCompact(tokenLead)}</strong>
          </div>
        ) : null}
        {tokenCost != null && tokenCost > 0 ? (
          <div className="s5-insight-sub">
            等额 API 价值 ≈ {formatApiEquivalent(tokenCost)}
            {topModel ? ` · ${topModel}` : ""}
          </div>
        ) : null}

        {detail.id === "codex" ? (
          <p className="s5-pd-lead" style={{ margin: "8px 0 0" }}>
            本地会留约 8 周会话用量，用来估节奏。这不是登录方式。
          </p>
        ) : null}
      </div>
      )}
    </>
  );
}
