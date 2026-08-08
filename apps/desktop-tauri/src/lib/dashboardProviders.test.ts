import { describe, expect, it } from "vitest";
import {
  dashboardShowsQuotaWindow,
  resolveDashboardProviderIds,
} from "./dashboardProviders";

describe("dashboardShowsQuotaWindow", () => {
  it("shows every window when no filter is set", () => {
    expect(dashboardShowsQuotaWindow("weekly", [])).toBe(true);
    expect(dashboardShowsQuotaWindow("weekly", undefined)).toBe(true);
  });

  it("keeps only the chosen cycles", () => {
    expect(dashboardShowsQuotaWindow("weekly", ["weekly"])).toBe(true);
    expect(dashboardShowsQuotaWindow("session", ["weekly"])).toBe(false);
  });

  it("never hides a window that is not a cycle", () => {
    // A prepaid balance, an API-key status, a credits count. A filter phrased
    // in cycles has nothing to say about them, and for a balance-type provider
    // hiding them would empty the card.
    expect(dashboardShowsQuotaWindow(null, ["weekly"])).toBe(true);
    expect(dashboardShowsQuotaWindow(undefined, ["weekly"])).toBe(true);
  });
});

describe("resolveDashboardProviderIds", () => {
  const enabled = ["claude", "codex", "gemini"];

  it("shows every candidate when no filter is set", () => {
    expect(resolveDashboardProviderIds(enabled, [])).toEqual(enabled);
    expect(resolveDashboardProviderIds(enabled, undefined)).toEqual(enabled);
  });

  it("keeps only the filtered providers, in the candidate order", () => {
    expect(resolveDashboardProviderIds(enabled, ["gemini", "claude"])).toEqual([
      "claude",
      "gemini",
    ]);
  });

  it("cannot re-add a provider that is not a candidate", () => {
    expect(resolveDashboardProviderIds(["claude"], ["claude", "codex"])).toEqual([
      "claude",
    ]);
  });

  it("falls back to everything rather than rendering a blank dashboard", () => {
    // Every filtered id has since been disabled. A blank panel carries no
    // controls, so there would be no way back to the setting from there.
    expect(resolveDashboardProviderIds(enabled, ["retired-provider"])).toEqual(
      enabled,
    );
  });
});
