import { describe, expect, it } from "vitest";
import {
  FIXTURE_BRIDGES,
  FIXTURE_SNAPSHOTS,
  fromBridge,
  isRealQuotaWindow,
} from "./index";

const NOW = Date.parse("2026-08-16T12:05:00.000Z");

describe("fromBridge", () => {
  it("maps Kimi-like slots without swapping the stored primary/secondary order", () => {
    const snapshot = FIXTURE_SNAPSHOTS.kimiSessionWeekly;
    expect(snapshot.windows[0]).toMatchObject({
      id: "primary",
      kind: "weekly",
      usageKnown: true,
    });
    expect(snapshot.windows[1]).toMatchObject({
      id: "secondary",
      kind: "session",
      windowMinutes: 300,
      usageKnown: true,
    });
    expect(snapshot.capabilities.hasQuota).toBe(true);
    expect(snapshot.capabilities.hasBalance).toBe(false);
  });

  it("keeps an informational primary as unknown usage, never 100% known", () => {
    const snapshot = FIXTURE_SNAPSHOTS.antigravityInformational;
    const primary = snapshot.windows.find((window) => window.id === "primary");
    expect(primary?.isInformational).toBe(true);
    expect(primary?.usageKnown).toBe(false);
    expect(primary?.remainingPercent).toBe(100);
    expect(isRealQuotaWindow(primary!)).toBe(false);
    expect(snapshot.windows.filter(isRealQuotaWindow)).toHaveLength(4);
  });

  it("treats a balance carrier as balance, not a quota bar", () => {
    const snapshot = FIXTURE_SNAPSHOTS.balanceOnly;
    expect(snapshot.cost?.formattedUsed).toBe("¥69.21");
    expect(snapshot.capabilities.hasBalance).toBe(true);
    expect(snapshot.capabilities.hasQuota).toBe(false);
    expect(snapshot.windows.every((window) => !isRealQuotaWindow(window))).toBe(true);
  });

  it("lifts wayfinder telemetry without inventing a quota window", () => {
    const snapshot = FIXTURE_SNAPSHOTS.telemetryOnly;
    expect(snapshot.telemetry?.gatewayStatus).toBe("ok");
    expect(snapshot.telemetry?.modelCount).toBe(3);
    expect(snapshot.capabilities.hasTelemetry).toBe(true);
    expect(snapshot.capabilities.hasQuota).toBe(false);
    expect(snapshot.capabilities.snapshotShape).toBe("telemetry");
  });

  it("preserves usageKnown=false on extra windows", () => {
    const snapshot = FIXTURE_SNAPSHOTS.unknownUsage;
    const extra = snapshot.windows.find((window) => window.id === "reset-only");
    expect(extra?.usageKnown).toBe(false);
    expect(extra?.remainingPercent).toBe(100);
    expect(snapshot.capabilities.hasQuota).toBe(false);
  });

  it("marks 4+ real windows as overflow shape", () => {
    const snapshot = FIXTURE_SNAPSHOTS.overflowFourPlus;
    expect(snapshot.windows.filter(isRealQuotaWindow).length).toBeGreaterThanOrEqual(5);
    expect(snapshot.capabilities.snapshotShape).toBe("overflow");
    expect(snapshot.capabilities.hasExtraWindows).toBe(true);
  });

  it("classifies notConfigured and error from existing error text", () => {
    expect(FIXTURE_SNAPSHOTS.notConfigured.displayState).toBe("notConfigured");
    expect(FIXTURE_SNAPSHOTS.notConfigured.sourceHealth).toBe("notConfigured");
    expect(FIXTURE_SNAPSHOTS.error.displayState).toBe("error");
    expect(FIXTURE_SNAPSHOTS.error.sourceHealth).toBe("error");
    expect(fromBridge(FIXTURE_BRIDGES.stale, NOW).displayState).toBe("stale");
  });

  it("maps auth failures to authRequired without a provider-name switch", () => {
    const snapshot = fromBridge(
      {
        ...FIXTURE_BRIDGES.error,
        error: "authentication required",
      },
      NOW,
    );
    expect(snapshot.displayState).toBe("authRequired");
  });

  it("does not switch on provider display names when inferring capabilities", () => {
    const renamed = fromBridge(
      {
        ...FIXTURE_BRIDGES.antigravityInformational,
        providerId: "renamed-family",
        displayName: "Renamed Family",
      },
      NOW,
    );
    expect(renamed.capabilities.hasQuota).toBe(
      FIXTURE_SNAPSHOTS.antigravityInformational.capabilities.hasQuota,
    );
    expect(renamed.windows.filter(isRealQuotaWindow)).toHaveLength(4);
  });
});
