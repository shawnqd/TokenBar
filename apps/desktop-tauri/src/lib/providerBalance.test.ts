import { describe, expect, it } from "vitest";
import type { ProviderUsageSnapshot } from "../types/bridge";
import { getProviderBalance } from "./providerBalance";

function mimoApiSnapshot(description: string): ProviderUsageSnapshot {
  return {
    providerId: "mimoapi",
    displayName: "Xiaomi MiMo API",
    primary: {
      usedPercent: 0,
      remainingPercent: 100,
      windowMinutes: null,
      resetsAt: null,
      resetDescription: description,
      isExhausted: false,
      reservePercent: null,
      reserveDescription: null,
    },
    primaryLabel: "API Key",
    secondary: null,
    modelSpecific: null,
    tertiary: null,
    extraRateWindows: [],
    cost: null,
    planName: null,
    accountEmail: null,
    sourceLabel: "api",
    updatedAt: "2026-07-13T00:00:00Z",
    error: null,
    pace: null,
    accountOrganization: null,
    trayStatusLabel: null,
    fetchDurationMs: null,
  };
}

describe("MiMo API balance presentation", () => {
  it("renders a console balance as a balance, not an API-status sentence", () => {
    const result = getProviderBalance(
      mimoApiSnapshot("34.46 CNY balance (Paid: 34.46 CNY / Granted: 0.00 CNY)"),
    );

    expect(result.balance).toMatchObject({
      kind: "balance",
      amount: "¥34.46",
      breakdown: "含赠送 ¥0.00",
    });
    expect(result.excludeWindows).toEqual(new Set(["primary"]));
  });

  it("keeps a successful API-key probe in the API status section", () => {
    const result = getProviderBalance(
      mimoApiSnapshot("API key valid; 7 models available. Balance is shown in the MiMo console"),
    );

    expect(result.balance).toMatchObject({
      kind: "status",
      amount: "API key valid; 7 models available. Balance is shown in the MiMo console",
    });
  });
});

describe("MiMo unified balance presentation", () => {
  it("uses the same balance block when no Token Plan is active", () => {
    const snapshot = mimoApiSnapshot("No active MiMo Token Plan");
    snapshot.providerId = "mimo";
    snapshot.displayName = "Xiaomi MiMo";
    snapshot.primaryLabel = "Tokens";
    snapshot.planName = "MiMo Token Plan";
    snapshot.secondary = {
      ...snapshot.primary,
      resetDescription: "34.46 CNY balance (Paid: 34.46 CNY / Granted: 0.00 CNY)",
    };

    const result = getProviderBalance(snapshot);

    expect(result.balance).toMatchObject({
      kind: "balance",
      title: "余额",
      amount: "¥34.46",
      breakdown: "含赠送 ¥0.00",
    });
    expect(result.excludeWindows).toEqual(new Set(["primary", "secondary"]));
    expect(result.suppressPlanBadge).toBe(true);
  });
});

describe("DeepSeek balance presentation", () => {
  it("recognizes the paid/granted amount instead of rendering its synthetic quota", () => {
    const snapshot = mimoApiSnapshot("¥38.81 (Paid: ¥38.81 / Granted: ¥0.00)");
    snapshot.providerId = "deepseek";
    snapshot.primaryLabel = "Balance";
    snapshot.planName = "CNY balance: ¥38.81";

    const result = getProviderBalance(snapshot);

    expect(result.balance).toMatchObject({
      kind: "balance",
      title: "余额",
      amount: "¥38.81",
    });
    expect(result.excludeWindows).toEqual(new Set(["primary"]));
    expect(result.suppressPlanBadge).toBe(true);
  });

  it("normalizes prefix and full-width CNY symbols to the same currency format", () => {
    const snapshot = mimoApiSnapshot("CNY ¥38.81 (Paid: ￥38.81 / Granted: ￥0.00)");
    snapshot.providerId = "deepseek";

    expect(getProviderBalance(snapshot).balance).toMatchObject({
      amount: "¥38.81",
      breakdown: "含赠送 ¥0.00",
    });
  });
});
