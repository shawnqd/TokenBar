import { useEffect, useMemo, useState } from "react";
import { BarChart } from "../../../../components/charts/BarChart";
import { LocalUsageBlock } from "../../../../components/MenuCard";
import { useLocale } from "../../../../hooks/useLocale";
import { getProviderChartData, getSettingsSnapshot } from "../../../../lib/tauri";
import { providerSupportsChartData } from "../../../../lib/providerCharts";
import type {
  CostSnapshotBridge,
  LocalUsagePeriod,
  ProviderChartData,
  ProviderLocalUsageSummary,
  ProviderOutputSpeed,
} from "../../../../types/bridge";
import { CostHistoryChart } from "./charts/CostHistoryChart";
import { CreditsHistoryChart } from "./charts/CreditsHistoryChart";
import { UsageBreakdownChart } from "./charts/UsageBreakdownChart";

interface Props {
  providerId: string;
  accountEmail: string | null;
  /** Live output-speed snapshot (Codex/Claude only), null hides the tab. */
  speed: ProviderOutputSpeed | null;
  /** Cost snapshot from the provider detail (credits-style providers). */
  cost: CostSnapshotBridge | null;
  /** Which period the token stats lead with (Settings-driven). */
  localUsagePeriod: LocalUsagePeriod;
}

type TabKey = "tokens" | "speed" | "cost" | "credits" | "usage";
type RangeKey = "7d" | "30d" | "quarter" | "year";

const RANGE_DAYS: Record<RangeKey, number> = {
  "7d": 7,
  "30d": 30,
  quarter: 90,
  year: 365,
};

function hasLocalUsage(
  summary: ProviderLocalUsageSummary | null,
): summary is ProviderLocalUsageSummary {
  if (!summary) return false;
  return [
    summary.todayTokens,
    summary.todayCost,
    summary.sevenDayTokens,
    summary.sevenDayCost,
    summary.thirtyDayTokens,
    summary.thirtyDayCost,
  ].some((value) => value != null && value > 0);
}

/**
 * Unified stats block for the Settings → Providers detail pane.
 *
 * Token usage, output speed and cost used to render as three stacked
 * sections with three different looks. They now share a single section:
 * one segmented tab bar switches between them (tabs only appear when the
 * underlying data exists), so the pane stays short and visually consistent.
 * The credits / usage-breakdown charts from the old charts block live on as
 * additional tabs so no data is lost.
 */
export function StatsSection({
  providerId,
  accountEmail,
  speed,
  cost,
  localUsagePeriod,
}: Props) {
  const { t } = useLocale();
  const [data, setData] = useState<ProviderChartData | null>(null);
  const [active, setActive] = useState<TabKey | null>(null);
  const [range, setRange] = useState<RangeKey>("30d");
  const [animations, setAnimations] = useState(true);

  useEffect(() => {
    let cancelled = false;
    setData(null);
    if (!providerSupportsChartData(providerId)) {
      return () => {
        cancelled = true;
      };
    }
    getProviderChartData(providerId, accountEmail ?? undefined)
      .then((d) => {
        if (!cancelled) setData(d);
      })
      .catch(() => {
        if (!cancelled) setData(null);
      });
    return () => {
      cancelled = true;
    };
  }, [providerId, accountEmail]);

  useEffect(() => {
    let cancelled = false;
    getSettingsSnapshot()
      .then((s) => {
        if (!cancelled) setAnimations(s.enableAnimations);
      })
      .catch(() => {
        // Keep defaults on failure.
      });
    return () => {
      cancelled = true;
    };
  }, [providerId]);

  const localUsage = data?.localUsage ?? null;
  const costHistory = useMemo(() => data?.costHistory ?? [], [data]);
  const creditsHistory = useMemo(() => data?.creditsHistory ?? [], [data]);
  const usageBreakdown = useMemo(() => data?.usageBreakdown ?? [], [data]);

  const available: TabKey[] = [];
  if (hasLocalUsage(localUsage)) available.push("tokens");
  if (speed) available.push("speed");
  if (cost || costHistory.length > 0) available.push("cost");
  if (creditsHistory.length > 0) available.push("credits");
  if (usageBreakdown.length > 0) available.push("usage");

  const tabLabel = (key: TabKey): string => {
    switch (key) {
      case "tokens":
        return t("DetailTokensTab");
      case "speed":
        return t("OutputSpeedTitle");
      case "cost":
        return t("DetailCostTitle");
      case "credits":
        return t("DetailChartCredits");
      case "usage":
        return t("DetailChartUsageBreakdown");
    }
  };

  const rangeLabel = (key: RangeKey): string => {
    if (key === "7d") return t("FloatBarSevenDayShort");
    if (key === "30d") return t("FloatBarThirtyDayShort");
    if (key === "quarter") return t("DetailChartQuarter");
    return t("DetailChartYear");
  };

  if (available.length === 0) return null;

  const current: TabKey =
    active && available.includes(active) ? active : available[0];
  const rangeOptions: RangeKey[] = ["7d", "30d", "quarter", "year"];
  const days = RANGE_DAYS[range];
  const emptyMsg = t("DetailChartEmpty");
  // The range row only makes sense under a dated chart.
  const showRange =
    (current === "cost" && costHistory.length > 0) ||
    current === "credits" ||
    current === "usage";

  return (
    <section className="provider-detail-section provider-detail-stats">
      <div className="provider-detail-charts__tabs" role="tablist">
        <div
          className="provider-detail-charts__thumb"
          aria-hidden
          style={{
            width: `calc((100% - 4px) / ${available.length})`,
            transform: `translateX(${available.indexOf(current) * 100}%)`,
          }}
        />
        {available.map((key) => (
          <button
            key={key}
            type="button"
            role="tab"
            aria-selected={key === current}
            className="provider-detail-charts__tab"
            data-active={key === current ? "true" : "false"}
            onClick={() => setActive(key)}
          >
            {tabLabel(key)}
          </button>
        ))}
      </div>
      {showRange && (
        <div
          className="provider-detail-charts__tabs provider-detail-charts__tabs--range"
          role="radiogroup"
          aria-label={t("DetailChartRange")}
        >
          <div
            className="provider-detail-charts__thumb"
            aria-hidden
            style={{
              width: `calc((100% - 4px) / ${rangeOptions.length})`,
              transform: `translateX(${rangeOptions.indexOf(range) * 100}%)`,
            }}
          />
          {rangeOptions.map((key) => (
            <button
              key={key}
              type="button"
              role="radio"
              aria-checked={key === range}
              className="provider-detail-charts__tab"
              data-active={key === range ? "true" : "false"}
              onClick={() => setRange(key)}
            >
              {rangeLabel(key)}
            </button>
          ))}
        </div>
      )}
      <div className="provider-detail-stats__body" role="tabpanel">
        {current === "tokens" && localUsage && (
          <LocalUsageBlock
            providerId={providerId}
            summary={localUsage}
            costHistory={costHistory}
            period={localUsagePeriod}
          />
        )}
        {current === "speed" && speed && (
          <SpeedTab speed={speed} />
        )}
        {current === "cost" && (
          <>
            {cost && <CostRows cost={cost} />}
            {costHistory.length > 0 && (
              <CostHistoryChart
                data={costHistory}
                title={t("DetailChartCost")}
                ariaLabel={t("DetailChartCost")}
                animations={animations}
                emptyMessage={emptyMsg}
                days={days}
              />
            )}
          </>
        )}
        {current === "credits" && (
          <CreditsHistoryChart
            data={creditsHistory}
            title={t("DetailChartCredits")}
            ariaLabel={t("DetailChartCredits")}
            animations={animations}
            emptyMessage={emptyMsg}
            days={days}
          />
        )}
        {current === "usage" && (
          <UsageBreakdownChart
            data={usageBreakdown}
            title={t("DetailChartUsageBreakdown")}
            ariaLabel={t("DetailChartUsageBreakdown")}
            animations={animations}
            emptyMessage={emptyMsg}
            days={days}
          />
        )}
      </div>
    </section>
  );
}

/** Output-speed tab: latest / average rows plus the recent-samples chart. */
function SpeedTab({ speed }: { speed: ProviderOutputSpeed }) {
  const { t } = useLocale();
  const samples = speed.recentSamples;
  const average = useMemo(
    () =>
      samples.length > 0
        ? samples.reduce((sum, sample) => sum + sample.tokensPerSecond, 0) /
          samples.length
        : null,
    [samples],
  );
  const latest = samples[samples.length - 1]?.tokensPerSecond ?? null;
  const chartData = useMemo(
    () =>
      samples.map((sample) => ({
        label: new Date(sample.completedAtMs).toLocaleTimeString([], {
          hour: "2-digit",
          minute: "2-digit",
          second: "2-digit",
        }),
        value: sample.tokensPerSecond,
      })),
    [samples],
  );

  return (
    <div className="provider-detail-stats__speed">
      <div className="provider-output-speed__average">
        <span>{t("OutputSpeedLatestReply")}</span>
        <strong>
          {latest == null ? "—" : latest.toFixed(1)}
          {latest != null && <small> t/s</small>}
        </strong>
      </div>
      <div className="provider-output-speed__average">
        <span>{t("OutputSpeedRecentAverage")}</span>
        <strong>{average == null ? "—" : `${average.toFixed(1)} t/s`}</strong>
      </div>
      <BarChart
        data={chartData}
        height={90}
        color="var(--accent)"
        valueFormatter={(value) => `${value.toFixed(1)} t/s`}
        ariaLabel={t("OutputSpeedChartAriaLabel")}
        emptyMessage={t("OutputSpeedNoHistory")}
        referenceValue={average ?? undefined}
        referenceLabel={t("OutputSpeedAverageLine")}
      />
    </div>
  );
}

/** Cost tab rows: used / limit / reset, mirroring the old CostSection. */
function CostRows({ cost }: { cost: CostSnapshotBridge }) {
  const { t } = useLocale();
  const rows: { label: string; value: string | null }[] = [
    { label: t("DetailCostUsed"), value: cost.formattedUsed },
    { label: t("DetailCostLimit"), value: cost.formattedLimit },
    { label: t("DetailCostResets"), value: cost.resetsAt ?? null },
  ];
  const visible = rows.filter(
    (r): r is { label: string; value: string } =>
      !!r.value && r.value.length > 0,
  );

  return (
    <dl className="provider-detail-grid">
      {visible.map((r) => (
        <div key={r.label} style={{ display: "contents" }}>
          <dt>{r.label}</dt>
          <dd>{r.value}</dd>
        </div>
      ))}
    </dl>
  );
}
