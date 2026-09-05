import { beforeEach, describe, expect, it, vi } from "vitest";
import { render, waitFor } from "@testing-library/react";
import { fromBridge, zaiChinaBalanceBridge } from "../../core";

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

function rw(usedPercent: number, windowMinutes: number | null, kind: string | null, resetsAt: string | null = null, resetDescription: string | null = null): RateWindowSnapshot {
  return {
    usedPercent, remainingPercent: 100 - usedPercent, kind: kind as any, windowMinutes,
    resetsAt, resetDescription, isExhausted: false, reservePercent: null, reserveDescription: null,
  };
}

/** OpenCode Go's real snapshot shape: 5h rolling (primary) + weekly
 *  (secondary) + monthly (tertiary) + a date-only "Renews" marker. */
function opencodeBridge(): ProviderUsageSnapshot {
  return {
    providerId: "opencodego",
    displayName: "OpenCode Go",
    primary: rw(72, 300, "session", "2026-08-16T17:00:00Z"),
    primaryLabel: "rolling",
    secondary: rw(58, 10080, "weekly", "2026-08-22T00:00:00Z"),
    secondaryLabel: "weekly",
    modelSpecific: null,
    tertiary: rw(45, 43200, "monthly", "2026-09-10T00:00:00Z"),
    extraRateWindows: [
      { id: "renewal", title: "Renews", window: rw(0, null, null, "2026-08-22T00:00:00Z"), usageKnown: true },
      { id: "zen-balance", title: "Zen balance", window: rw(0, null, null, null, "$38.80"), usageKnown: true },
    ],
    cost: null, planName: null, accountEmail: null, sourceLabel: "auto",
    updatedAt: "2026-08-16T12:00:00Z", error: null, pace: null, accountOrganization: null,
    trayStatusLabel: null, fetchDurationMs: null,
  };
}

export function opencodeCoreSnapshot(): ProviderSnapshot {
  return fromBridge(opencodeBridge());
}

const DISPLAY = {
  showAsUsed: true,
  resetTimeRelative: true,
  highUsageThreshold: 75,
  criticalUsageThreshold: 95,
} as const;

function renderCard(
  provider: ProviderSnapshot | ProviderUsageSnapshot,
  props: Partial<Parameters<typeof TrayCard>[0]> = {},
) {
  return render(
    <LocaleProvider>
      <TrayCard provider={provider} display={{ ...DISPLAY }} densityMode="detailed" {...props} />
    </LocaleProvider>,
  );
}

describe("TrayCard OpenCode Go projection (core read model)", () => {
  beforeEach(() => {
    tauriMocks.getLocaleStrings.mockResolvedValue(buildBundle({
      ProviderSessionLabel: "5 小时额度",
      ProviderWeeklyLabel: "周额度",
      ProviderMonthlyLabel: "月额度",
      DetailWindowModelSpecific: "模型专属",
      ResetsInDaysHours: "{}d {}h",
      ResetsInHoursMinutes: "{}h {}m",
    }));
    tauriMocks.openProviderDashboard.mockResolvedValue(undefined);
    tauriMocks.openProviderStatusPage.mockResolvedValue(undefined);
  });

  it("renders exactly the three real cycles from the core projection — 5h hero, weekly + monthly tiles, plus the zen balance block", async () => {
    const { container } = renderCard(opencodeCoreSnapshot(), { densityMode: "detailed" });
    await waitFor(() => {
      if (!container.querySelector(".tray-card")) throw new Error("card not rendered");
    });
    const labels = Array.from(container.querySelectorAll(".quota-row__label, .quota-tile__label"))
      .map((el) => el.textContent);
    // OpenCode Go layout: 5h hero on top, weekly + monthly as a 2-col pair.
    // Tiles use the short cycle labels (周/月) so label + reset share a line.
    expect(labels).toEqual(["5 小时额度", "周", "月"]);
    const grid = container.querySelector(".tiles-grid-2col");
    expect(grid?.querySelectorAll(".quota-tile")).toHaveLength(2);
    expect(grid?.querySelector(".quota-tile--full")).toBeNull();
    expect(container.querySelector(".quota-stage .quota-tile")).toBeNull();
    const heroPct = container.querySelector(".quota-row__hero-pct");
    expect(heroPct?.textContent).toBe("72%");
    // Zen balance surfaces through the balance block, not as a quota tile.
    const balanceAmount = container.querySelector(".balance-block__amount");
    expect(balanceAmount?.textContent).toBe("$38.80");
    // The date-only "Renews" marker must never become a tile with a fake bar.
    expect(container.textContent).not.toContain("Renews");
    // The zen-balance row's own title stays out of the UI; only 余额 amount renders.
    expect(container.textContent).not.toContain("Zen balance");
    const tileResets = Array.from(container.querySelectorAll(".quota-tile__reset"))
      .map((el) => el.textContent);
    expect(tileResets).toHaveLength(2);
    expect(tileResets.every((text) => text && text.length > 0)).toBe(true);
    // OpenCode Go has no local session-log scanner and no output-speed parser,
    // so the insight footer (speed | usage) must be hidden entirely — never an
    // empty slot. Capability comes from the core snapshot, not fetch state,
    // so the card does not flicker the footer in and out while data loads.
    expect(container.querySelector(".meta-well")).toBeNull();
  });

  it("still accepts the legacy bridge shape (Settings preview path) and projects it through core", async () => {
    const { container } = renderCard(opencodeBridge(), { densityMode: "detailed" });
    await waitFor(() => {
      if (!container.querySelector(".tray-card")) throw new Error("card not rendered");
    });
    const labels = Array.from(container.querySelectorAll(".quota-row__label, .quota-tile__label"))
      .map((el) => el.textContent);
    expect(labels).toEqual(["5 小时额度", "周", "月"]);
    expect(container.querySelector(".quota-row__hero-pct")?.textContent).toBe("72%");
    expect(container.querySelector(".balance-block__amount")?.textContent).toBe("$38.80");
    expect(container.textContent).not.toContain("Renews");
    expect(container.textContent).not.toContain("Zen balance");
  });

  it("lone extra tile spans full width", async () => {
    const { container } = renderCard(
      fromBridge({ ...opencodeBridge(), secondary: null }),
      { densityMode: "detailed" },
    );
    await waitFor(() => {
      if (!container.querySelector(".tray-card")) throw new Error("card not rendered");
    });
    // A single leftover tile must take both grid columns and keep label +
    // reset together on the title row of the full-width head.
    const fullTile = container.querySelector(".quota-tile--full");
    expect(fullTile).not.toBeNull();
    expect(fullTile?.querySelector(".quota-tile__title")).not.toBeNull();
    expect(fullTile?.querySelector(".quota-tile__reset")).not.toBeNull();
  });

  it("minimal tier keeps the hero metric and never invents a bar", async () => {
    const { container } = renderCard(opencodeCoreSnapshot(), { densityMode: "minimal" });
    await waitFor(() => {
      if (!container.querySelector(".tray-card")) throw new Error("card not rendered");
    });
    expect(container.querySelector(".minimal-streamlined__metric")?.textContent).toBe("72%");
    // Secondary + extra windows condense into one chip on the minimal tier.
    expect(container.textContent).toContain("周58%·月45%");
    expect(container.textContent).not.toContain("Renews");
  });

  it("renders the injected chart/usage enrichment when the provider has the local-usage capability", async () => {
    const provider = opencodeCoreSnapshot();
    const capable = {
      ...provider,
      capabilities: {
        ...provider.capabilities,
        supportsCharts: true,
        supportsLocalCost: true,
      },
    };
    const { container } = renderCard(capable, {
      densityMode: "detailed",
      localUsagePeriod: "7d",
      chartData: {
        providerId: "opencodego",
        costHistory: [],
        creditsHistory: [],
        usageBreakdown: [],
        localUsage: {
          todayCost: null,
          todayTokens: null,
          sevenDayCost: 0.42,
          sevenDayTokens: 48_000,
          thirtyDayCost: null,
          thirtyDayTokens: null,
          todayTopModel: null,
          sevenDayTopModel: "opencode-2",
          thirtyDayTopModel: null,
          estimateNote: "Estimated from local logs",
        },
      },
    });
    await waitFor(() => {
      if (!container.querySelector(".tray-card")) throw new Error("card not rendered");
    });
    // Data comes from the injected enrichment result, not a tauri chart call.
    expect(container.querySelector('[data-slot="usage"]')?.textContent).toBe("≈ 48K");
    expect(container.textContent).toContain("opencode-2");
  });
});

describe("TrayCard GLM BigModel CN balance projection", () => {
  beforeEach(() => {
    tauriMocks.getLocaleStrings.mockResolvedValue(buildBundle({
      AllSystemsOperational: "All systems operational",
      TrayStatusStale: "Stale",
      StatusUnableToGetUsage: "Unable to get usage",
      TrayLoading: "Loading",
    }));
  });

  it("renders the reference balance block in detailed, compact, and minimal density", async () => {
    for (const densityMode of ["detailed", "compact", "minimal"] as const) {
      const { container } = renderCard(zaiChinaBalanceBridge, { densityMode });
      await waitFor(() => {
        if (!container.querySelector(".tray-card")) throw new Error("card not rendered");
      });

      expect(container.textContent).toContain("¥12.50");
      if (densityMode === "detailed") {
        expect(container.querySelector(".balance-block")).not.toBeNull();
        expect(container.querySelector(".balance-block__head")?.textContent).toContain("余额");
        expect(container.querySelector(".balance-block__amount")?.textContent).toBe("¥12.50");
        expect(container.querySelector(".balance-block .soft-badge")).not.toBeNull();
      } else if (densityMode === "compact") {
        expect(container.querySelector(".balance-compact-row")).not.toBeNull();
        expect(container.querySelector(".balance-compact-row__amount")?.textContent).toBe("¥12.50");
        expect(container.querySelector(".balance-compact-row .soft-badge")).not.toBeNull();
      } else {
        expect(container.querySelector(".minimal-streamlined__metric")?.textContent).toBe("¥12.50");
        expect(container.querySelector(".minimal-streamlined .soft-badge")).not.toBeNull();
        expect(container.querySelector(".progress-bar")).toBeNull();
      }
    }
  });

  it("renders the balance after the core lifts the extra row into cost", async () => {
    const coreSnapshot = fromBridge(
      zaiChinaBalanceBridge,
      Date.parse("2026-08-16T12:05:00.000Z"),
    );
    const { container } = renderCard(coreSnapshot, { densityMode: "detailed" });
    await waitFor(() => {
      if (!container.querySelector(".tray-card")) throw new Error("card not rendered");
    });
    expect(container.querySelector(".balance-block__amount")?.textContent).toBe("¥12.50");
  });
});
