import { beforeEach, describe, expect, it, vi } from "vitest";
import { render, waitFor } from "@testing-library/react";
import { fromBridge } from "../../core";

const tauriMocks = vi.hoisted(() => ({
  getLocaleStrings: vi.fn(),
  openProviderDashboard: vi.fn(),
  openProviderStatusPage: vi.fn(),
  invokeSurfaceAction: vi.fn(async () => "ok"),
}));

vi.mock("../../lib/tauri", () => tauriMocks);

import { LocaleProvider } from "../../i18n/LocaleProvider";
import { buildBundle } from "../../test/localeHarness";
import TrayCard from "./TrayCard";
import type { ProviderSnapshot } from "../../core";
import type { ProviderUsageSnapshot, RateWindowSnapshot } from "../../types/bridge";

function rw(usedPercent: number, windowMinutes: number | null, kind: string | null): RateWindowSnapshot {
  return {
    usedPercent, remainingPercent: 100 - usedPercent, kind: kind as any, windowMinutes,
    resetsAt: null, resetDescription: null, isExhausted: false, reservePercent: null, reserveDescription: null,
  };
}

/** Grok-shaped bridge: only a monthly quota plus a local-usage capability. */
function grokBridge(): ProviderUsageSnapshot {
  return {
    providerId: "grok",
    displayName: "Grok",
    primary: rw(33, 30 * 24 * 60, "monthly"),
    primaryLabel: "Monthly",
    secondary: null,
    modelSpecific: null,
    tertiary: null,
    extraRateWindows: [],
    cost: null, planName: null, accountEmail: null, sourceLabel: "auto",
    updatedAt: "2026-09-16T00:00:00Z", error: null, pace: null, accountOrganization: null,
    trayStatusLabel: null, fetchDurationMs: null,
  };
}

const DISPLAY = {
  showAsUsed: true,
  resetTimeRelative: true,
  highUsageThreshold: 75,
  criticalUsageThreshold: 95,
} as const;

/** grok's real shape: last local usage 09-06, viewed 09-16. 7d is empty,
 *  30d carries real log-measured tokens — the card must widen instead of
 *  rendering an empty slot, with the label following the shown window. */
function grokChartData(sevenDay: number | null = null, thirtyDay = 218_205_427) {
  return {
    providerId: "grok",
    costHistory: [],
    creditsHistory: [],
    usageBreakdown: [],
    localUsage: {
      todayCost: null,
      todayTokens: null,
      sevenDayCost: null,
      sevenDayTokens: sevenDay,
      thirtyDayCost: 367.55,
      thirtyDayTokens: thirtyDay,
      todayTopModel: null,
      sevenDayTopModel: null,
      thirtyDayTopModel: "grok-4.6-build",
      estimateNote: "Estimated from local logs",
    },
  };
}

function grokCapable() {
  const core = fromBridge(grokBridge());
  return {
    ...core,
    capabilities: {
      ...core.capabilities,
      supportsCharts: true,
      supportsLocalCost: true,
    },
  };
}

function renderCard(provider: ProviderSnapshot, props: Record<string, unknown> = {}) {
  return render(
    <LocaleProvider>
      <TrayCard
        provider={provider}
        display={{ ...DISPLAY }}
        densityMode="detailed"
        {...props}
      />
    </LocaleProvider>,
  );
}

describe("TrayCard local usage window upgrade + error resilience", () => {
  beforeEach(() => {
    tauriMocks.getLocaleStrings.mockResolvedValue(buildBundle({
      PanelSevenDayUsage: "近 7 天使用",
      PanelThirtyDayUsage: "近 30 天使用",
      PanelTodayUsage: "今日使用",
      TaskbarWidgetPreviewSpeed: "输出速度",
      PanelApiEquivalentValue: "API 等值价值",
      StatusUnableToGetUsage: "无法获取用量",
      TrayStatusError: "错误",
    }));
    tauriMocks.openProviderDashboard.mockResolvedValue(undefined);
    tauriMocks.openProviderStatusPage.mockResolvedValue(undefined);
  });

  it("widens a empty 7d window to the real 30d reading instead of an empty slot", async () => {
    const { container } = renderCard(grokCapable(), {
      localUsagePeriod: "7d",
      chartData: grokChartData(null, 218_205_427),
    });
    await waitFor(() => {
      if (!container.querySelector(".tray-card")) throw new Error("card not rendered");
    });
    const usage = container.querySelector('[data-slot="usage"]');
    expect(usage).not.toBeNull();
    expect(usage?.textContent).toBe("≈ 218.2M");
    expect(container.textContent).toContain("近 30 天");
  });

  it("a provider fetch error no longer suppresses the real local usage read", async () => {
    const core = grokCapable();
    const { container } = renderCard(
      { ...core, error: "Grok Error: No cookies available for web API", updatedAt: null } as any,
      {
        localUsagePeriod: "7d",
        chartData: grokChartData(null, 218_205_427),
      },
    );
    await waitFor(() => {
      if (!container.querySelector(".tray-card")) throw new Error("card not rendered");
    });
    // The error is announced, but the measured local usage is still shown.
    expect(container.querySelector('[data-slot="usage"]')).not.toBeNull();
    expect(container.textContent).toContain("近 30 天");
  });

  it("keeps the exact window when it does carry data", async () => {
    const { container } = renderCard(grokCapable(), {
      localUsagePeriod: "7d",
      chartData: grokChartData(48_000, 218_205_427),
    });
    await waitFor(() => {
      if (!container.querySelector(".tray-card")) throw new Error("card not rendered");
    });
    expect(container.querySelector('[data-slot="usage"]')?.textContent).toBe("≈ 48K");
    expect(container.textContent).toContain("近 7 天");
  });
});
