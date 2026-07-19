import { BarChart } from "../../../../../components/charts/BarChart";
import type { DailyUsageBreakdown } from "../../../../../types/bridge";

interface Props {
  data: DailyUsageBreakdown[];
  title: string;
  ariaLabel: string;
  animations: boolean;
  emptyMessage: string;
  days: number;
}

/** A height-stable daily total chart shared by every selected range. */
export function UsageBreakdownChart({
  data,
  title,
  ariaLabel,
  animations,
  emptyMessage,
  days,
}: Props) {
  const points = data.slice(-days).map((day) => ({
    label: day.day,
    value: day.totalCreditsUsed,
  }));

  return (
    <div className="provider-detail-chart">
      <div className="provider-detail-chart__title">{title}</div>
      <BarChart
        data={points}
        color="var(--accent)"
        height={90}
        ariaLabel={ariaLabel}
        valueFormatter={(value) => value.toFixed(1)}
        animations={animations}
        emptyMessage={emptyMessage}
      />
    </div>
  );
}
