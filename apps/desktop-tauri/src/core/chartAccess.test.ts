import { beforeEach, describe, expect, it, vi } from "vitest";

const tauriMocks = vi.hoisted(() => ({
  getProviderChartData: vi.fn(),
  getProviderLocalUsageSummary: vi.fn(),
}));

vi.mock("../lib/tauri", () => tauriMocks);

import { clearChartCache, defaultChartLoader } from "./chartAccess";

const summary = {
  todayCost: 0.1,
  todayTokens: 10,
  sevenDayCost: 0.7,
  sevenDayTokens: 70,
  thirtyDayCost: 2,
  thirtyDayTokens: 200,
  todayTopModel: "gpt-5",
  sevenDayTopModel: "gpt-5",
  thirtyDayTopModel: "gpt-5",
  estimateNote: "estimated",
};

function chart(localUsage: typeof summary | null = null) {
  return {
    providerId: "codex",
    costHistory: [{ date: "2026-09-01", value: 1 }],
    creditsHistory: [],
    usageBreakdown: [],
    localUsage,
  };
}

describe("defaultChartLoader", () => {
  beforeEach(() => {
    clearChartCache();
    vi.clearAllMocks();
    tauriMocks.getProviderChartData.mockResolvedValue(chart());
    tauriMocks.getProviderLocalUsageSummary.mockResolvedValue(summary);
  });

  it("fills a bounded chart bundle with the authoritative local usage summary", async () => {
    const result = await defaultChartLoader("codex");

    expect(result?.localUsage).toEqual(summary);
    expect(tauriMocks.getProviderChartData).toHaveBeenCalledWith(
      "codex",
      undefined,
    );
    expect(tauriMocks.getProviderLocalUsageSummary).toHaveBeenCalledWith(
      "codex",
    );
  });

  it("keeps real local usage visible even when the chart command fails", async () => {
    tauriMocks.getProviderChartData.mockRejectedValue(new Error("chart timeout"));

    const result = await defaultChartLoader("codex");

    expect(result).toMatchObject({
      providerId: "codex",
      costHistory: [],
      localUsage: summary,
    });
  });

  it("coalesces concurrent reads for the same provider key", async () => {
    let resolveChart!: (value: ReturnType<typeof chart>) => void;
    tauriMocks.getProviderChartData.mockReturnValue(
      new Promise((resolve) => {
        resolveChart = resolve;
      }),
    );

    const first = defaultChartLoader("codex");
    const second = defaultChartLoader("codex");
    resolveChart(chart());

    await expect(Promise.all([first, second])).resolves.toHaveLength(2);
    expect(tauriMocks.getProviderChartData).toHaveBeenCalledTimes(1);
    expect(tauriMocks.getProviderLocalUsageSummary).toHaveBeenCalledTimes(1);
  });
});
