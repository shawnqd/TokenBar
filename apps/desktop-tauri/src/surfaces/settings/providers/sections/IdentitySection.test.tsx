import { render, screen } from "@testing-library/react";
import { describe, expect, it } from "vitest";
import type { ProviderDetail, RateWindowSnapshot } from "../../../../types/bridge";
import { IdentitySection } from "./IdentitySection";

function rateWindow(resetDescription: string): RateWindowSnapshot {
  return {
    usedPercent: 0,
    remainingPercent: 100,
    windowMinutes: null,
    resetsAt: null,
    resetDescription,
    isExhausted: false,
    reservePercent: null,
    reserveDescription: null,
  };
}

function detail(id: "deepseek" | "mimo"): ProviderDetail {
  const isMiMo = id === "mimo";
  return {
    id,
    displayName: isMiMo ? "Xiaomi MiMo Token Plan" : "DeepSeek",
    enabled: true,
    email: null,
    plan: isMiMo ? "lite (expired)" : "CNY balance: ¥38.81",
    authType: null,
    sourceLabel: isMiMo ? "web" : "api",
    organization: null,
    lastUpdated: "2026-07-13T00:00:00Z",
    session: rateWindow(
      isMiMo
        ? "No active MiMo Token Plan"
        : "¥38.81 (Paid: ¥38.81 / Granted: ¥0.00)",
    ),
    weekly: isMiMo
      ? rateWindow("34.46 CNY balance (Paid: 34.46 CNY / Granted: 0.00 CNY)")
      : null,
    modelSpecific: null,
    tertiary: null,
    extraRateWindows: [],
    cost: null,
    pace: null,
    lastError: null,
    dashboardUrl: null,
    statusPageUrl: null,
    buyCreditsUrl: null,
    hasSnapshot: true,
    cookieSource: null,
    region: null,
  };
}

describe("IdentitySection balance-only providers", () => {
  it.each([
    ["deepseek", "CNY balance: ¥38.81", "api"],
    ["mimo", "lite (expired)", "web"],
  ] as const)("hides the %s plan echo; the subtitle carries the source", (id, plan, source) => {
    // The parent always builds the subtitle as "{sourceLabel} · {updated}",
    // so the identity grid no longer repeats the data source as its own row.
    render(
      <IdentitySection
        provider={detail(id)}
        subtitle={`${source} · 刚刚更新`}
        t={(key) => key}
      />,
    );

    expect(screen.queryByText(plan)).not.toBeInTheDocument();
    expect(screen.getByText(`${source} · 刚刚更新`)).toBeInTheDocument();
    expect(screen.queryByText("DataSource")).not.toBeInTheDocument();
  });
});
