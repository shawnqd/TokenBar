import type {
  CostSnapshotBridge,
  ProviderUsageSnapshot,
  RateWindowSnapshot as BridgeRateWindow,
  WayfinderUsageSnapshot,
} from "../types/bridge";
import { fromBridge } from "./fromBridge";
import type { ProviderDisplayModel, ProviderSnapshot } from "./snapshot";

export function bridgeRateWindow(
  overrides: Partial<BridgeRateWindow> = {},
): BridgeRateWindow {
  return {
    usedPercent: 0,
    remainingPercent: 100,
    kind: null,
    windowMinutes: null,
    resetsAt: null,
    resetDescription: null,
    isExhausted: false,
    isInformational: false,
    reservePercent: null,
    reserveDescription: null,
    ...overrides,
  };
}

export function bridgeCost(
  overrides: Partial<CostSnapshotBridge> = {},
): CostSnapshotBridge {
  return {
    used: 69.21,
    limit: null,
    remaining: 69.21,
    currencyCode: "CNY",
    period: "prepaid",
    resetsAt: null,
    formattedUsed: "¥69.21",
    formattedLimit: null,
    ...overrides,
  };
}

export function bridgeTelemetry(
  overrides: Partial<WayfinderUsageSnapshot> = {},
): WayfinderUsageSnapshot {
  return {
    gatewayStatus: "ok",
    offline: false,
    dryRun: false,
    missingKeys: [],
    modelCount: 3,
    models: ["alpha", "beta", "gamma"],
    requests: 12,
    estimatedRequests: 12,
    tokens: 1000,
    realized: 1.5,
    baseline: 3,
    saved: 1.5,
    savedPercent: 50,
    periodDays: 7,
    unit: "usd",
    priced: true,
    routes: [],
    ...overrides,
  };
}

export function bridgeSnapshot(
  overrides: Partial<ProviderUsageSnapshot> = {},
): ProviderDisplayModel {
  return {
    providerId: "fixture",
    displayName: "Fixture",
    primary: bridgeRateWindow(),
    extraRateWindows: [],
    cost: null,
    planName: null,
    accountEmail: null,
    sourceLabel: "auto",
    updatedAt: "2026-08-16T12:00:00.000Z",
    error: null,
    pace: null,
    accountOrganization: null,
    trayStatusLabel: null,
    secondary: null,
    modelSpecific: null,
    tertiary: null,
    ...overrides,
  };
}

/** Data-layer primary is weekly; secondary is the 5h session. */
export const kimiSessionWeeklyBridge: ProviderDisplayModel = bridgeSnapshot({
  providerId: "kimi-like",
  displayName: "Kimi-like",
  primaryLabel: "weekly",
  primary: bridgeRateWindow({
    kind: "weekly",
    windowMinutes: 7 * 24 * 60,
    usedPercent: 62,
    remainingPercent: 38,
  }),
  secondaryLabel: "5h",
  secondary: bridgeRateWindow({
    kind: "session",
    windowMinutes: 300,
    usedPercent: 40,
    remainingPercent: 60,
  }),
});

/** Informational primary placeholder; real buckets live in extras. */
export const antigravityInformationalBridge: ProviderDisplayModel = bridgeSnapshot({
  providerId: "multi-family",
  displayName: "Multi-family",
  primary: bridgeRateWindow({
    isInformational: true,
    usedPercent: 0,
    remainingPercent: 100,
    resetDescription: "",
  }),
  extraRateWindows: [
    {
      id: "gemini-session",
      title: "Gemini 5h",
      usageKnown: true,
      window: bridgeRateWindow({
        kind: "session",
        windowMinutes: 300,
        usedPercent: 22,
        remainingPercent: 78,
      }),
    },
    {
      id: "gemini-weekly",
      title: "Gemini weekly",
      usageKnown: true,
      window: bridgeRateWindow({
        kind: "weekly",
        windowMinutes: 7 * 24 * 60,
        usedPercent: 5,
        remainingPercent: 95,
      }),
    },
    {
      id: "claude-session",
      title: "Claude 5h",
      usageKnown: true,
      window: bridgeRateWindow({
        kind: "session",
        windowMinutes: 300,
        usedPercent: 30,
        remainingPercent: 70,
      }),
    },
    {
      id: "claude-weekly",
      title: "Claude weekly",
      usageKnown: true,
      window: bridgeRateWindow({
        kind: "weekly",
        windowMinutes: 7 * 24 * 60,
        usedPercent: 12,
        remainingPercent: 88,
      }),
    },
  ],
});

export const balanceOnlyBridge: ProviderDisplayModel = bridgeSnapshot({
  providerId: "prepaid",
  displayName: "Prepaid",
  primary: bridgeRateWindow({
    usedPercent: 0,
    remainingPercent: 100,
    resetDescription: "¥69.21 (Paid: ¥69.21 / Granted: ¥0.00)",
  }),
  cost: bridgeCost(),
});

export const telemetryOnlyBridge: ProviderDisplayModel = bridgeSnapshot({
  providerId: "gateway",
  displayName: "Gateway",
  primary: bridgeRateWindow({
    isInformational: true,
    usedPercent: 0,
    remainingPercent: 100,
    resetDescription: "deploy ready",
  }),
  wayfinderUsage: bridgeTelemetry(),
});

export const unknownUsageBridge: ProviderDisplayModel = bridgeSnapshot({
  providerId: "unknown-usage",
  displayName: "Unknown usage",
  primary: bridgeRateWindow({
    isInformational: true,
    usedPercent: 0,
    remainingPercent: 100,
  }),
  extraRateWindows: [
    {
      id: "reset-only",
      title: "Reset only",
      usageKnown: false,
      window: bridgeRateWindow({
        kind: "weekly",
        windowMinutes: 7 * 24 * 60,
        usedPercent: 0,
        remainingPercent: 100,
        isInformational: true,
        resetDescription: "resets Friday",
      }),
    },
  ],
});

export const overflowFourPlusBridge: ProviderDisplayModel = bridgeSnapshot({
  providerId: "many-windows",
  displayName: "Many windows",
  primary: bridgeRateWindow({
    kind: "session",
    windowMinutes: 240,
    usedPercent: 10,
    remainingPercent: 90,
  }),
  secondary: bridgeRateWindow({
    kind: "session",
    windowMinutes: 300,
    usedPercent: 20,
    remainingPercent: 80,
  }),
  modelSpecific: bridgeRateWindow({
    kind: "weekly",
    windowMinutes: 7 * 24 * 60,
    usedPercent: 30,
    remainingPercent: 70,
  }),
  tertiary: bridgeRateWindow({
    kind: "monthly",
    windowMinutes: 30 * 24 * 60,
    usedPercent: 40,
    remainingPercent: 60,
  }),
  extraRateWindows: [
    {
      id: "pool-a",
      title: "Pool A",
      usageKnown: true,
      window: bridgeRateWindow({
        kind: "weekly",
        usedPercent: 50,
        remainingPercent: 50,
      }),
    },
    {
      id: "pool-b",
      title: "Pool B",
      usageKnown: true,
      window: bridgeRateWindow({
        kind: null,
        usedPercent: 60,
        remainingPercent: 40,
      }),
    },
  ],
});

export const notConfiguredBridge: ProviderDisplayModel = bridgeSnapshot({
  providerId: "unset",
  displayName: "Unset",
  error: "Provider not configured",
  primary: bridgeRateWindow({ isInformational: true }),
});

export const errorBridge: ProviderDisplayModel = bridgeSnapshot({
  providerId: "broken",
  displayName: "Broken",
  error: "upstream 500",
  primary: bridgeRateWindow({
    kind: "weekly",
    windowMinutes: 7 * 24 * 60,
    usedPercent: 44,
    remainingPercent: 56,
  }),
});

export const staleBridge: ProviderDisplayModel = bridgeSnapshot({
  providerId: "stale",
  displayName: "Stale",
  updatedAt: "2026-08-01T00:00:00.000Z",
  primary: bridgeRateWindow({
    kind: "weekly",
    windowMinutes: 7 * 24 * 60,
    usedPercent: 15,
    remainingPercent: 85,
  }),
});

export const FIXTURE_BRIDGES = {
  kimiSessionWeekly: kimiSessionWeeklyBridge,
  antigravityInformational: antigravityInformationalBridge,
  balanceOnly: balanceOnlyBridge,
  telemetryOnly: telemetryOnlyBridge,
  unknownUsage: unknownUsageBridge,
  overflowFourPlus: overflowFourPlusBridge,
  notConfigured: notConfiguredBridge,
  error: errorBridge,
  stale: staleBridge,
} as const;

export function snapshotFromFixture(
  model: ProviderDisplayModel,
  nowMs = Date.parse("2026-08-16T12:05:00.000Z"),
): ProviderSnapshot {
  return fromBridge(model, nowMs);
}

export const FIXTURE_SNAPSHOTS = {
  kimiSessionWeekly: snapshotFromFixture(kimiSessionWeeklyBridge),
  antigravityInformational: snapshotFromFixture(antigravityInformationalBridge),
  balanceOnly: snapshotFromFixture(balanceOnlyBridge),
  telemetryOnly: snapshotFromFixture(telemetryOnlyBridge),
  unknownUsage: snapshotFromFixture(unknownUsageBridge),
  overflowFourPlus: snapshotFromFixture(overflowFourPlusBridge),
  notConfigured: snapshotFromFixture(notConfiguredBridge),
  error: snapshotFromFixture(errorBridge),
  stale: snapshotFromFixture(staleBridge),
} as const;
