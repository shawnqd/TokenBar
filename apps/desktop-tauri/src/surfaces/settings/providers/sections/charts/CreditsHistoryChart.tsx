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
 * Port target: the credits_history line/area in
 * `rust/src/native_ui/preferences.rs::render_provider_detail_panel`.
 */
export function CreditsHistoryChart({
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
        valueFormatter={(v) => v.toFixed(1)}
        animations={animations}
        emptyMessage={emptyMessage}
      />
    </div>
  );
}
