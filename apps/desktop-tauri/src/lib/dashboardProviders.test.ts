import { describe, expect, it } from "vitest";
import { resolveDashboardProviderIds } from "./dashboardProviders";

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
