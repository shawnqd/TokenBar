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

// ── CORE-01 minimal provider-shape matrix ─────────────────────────────────
// Each bridge below must survive fromBridge() → projectSurface() and pin one
// real provider shape. Perceptible rules enforced in fixtures.matrix.test.ts:
// missing/unknown fractions never turn into a known 100% bar, informational
// windows never draw a progress bar, and a clientModelConfigs[] family+cycle
// combo appears exactly once.

/** Codex: prepaid credits as cost + one real weekly quota window. */
export const codexCreditsBridge: ProviderDisplayModel = bridgeSnapshot({
  providerId: "codex",
  displayName: "Codex",
  primary: bridgeRateWindow({
    kind: "weekly",
    windowMinutes: 7 * 24 * 60,
    usedPercent: 41,
    remainingPercent: 59,
    resetsAt: "2026-08-23T17:00:00.000Z",
    resetDescription: "resets weekly",
  }),
  cost: bridgeCost({
    used: 12.4,
    limit: 100,
    remaining: 87.6,
    currencyCode: "USD",
    period: "prepaid",
    formattedUsed: "$12.40",
    formattedLimit: "$100.00",
  }),
});

/** Claude: weekly + monthly dual cycle; weekly sorts above monthly. */
export const claudeWeeklyMonthlyBridge: ProviderDisplayModel = bridgeSnapshot({
  providerId: "claude",
  displayName: "Claude",
  primary: bridgeRateWindow({
    kind: "weekly",
    windowMinutes: 7 * 24 * 60,
    usedPercent: 61,
    remainingPercent: 39,
    resetsAt: "2026-08-23T17:00:00.000Z",
    resetDescription: "resets weekly",
  }),
  secondary: bridgeRateWindow({
    kind: "monthly",
    windowMinutes: 30 * 24 * 60,
    usedPercent: 34,
    remainingPercent: 66,
    resetsAt: "2026-09-16T17:00:00.000Z",
    resetDescription: "resets monthly",
  }),
});

/** OpenCode-style three cycles: session + weekly + monthly. */
export const opencodeMonthlyBridge: ProviderDisplayModel = bridgeSnapshot({
  providerId: "opencode",
  displayName: "OpenCode",
  primary: bridgeRateWindow({
    kind: "weekly",
    windowMinutes: 7 * 24 * 60,
    usedPercent: 72,
    remainingPercent: 28,
  }),
  secondary: bridgeRateWindow({
    kind: "session",
    windowMinutes: 240,
    usedPercent: 55,
    remainingPercent: 45,
  }),
  tertiary: bridgeRateWindow({
    kind: "monthly",
    windowMinutes: 30 * 24 * 60,
    usedPercent: 35,
    remainingPercent: 65,
  }),
});

/** Ark Agent Plan: 5h + daily + weekly + monthly — 4 real windows overflow. */
export const arkMultiWindowBridge: ProviderDisplayModel = bridgeSnapshot({
  providerId: "ark",
  displayName: "Ark Agent Plan",
  primary: bridgeRateWindow({
    kind: "session",
    windowMinutes: 300,
    usedPercent: 40,
    remainingPercent: 60,
  }),
  secondary: bridgeRateWindow({
    kind: "daily",
    windowMinutes: 24 * 60,
    usedPercent: 25,
    remainingPercent: 75,
  }),
  modelSpecific: bridgeRateWindow({
    kind: "weekly",
    windowMinutes: 7 * 24 * 60,
    usedPercent: 50,
    remainingPercent: 50,
  }),
  tertiary: bridgeRateWindow({
    kind: "monthly",
    windowMinutes: 30 * 24 * 60,
    usedPercent: 10,
    remainingPercent: 90,
  }),
});

/**
 * DeepSeek: balance-only. The primary is the synthetic 0/100 placeholder whose
 * resetDescription carries the prepaid balance; fromBridge lifts it to cost
 * and no window may render as a quota bar.
 */
export const deepseekBalanceBridge: ProviderDisplayModel = bridgeSnapshot({
  providerId: "deepseek",
  displayName: "DeepSeek",
  planName: "CNY balance: ¥38.88",
  primary: bridgeRateWindow({
    isInformational: true,
    usedPercent: 0,
    remainingPercent: 100,
    resetDescription: "¥38.88 (Paid: ¥38.88 / Granted: ¥0.00)",
  }),
});

/**
 * MiMo: balance-only. No token plan → primary is the informational marker;
 * the real balance lives in secondary.resetDescription and becomes cost.
 */
export const mimoBalanceBridge: ProviderDisplayModel = bridgeSnapshot({
  providerId: "mimo",
  displayName: "MiMo",
  primaryLabel: "Token Plan",
  primary: bridgeRateWindow({
    isInformational: true,
    usedPercent: 0,
    remainingPercent: 100,
    resetDescription: "No active MiMo Token Plan",
  }),
  secondaryLabel: "Balance",
  secondary: bridgeRateWindow({
    usedPercent: 0,
    remainingPercent: 100,
    resetDescription: "12.50 CNY balance (Paid: 8.25 CNY / Granted: 4.25 CNY)",
  }),
});

/** z.ai / GLM BigModel CN: quota is absent in this fixture and the wallet is
 * published as an informational named row with the upstream "available"
 * suffix. This is the production shape that the tray balance adapter must
 * promote into the shared balance layer. */
export const zaiChinaBalanceBridge: ProviderDisplayModel = bridgeSnapshot({
  providerId: "zai",
  displayName: "GLM (BigModel CN)",
  primaryLabel: "Usage",
  primary: bridgeRateWindow({
    isInformational: true,
  }),
  extraRateWindows: [
    {
      id: "zai-account-balance",
      title: "Account balance",
      usageKnown: true,
      window: bridgeRateWindow({
        isInformational: true,
        resetDescription: "¥12.50 available",
      }),
    },
  ],
});

/**
 * Sub2API is polymorphic: one parse yields subscription cycles, a key quota,
 * a wallet balance or an unknown payload. No single snapshot shows all four,
 * so this fixture is the combination matrix: cyclic quota windows (the
 * subscription/key-quota reading), a wallet cost, and an informational
 * unknown stream that must never render as a quota bar.
 */
export const sub2apiFourShapesBridge: ProviderDisplayModel = bridgeSnapshot({
  providerId: "sub2api",
  displayName: "Sub2API",
  primary: bridgeRateWindow({
    kind: "daily",
    windowMinutes: 24 * 60,
    usedPercent: 72,
    remainingPercent: 28,
  }),
  secondary: bridgeRateWindow({
    kind: "weekly",
    windowMinutes: 7 * 24 * 60,
    usedPercent: 58,
    remainingPercent: 42,
  }),
  tertiary: bridgeRateWindow({
    kind: "monthly",
    windowMinutes: 30 * 24 * 60,
    usedPercent: 45,
    remainingPercent: 55,
  }),
  extraRateWindows: [
    {
      id: "key-1",
      title: "Key 1",
      usageKnown: true,
      window: bridgeRateWindow({
        kind: "session",
        windowMinutes: 300,
        usedPercent: 30,
        remainingPercent: 70,
      }),
    },
    {
      id: "unknown-stream",
      title: "Unknown stream",
      usageKnown: false,
      window: bridgeRateWindow({
        kind: "weekly",
        windowMinutes: 7 * 24 * 60,
        isInformational: true,
        usedPercent: 0,
        remainingPercent: 100,
        resetDescription: "no remaining fraction",
      }),
    },
  ],
  cost: bridgeCost({
    used: 48.5,
    limit: null,
    remaining: 48.5,
    currencyCode: "USD",
    period: "",
    formattedUsed: "$48.50",
    formattedLimit: null,
  }),
});

/** Copilot: distinct product slots (Premium/Chat/Completions) as windows. */
export const copilotMultiWindowBridge: ProviderDisplayModel = bridgeSnapshot({
  providerId: "copilot",
  displayName: "Copilot",
  primaryLabel: "Premium",
  primary: bridgeRateWindow({
    kind: "weekly",
    windowMinutes: 7 * 24 * 60,
    usedPercent: 55,
    remainingPercent: 45,
  }),
  secondaryLabel: "Chat",
  secondary: bridgeRateWindow({
    kind: "session",
    windowMinutes: 300,
    usedPercent: 42,
    remainingPercent: 58,
  }),
  extraRateWindows: [
    {
      id: "completions",
      title: "Completions",
      usageKnown: true,
      window: bridgeRateWindow({
        kind: "monthly",
        windowMinutes: 30 * 24 * 60,
        usedPercent: 28,
        remainingPercent: 72,
      }),
    },
  ],
});

/**
 * Antigravity: informational summary primary + multi-family windows exactly as
 * the Rust parser aggregates one clientModelConfigs[]: each family+cycle combo
 * appears once (Gemini/GPT/Claude windows are pooled across the models in a
 * config array, never exploded into per-model cards). Titles carry the family
 * plus a cycle word — a bare model name is not a quota card.
 */
export const antigravityMultiFamilyBridge: ProviderDisplayModel = bridgeSnapshot({
  providerId: "antigravity",
  displayName: "Antigravity",
  primary: bridgeRateWindow({
    isInformational: true,
    usedPercent: 0,
    remainingPercent: 100,
    resetDescription: "",
  }),
  extraRateWindows: [
    {
      id: "gemini-5h",
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
      id: "claude-5h",
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
    {
      id: "gpt-weekly",
      title: "GPT weekly",
      usageKnown: true,
      window: bridgeRateWindow({
        kind: "weekly",
        windowMinutes: 7 * 24 * 60,
        usedPercent: 18,
        remainingPercent: 82,
      }),
    },
  ],
});

/**
 * Unknown state: a fresh snapshot whose primary is only informational and no
 * quota/balance/telemetry is recognizable — derived displayState "unknown",
 * never a fake 0%/100% quota.
 */
export const unknownBridge: ProviderDisplayModel = bridgeSnapshot({
  providerId: "unknown-state",
  displayName: "Unknown",
  primary: bridgeRateWindow({
    isInformational: true,
    usedPercent: 0,
    remainingPercent: 100,
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
  codexCredits: codexCreditsBridge,
  claudeWeeklyMonthly: claudeWeeklyMonthlyBridge,
  opencodeMonthly: opencodeMonthlyBridge,
  arkMultiWindow: arkMultiWindowBridge,
  deepseekBalance: deepseekBalanceBridge,
  mimoBalance: mimoBalanceBridge,
  zaiChinaBalance: zaiChinaBalanceBridge,
  sub2apiFourShapes: sub2apiFourShapesBridge,
  copilotMultiWindow: copilotMultiWindowBridge,
  antigravityMultiFamily: antigravityMultiFamilyBridge,
  unknown: unknownBridge,
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
  codexCredits: snapshotFromFixture(codexCreditsBridge),
  claudeWeeklyMonthly: snapshotFromFixture(claudeWeeklyMonthlyBridge),
  opencodeMonthly: snapshotFromFixture(opencodeMonthlyBridge),
  arkMultiWindow: snapshotFromFixture(arkMultiWindowBridge),
  deepseekBalance: snapshotFromFixture(deepseekBalanceBridge),
  mimoBalance: snapshotFromFixture(mimoBalanceBridge),
  zaiChinaBalance: snapshotFromFixture(zaiChinaBalanceBridge),
  sub2apiFourShapes: snapshotFromFixture(sub2apiFourShapesBridge),
  copilotMultiWindow: snapshotFromFixture(copilotMultiWindowBridge),
  antigravityMultiFamily: snapshotFromFixture(antigravityMultiFamilyBridge),
  unknown: snapshotFromFixture(unknownBridge),
} as const;
