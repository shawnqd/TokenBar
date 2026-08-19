import { describe, expect, it } from "vitest";
import {
  FIXTURE_SNAPSHOTS,
  quotaFillPercent,
  sourceHealthFromDisplayState,
} from "./index";

describe("quotaFillPercent", () => {
  it("never turns unknown usage into 1.0 or 100%", () => {
    const unknown = FIXTURE_SNAPSHOTS.unknownUsage.windows.find(
      (window) => window.id === "reset-only",
    );
    expect(unknown?.usageKnown).toBe(false);
    expect(unknown?.remainingPercent).toBe(100);
    expect(quotaFillPercent(unknown!, true)).toBeNull();
    expect(quotaFillPercent(unknown!, false)).toBeNull();
  });

  it("returns a real fill only for known quota windows", () => {
    const weekly = FIXTURE_SNAPSHOTS.kimiSessionWeekly.windows.find(
      (window) => window.kind === "weekly",
    );
    expect(weekly).toBeTruthy();
    expect(quotaFillPercent(weekly!, true)).toBe(62);
    expect(quotaFillPercent(weekly!, false)).toBe(38);
  });
});

describe("sourceHealthFromDisplayState", () => {
  it("maps display states without inventing a fresh health on errors", () => {
    expect(sourceHealthFromDisplayState("ready")).toBe("fresh");
    expect(sourceHealthFromDisplayState("refreshing")).toBe("fresh");
    expect(sourceHealthFromDisplayState("stale")).toBe("stale");
    expect(sourceHealthFromDisplayState("error")).toBe("error");
    expect(sourceHealthFromDisplayState("authRequired")).toBe("error");
    expect(sourceHealthFromDisplayState("notConfigured")).toBe("notConfigured");
    expect(sourceHealthFromDisplayState("loading")).toBe("unknown");
    expect(sourceHealthFromDisplayState("unknown")).toBe("unknown");
    expect(sourceHealthFromDisplayState("unsupported")).toBe("unknown");
  });
});
