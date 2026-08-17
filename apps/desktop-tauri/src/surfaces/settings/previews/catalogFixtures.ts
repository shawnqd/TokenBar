import { TEST_PROVIDER_CATALOG } from "../../../test/providerCatalog";
import type {
  ProviderUsageSnapshot,
  QuotaCycleKind,
  RateWindowSnapshot,
} from "../../../types/bridge";

function rateWindow(
  usedPercent: number,
  opts: {
    kind?: QuotaCycleKind | null;
    windowMinutes?: number | null;
    resetDescription?: string | null;
    resetsAt?: string | null;
  } = {},
): RateWindowSnapshot {
  return {
    usedPercent,
    remainingPercent: 100 - usedPercent,
    kind: opts.kind ?? null,
    windowMinutes: opts.windowMinutes ?? null,
    resetsAt: opts.resetsAt ?? null,
    resetDescription: opts.resetDescription ?? null,
    isExhausted: false,
    reservePercent: null,
    reserveDescription: null,
  };
}

function baseSnapshot(
  providerId: string,
  displayName: string,
  rest: Partial<ProviderUsageSnapshot> = {},
): ProviderUsageSnapshot {
  return {
    providerId,
    displayName,
    primary: rateWindow(42, { kind: "session", windowMinutes: 5 * 60 }),
    primaryLabel: "Session",
    secondary: rateWindow(70, { kind: "weekly", windowMinutes: 7 * 24 * 60 }),
    secondaryLabel: "Weekly",
    modelSpecific: null,
    tertiary: null,
    extraRateWindows: [],
    cost: null,
    planName: null,
    accountEmail: null,
    sourceLabel: "auto",
    updatedAt: "2026-05-24T00:00:00Z",
    error: null,
    pace: null,
    accountOrganization: null,
    trayStatusLabel: null,
    fetchDurationMs: null,
    ...rest,
  };
}

/**
 * Catalog-backed preview snapshots. Numbers come from the same Claude / Codex
 * shapes already used in MenuCard tests — not invented 100% bars.
 */
export function catalogPreviewSnapshots(): ProviderUsageSnapshot[] {
  const byId = new Map(TEST_PROVIDER_CATALOG);
  const claudeName = byId.get("claude") ?? "Claude";
  const deepseekName = byId.get("deepseek") ?? "DeepSeek";
  const cursorName = byId.get("cursor") ?? "Cursor";
  const codexName = byId.get("codex") ?? "Codex";

  return [
    baseSnapshot("claude", claudeName, {
      primary: rateWindow(78, {
        kind: "session",
        windowMinutes: 5 * 60,
        resetDescription: "3h 57m",
      }),
      secondary: rateWindow(52, {
        kind: "weekly",
        windowMinutes: 7 * 24 * 60,
        resetsAt: "2026-05-30T00:00:00Z",
      }),
      pace: {
        stage: "ahead",
        deltaPercent: 12.3,
        willLastToReset: true,
        etaSeconds: null,
        expectedUsedPercent: 40,
        actualUsedPercent: 52.3,
        speedMultiplierToReset: 1.5,
      },
    }),
    baseSnapshot("codex", codexName, {
      primary: rateWindow(62, {
        kind: "weekly",
        windowMinutes: 7 * 24 * 60,
      }),
      secondary: undefined,
      secondaryLabel: undefined,
    }),
    baseSnapshot("cursor", cursorName, {
      primary: rateWindow(85, {
        kind: "monthly",
        windowMinutes: 30 * 24 * 60,
      }),
      secondary: undefined,
      secondaryLabel: undefined,
    }),
    baseSnapshot("deepseek", deepseekName, {
      primary: rateWindow(0, {
        resetDescription: "¥69.21 (Paid: ¥69.21 / Granted: ¥0.00)",
      }),
      primaryLabel: "Balance",
      secondary: undefined,
      secondaryLabel: undefined,
    }),
  ];
}

export function pickPreviewSnapshot(
  live: ProviderUsageSnapshot[],
  preferredIds: string[] = ["claude", "codex", "cursor"],
): ProviderUsageSnapshot {
  for (const id of preferredIds) {
    const hit = live.find((row) => row.providerId === id);
    if (hit) return hit;
  }
  if (live[0]) return live[0];
  const fixtures = catalogPreviewSnapshots();
  return fixtures[0];
}

export function previewProviderList(
  live: ProviderUsageSnapshot[],
): ProviderUsageSnapshot[] {
  return live.length > 0 ? live : catalogPreviewSnapshots();
}
