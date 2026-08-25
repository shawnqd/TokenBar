import { beforeEach, describe, expect, it, vi } from "vitest";
import { render, waitFor } from "@testing-library/react";

const tauriMocks = vi.hoisted(() => ({
  getLocaleStrings: vi.fn(),
  getProviderChartData: vi.fn().mockResolvedValue({ providerId: "opencodego", costHistory: [], creditsHistory: [], usageBreakdown: [], localUsage: null }),
}));

vi.mock("../../lib/tauri", () => tauriMocks);

import { LocaleProvider } from "../../i18n/LocaleProvider";
import { buildBundle } from "../../test/localeHarness";
import TrayCard from "./TrayCard";
import type { ProviderUsageSnapshot, RateWindowSnapshot } from "../../types/bridge";

function rw(usedPercent: number, windowMinutes: number | null, kind: string | null, resetsAt: string | null = null, resetDescription: string | null = null): RateWindowSnapshot {
  return {
    usedPercent, remainingPercent: 100 - usedPercent, kind: kind as any, windowMinutes,
    resetsAt, resetDescription, isExhausted: false, reservePercent: null, reserveDescription: null,
  };
}

/** OpenCode Go's real snapshot shape: 5h rolling (primary) + weekly
 *  (secondary) + monthly (tertiary) + a date-only "Renews" marker. */
function opencodeSnapshot(): ProviderUsageSnapshot {
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

describe("TrayCard OpenCode Go projection", () => {
  beforeEach(() => {
    tauriMocks.getLocaleStrings.mockResolvedValue(buildBundle({
      ProviderSessionLabel: "5 小时额度",
      ProviderWeeklyLabel: "周额度",
      ProviderMonthlyLabel: "月额度",
      DetailWindowModelSpecific: "模型专属",
      ResetsInDaysHours: "{}d {}h",
      ResetsInHoursMinutes: "{}h {}m",
    }));
  });

  it("renders exactly the three real cycles — 5h hero, weekly + monthly tiles, plus the zen balance block", async () => {
    const { container } = render(
      <LocaleProvider>
        <TrayCard
          provider={opencodeSnapshot()}
          densityMode="detailed"
          display={{ showAsUsed: true, resetTimeRelative: true, highUsageThreshold: 75, criticalUsageThreshold: 95 }}
        />
      </LocaleProvider>
    );
    await waitFor(() => {
      if (!container.querySelector(".tray-card")) throw new Error("card not rendered");
    });
    const labels = Array.from(container.querySelectorAll(".quota-row__label, .quota-tile__label"))
      .map((el) => el.textContent);
    // OpenCode Go layout: weekly is now a full tile in the grid, not the hero secondary.
    // Tiles use the short cycle labels (周/月) so label + reset share a line.
    expect(labels).toEqual(["5 小时额度", "周", "月"]);
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
    // empty slot. Capability comes from the stable snapshot, not fetch state,
    // so the card does not flicker the footer in and out while data loads.
    expect(container.querySelector(".meta-well")).toBeNull();
  });

  it("lone extra tile spans full width", async () => {
    const { container } = render(
      <LocaleProvider>
        <TrayCard
          provider={{ ...opencodeSnapshot(), secondary: null }}
          densityMode="detailed"
          display={{ showAsUsed: true, resetTimeRelative: true, highUsageThreshold: 75, criticalUsageThreshold: 95 }}
        />
      </LocaleProvider>
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
    const { container } = render(
      <LocaleProvider>
        <TrayCard
          provider={opencodeSnapshot()}
          densityMode="minimal"
          display={{ showAsUsed: true, resetTimeRelative: true, highUsageThreshold: 75, criticalUsageThreshold: 95 }}
        />
      </LocaleProvider>
    );
    await waitFor(() => {
      if (!container.querySelector(".tray-card")) throw new Error("card not rendered");
    });
    expect(container.querySelector(".minimal-streamlined__metric")?.textContent).toBe("72%");
    // Secondary + extra windows condense into one chip on the minimal tier.
    expect(container.textContent).toContain("周58%·月45%");
    expect(container.textContent).not.toContain("Renews");
  });
});