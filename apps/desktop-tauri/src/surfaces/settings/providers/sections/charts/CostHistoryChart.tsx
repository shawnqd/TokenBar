import { BarChart } from "../../../../../components/charts/BarChart";
import type { DailyCostPoint } from "../../../../../types/bridge";

interface Props {
  data: DailyCostPoint[];
  title: string;
  ariaLabel: string;
  animations: boolean;
  emptyMessage: string;
  days: number;
}

/**
 * Port target: the cost_history bar cluster in
 * `rust/src/native_ui/preferences.rs::render_provider_detail_panel`.
 * Phase 10 wires through per-provider palette tokens + animation flags.
 */
export function CostHistoryChart({
  data,
  title,
  ariaLabel,
  animations,
  emptyMessage,
  days,
}: Props) {
  const recent = data.slice(-days);
  const points = recent.map((p) => ({ label: p.date, value: p.value }));
  return (
    <div className="provider-detail-chart">
      <div className="provider-detail-chart__title">{title}</div>
      <BarChart
        data={points}
        color="var(--accent)"
        height={90}
        ariaLabel={ariaLabel}
        valueFormatter={(v) => `$${v.toFixed(2)}`}
        animations={animations}
        emptyMessage={emptyMessage}
      />
    </div>
  );
}
