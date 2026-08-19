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

function rw(usedPercent: number, windowMinutes: number | null, kind: string | null, resetsAt: string | null = null): RateWindowSnapshot {
  return {
    usedPercent, remainingPercent: 100 - usedPercent, kind: kind as any, windowMinutes,
    resetsAt, resetDescription: null, isExhausted: false, reservePercent: null, reserveDescription: null,
  };
}

/** OpenCode Go's real snapshot shape: 5h rolling (primary) + weekly
 *  (secondary) + monthly (tertiary) + a date-only "Renews" marker. */
function opencodeSnapshot(): ProviderUsageSnapshot {
  return {
    providerId: "opencodego",
    displayName: "OpenCode Go",
    primary: rw(72, 300, "session"),
    primaryLabel: "rolling",
    secondary: rw(58, 10080, "weekly"),
    secondaryLabel: "weekly",
    modelSpecific: null,
    tertiary: rw(45, 43200, "monthly"),
    extraRateWindows: [
      { id: "renewal", title: "Renews", window: rw(0, null, null, "2026-08-22T00:00:00Z"), usageKnown: true },
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
    }));
  });

  it("renders exactly the three real cycles — 5h hero, weekly secondary, monthly tile", async () => {
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
    expect(labels).toEqual(["5 小时额度", "周额度", "月额度"]);
    const heroPct = container.querySelector(".quota-row__hero-pct");
    expect(heroPct?.textContent).toBe("72%");
    // The date-only "Renews" marker must never become a tile with a fake bar.
    expect(container.textContent).not.toContain("Renews");
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
    expect(container.textContent).not.toContain("Renews");
  });
});