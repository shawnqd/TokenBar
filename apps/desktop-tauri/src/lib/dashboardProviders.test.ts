import { describe, expect, it } from "vitest";
import { dashboardShowsQuotaWindow } from "./dashboardProviders";

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
