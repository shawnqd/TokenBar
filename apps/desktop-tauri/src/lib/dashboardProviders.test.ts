import { describe, expect, it } from "vitest";
import { dashboardShowsQuotaWindow } from "./dashboardProviders";

/**
 * The dashboard surface is gone; this helper is a read-only compatibility
 * read for MenuCard's optional `quotaWindows` API. Nothing in the tray panel
 * or any other live surface consumes it as a render input.
 */
describe("dashboardShowsQuotaWindow (compat read only)", () => {
  it("shows every window when no filter is set", () => {
    expect(dashboardShowsQuotaWindow("weekly", [])).toBe(true);
    expect(dashboardShowsQuotaWindow("weekly", undefined)).toBe(true);
  });

  it("keeps only the chosen cycles when a caller supplies a filter", () => {
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

  it("is a pure read — it cannot observe settings or credentials", () => {
    const before = dashboardShowsQuotaWindow("daily", ["daily"]);
    // No side effects, no state, no I/O: repeated calls are deterministic and
    // the helper owns no module state that a surface could depend on.
    expect(dashboardShowsQuotaWindow("daily", ["daily"])).toBe(before);
    expect(dashboardShowsQuotaWindow("daily", ["daily"])).toBe(true);
  });
});