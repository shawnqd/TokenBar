import { fireEvent, render, screen, waitFor } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";

const tauriMocks = vi.hoisted(() => ({
  getProviderChartData: vi.fn(),
  getSettingsSnapshot: vi.fn(),
}));

vi.mock("../../../../lib/tauri", () => tauriMocks);
vi.mock("../../../../hooks/useLocale", () => ({
  useLocale: () => ({ t: (key: string) => key }),
}));

import type { ProviderOutputSpeed } from "../../../../types/bridge";
import { StatsSection } from "./StatsSection";

function speed(): ProviderOutputSpeed {
  return {
    providerId: "codex",
    status: "recent",
    tokensPerSecond: 30,
    outputTokens: 90,
    updatedAtMs: 2_000,
    approximate: true,
    recentSamples: [
      {
        tokensPerSecond: 20,
        outputTokens: 40,
        durationMs: 2_000,
        completedAtMs: 1_000,
        model: null,
      },
      {
        tokensPerSecond: 30,
        outputTokens: 90,
        durationMs: 3_000,
        completedAtMs: 2_000,
        model: null,
      },
    ],
  };
}

function renderSection(overrides: Partial<Parameters<typeof StatsSection>[0]> = {}) {
  return render(
    <StatsSection
      providerId="codex"
      accountEmail={null}
      speed={null}
      cost={null}
      localUsagePeriod="7d"
      {...overrides}
    />,
  );
}

describe("StatsSection", () => {
  beforeEach(() => {
    vi.clearAllMocks();
    tauriMocks.getSettingsSnapshot.mockResolvedValue({ enableAnimations: false });
    tauriMocks.getProviderChartData.mockResolvedValue({
      providerId: "codex",
      costHistory: Array.from({ length: 100 }, (_, index) => ({
        date: `day-${String(index + 1).padStart(3, "0")}`,
        value: index + 1,
      })),
      creditsHistory: [{ date: "day-100", value: 10 }],
      usageBreakdown: [{ day: "day-100", totalCreditsUsed: 10, services: [] }],
      localUsage: {
        todayCost: null,
        todayTokens: null,
        sevenDayCost: 1.23,
        sevenDayTokens: 584_000,
        thirtyDayCost: null,
        thirtyDayTokens: null,
        topModel: null,
        estimateNote: "Estimated from local logs",
      },
    });
  });

  it("renders nothing when no stat data exists", async () => {
    tauriMocks.getProviderChartData.mockResolvedValue({
      providerId: "codex",
      costHistory: [],
      creditsHistory: [],
      usageBreakdown: [],
      localUsage: null,
    });

    const { container } = renderSection();

    await waitFor(() => {
      expect(tauriMocks.getProviderChartData).toHaveBeenCalled();
    });
    expect(container.querySelector(".provider-detail-stats")).not.toBeInTheDocument();
  });

  it("shows one stats panel at a time and switches via the tab buttons", async () => {
    renderSection({ speed: speed() });

    // Tokens tab leads (first available); speed/cost/credits/usage stay hidden
    // until their tab button is clicked.
    expect(await screen.findByText("584,000")).toBeInTheDocument();
    expect(screen.queryByRole("img", { name: "OutputSpeedChartAriaLabel" })).not.toBeInTheDocument();
    expect(screen.queryByRole("img", { name: "DetailChartCost" })).not.toBeInTheDocument();

    fireEvent.click(screen.getByRole("tab", { name: "OutputSpeedTitle" }));
    expect(screen.getByText("30.0")).toBeInTheDocument();
    expect(screen.getByText("25.0 t/s")).toBeInTheDocument();
    expect(screen.queryByText("584,000")).not.toBeInTheDocument();

    fireEvent.click(screen.getByRole("tab", { name: "DetailCostTitle" }));
    expect(screen.getByRole("img", { name: "DetailChartCost" })).toBeInTheDocument();
    expect(screen.queryByText("30.0")).not.toBeInTheDocument();

    fireEvent.click(screen.getByRole("tab", { name: "DetailChartUsageBreakdown" }));
    expect(
      screen.getByRole("img", { name: "DetailChartUsageBreakdown" }),
    ).toBeInTheDocument();
    expect(screen.queryByRole("img", { name: "DetailChartCost" })).not.toBeInTheDocument();
  });

  it("keeps the range switcher scoped to the dated chart tabs", async () => {
    renderSection();

    fireEvent.click(await screen.findByRole("tab", { name: "DetailCostTitle" }));
    await waitFor(() => {
      expect(screen.getByText("y-071")).toBeInTheDocument();
    });

    fireEvent.click(screen.getByRole("radio", { name: "FloatBarSevenDayShort" }));
    expect(screen.getByText("y-094")).toBeInTheDocument();

    // The token stats tab has no dated chart — the range row disappears.
    fireEvent.click(screen.getByRole("tab", { name: "DetailTokensTab" }));
    expect(screen.queryByRole("radiogroup")).not.toBeInTheDocument();
  });

  it("renders cost snapshot rows inside the cost tab", async () => {
    renderSection({
      cost: {
        used: 12.5,
        limit: 50,
        remaining: 37.5,
        currencyCode: "USD",
        period: "May",
        resetsAt: "in 3 days",
        formattedUsed: "$12.50",
        formattedLimit: "$50.00",
      },
    });

    fireEvent.click(await screen.findByRole("tab", { name: "DetailCostTitle" }));

    expect(screen.getByText("$12.50")).toBeInTheDocument();
    expect(screen.getByText("$50.00")).toBeInTheDocument();
    expect(screen.getByText("in 3 days")).toBeInTheDocument();
  });
});
