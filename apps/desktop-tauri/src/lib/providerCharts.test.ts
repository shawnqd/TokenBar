import { describe, expect, it } from "vitest";
import { providerSupportsChartData } from "./providerCharts";

describe("providerSupportsChartData", () => {
  it("treats every provider uniformly so the UI always fetches backend data", () => {
    // The UI asks for chart/local-usage data for every provider through the
    // same path; the backend (scan_local_cost / get_daily_cost_history /
    // load_openai_dashboard_chart_data) returns real data where it has a
    // parser and empty/zero otherwise. No provider hides the slots by list.
    for (const id of [
      "codex",
      "claude",
      "openai",
      "grok",
      "copilot",
      "cursor",
      "deepseek",
      "opencodego",
    ]) {
      expect(providerSupportsChartData(id)).toBe(true);
    }
  });
});
