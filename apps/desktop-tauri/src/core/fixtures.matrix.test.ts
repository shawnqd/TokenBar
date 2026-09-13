import { describe, expect, it } from "vitest";
import {
  fromBridge,
  isRealQuotaWindow,
  projectSurface,
  quotaFillPercent,
  type ProviderDisplayModel,
  type ProviderSnapshot,
} from "./index";
import {
  FIXTURE_BRIDGES,
  FIXTURE_SNAPSHOTS,
} from "./fixtures";

const NOW = Date.parse("2026-08-16T12:05:00.000Z");

function snapshotOf(bridge: ProviderDisplayModel): ProviderSnapshot {
  return fromBridge(bridge, NOW);
}

/** The CORE-01 minimal matrix added on top of the original nine fixtures. */
const MATRIX: ReadonlyArray<readonly [name: string, bridge: ProviderDisplayModel]> = [
  ["codexCredits", FIXTURE_BRIDGES.codexCredits],
  ["claudeWeeklyMonthly", FIXTURE_BRIDGES.claudeWeeklyMonthly],
  ["opencodeMonthly", FIXTURE_BRIDGES.opencodeMonthly],
  ["arkMultiWindow", FIXTURE_BRIDGES.arkMultiWindow],
  ["deepseekBalance", FIXTURE_BRIDGES.deepseekBalance],
  ["mimoBalance", FIXTURE_BRIDGES.mimoBalance],
  ["zaiChinaBalance", FIXTURE_BRIDGES.zaiChinaBalance],
  ["sub2apiFourShapes", FIXTURE_BRIDGES.sub2apiFourShapes],
  ["copilotMultiWindow", FIXTURE_BRIDGES.copilotMultiWindow],
  ["antigravityMultiFamily", FIXTURE_BRIDGES.antigravityMultiFamily],
  ["unknown", FIXTURE_BRIDGES.unknown],
];

describe("CORE-01 fixture matrix: every bridge is projection-consumable", () => {
  it("converts and projects every matrix fixture without throwing", () => {
    for (const [name, bridge] of MATRIX) {
      let snapshot: ProviderSnapshot;
      expect(() => {
        snapshot = snapshotOf(bridge);
      }, `${name}: fromBridge threw`).not.toThrow();
      expect(() => {
        projectSurface(snapshot!);
      }, `${name}: projectSurface threw`).not.toThrow();
      const projection = projectSurface(snapshot!);
      expect(projection.providerId, name).toBe(snapshot!.providerId);
      expect(projection.displayName, name).toBeTruthy();
    }
  });

  it("never draws an informational or unknown window as a progress bar", () => {
    for (const [name, bridge] of MATRIX) {
      const snapshot = snapshotOf(bridge);
      for (const window of snapshot.windows) {
        if (window.isInformational || !window.usageKnown) {
          expect(
            quotaFillPercent(window, true),
            `${name}:${window.id} must not fill`,
          ).toBeNull();
          expect(
            quotaFillPercent(window, false),
            `${name}:${window.id} must not fill (remaining mode)`,
          ).toBeNull();
        }
      }
    }
  });

  it("never turns a missing/unknown remaining fraction into a known 100% quota", () => {
    for (const [name, bridge] of MATRIX) {
      const snapshot = snapshotOf(bridge);
      for (const window of snapshot.windows) {
        if (window.remainingPercent === 100) {
          expect(
            isRealQuotaWindow(window),
            `${name}:${window.id} reports 100% yet is informational/unknown`,
          ).toBe(false);
          expect(quotaFillPercent(window), `${name}:${window.id}`).toBeNull();
        }
      }
    }
  });

  it("pre-built FIXTURE_SNAPSHOTS entries stay consumable", () => {
    const snapshots = FIXTURE_SNAPSHOTS as Record<string, ProviderSnapshot>;
    for (const [name] of MATRIX) {
      const snapshot = snapshots[name];
      expect(snapshot, `${name} missing from FIXTURE_SNAPSHOTS`).toBeTruthy();
      expect(() => projectSurface(snapshot!), `${name}`).not.toThrow();
    }
  });
});

describe("CORE-01 shape coverage", () => {
  it("codexCredits: prepaid credits + weekly quota stays a hybrid", () => {
    const snapshot = snapshotOf(FIXTURE_BRIDGES.codexCredits);
    expect(snapshot.capabilities.hasQuota).toBe(true);
    expect(snapshot.capabilities.hasBalance).toBe(true);
    expect(snapshot.capabilities.snapshotShape).toBe("mixed");
    expect(snapshot.cost?.period).toBe("prepaid");
    expect(snapshot.cost?.formattedUsed).toBe("$12.40");
    const projection = projectSurface(snapshot);
    expect(projection.archetype).toBe("hybrid");
    expect(projection.primary?.kind).toBe("weekly");
    expect(projection.primary?.usageKnown).toBe(true);
    expect(projection.primary?.fillPercent).toBe(41);
    expect(projection.layers.balance?.amountText).toBe("$12.40");
  });

  it("claudeWeeklyMonthly: weekly sorts above monthly", () => {
    const snapshot = snapshotOf(FIXTURE_BRIDGES.claudeWeeklyMonthly);
    expect(snapshot.windows.map((window) => window.kind)).toEqual([
      "weekly",
      "monthly",
    ]);
    const projection = projectSurface(snapshot);
    expect(projection.layers.quota.map((window) => window.kind)).toEqual([
      "weekly",
      "monthly",
    ]);
    expect(projection.primary?.kind).toBe("weekly");
    expect(projection.secondary?.kind).toBe("monthly");
  });

  it("opencodeMonthly: session + weekly + monthly three-cycle", () => {
    const snapshot = snapshotOf(FIXTURE_BRIDGES.opencodeMonthly);
    expect(snapshot.capabilities.hasQuota).toBe(true);
    expect(snapshot.capabilities.hasBalance).toBe(false);
    expect(snapshot.windows.filter(isRealQuotaWindow)).toHaveLength(3);
    const projection = projectSurface(snapshot);
    expect(projection.layers.quota.map((window) => window.kind)).toEqual([
      "session",
      "weekly",
      "monthly",
    ]);
    expect(projection.primary?.kind).toBe("session");
    expect(projection.primary?.windowMinutes).toBe(240);
    expect(projection.overflow).toHaveLength(0);
  });

  it("arkMultiWindow: 5h + daily + weekly + monthly overflows at four windows", () => {
    const snapshot = snapshotOf(FIXTURE_BRIDGES.arkMultiWindow);
    expect(snapshot.windows.filter(isRealQuotaWindow)).toHaveLength(4);
    expect(snapshot.capabilities.snapshotShape).toBe("overflow");
    const projection = projectSurface(snapshot);
    expect(projection.layers.quota.map((window) => window.kind)).toEqual([
      "session",
      "daily",
      "weekly",
      "monthly",
    ]);
    expect(projection.primary?.kind).toBe("session");
    expect(projection.primary?.windowMinutes).toBe(300);
    expect(projection.extras).toHaveLength(2);
    expect(projection.overflow).toHaveLength(0);
  });

  it("deepseekBalance: balance carrier becomes cost, no quota bar", () => {
    const snapshot = snapshotOf(FIXTURE_BRIDGES.deepseekBalance);
    expect(snapshot.cost?.formattedUsed).toBe("¥38.88");
    expect(snapshot.capabilities.hasBalance).toBe(true);
    expect(snapshot.capabilities.hasQuota).toBe(false);
    expect(snapshot.windows.filter(isRealQuotaWindow)).toEqual([]);
    const projection = projectSurface(snapshot);
    expect(projection.archetype).toBe("balance");
    expect(projection.primary).toBeNull();
    expect(projection.layers.quota).toEqual([]);
    expect(projection.layers.balance?.amountText).toBe("¥38.88");
  });

  it("mimoBalance: no-plan marker stays informational, secondary is balance", () => {
    const snapshot = snapshotOf(FIXTURE_BRIDGES.mimoBalance);
    expect(
      snapshot.windows.find((window) => window.id === "primary")?.displayKind,
    ).toBe("informational");
    expect(
      snapshot.windows.find((window) => window.id === "secondary")?.displayKind,
    ).toBe("balance");
    expect(snapshot.cost?.formattedUsed).toBe("¥12.50");
    expect(snapshot.capabilities.hasBalance).toBe(true);
    expect(snapshot.capabilities.hasQuota).toBe(false);
    expect(snapshot.windows.filter(isRealQuotaWindow)).toEqual([]);
    const projection = projectSurface(snapshot);
    expect(projection.layers.quota).toEqual([]);
    expect(projection.layers.balance?.amountText).toBe("¥12.50");
  });

  it("zaiChinaBalance: available wallet row becomes the shared balance layer", () => {
    const snapshot = snapshotOf(FIXTURE_BRIDGES.zaiChinaBalance);
    expect(snapshot.cost?.formattedUsed).toBe("¥12.50");
    expect(snapshot.capabilities.hasBalance).toBe(true);
    expect(snapshot.capabilities.hasQuota).toBe(false);
    expect(snapshot.windows.some((window) => window.id === "zai-account-balance")).toBe(true);
    expect(snapshot.windows.filter(isRealQuotaWindow)).toEqual([]);
    const projection = projectSurface(snapshot);
    expect(projection.archetype).toBe("balance");
    expect(projection.layers.balance?.amountText).toBe("¥12.50");
    expect(projection.taskbarCells[0]).toMatchObject({
      window: "balance",
      value: "¥12.50",
    });
  });

  it("sub2apiFourShapes: cycles + wallet balance + unknown stream", () => {
    const snapshot = snapshotOf(FIXTURE_BRIDGES.sub2apiFourShapes);
    expect(snapshot.capabilities.snapshotShape).toBe("overflow");
    expect(snapshot.capabilities.hasBalance).toBe(true);
    expect(snapshot.cost?.formattedUsed).toBe("$48.50");
    const real = snapshot.windows.filter(isRealQuotaWindow);
    expect(real.map((window) => window.kind)).toEqual(
      expect.arrayContaining(["session", "daily", "weekly", "monthly"]),
    );
    const unknown = snapshot.windows.find((window) => window.id === "unknown-stream");
    expect(unknown?.usageKnown).toBe(false);
    expect(quotaFillPercent(unknown!)).toBeNull();
    const projection = projectSurface(snapshot);
    expect(projection.archetype).toBe("hybrid");
    expect(projection.layers.quota).toHaveLength(4);
    expect(projection.layers.balance?.amountText).toBe("$48.50");
    expect(projection.overflow).toHaveLength(0);
  });

  it("copilotMultiWindow: product slots (Premium/Chat/Completions) as windows", () => {
    const snapshot = snapshotOf(FIXTURE_BRIDGES.copilotMultiWindow);
    expect(snapshot.capabilities.hasQuota).toBe(true);
    expect(snapshot.windows.filter(isRealQuotaWindow)).toHaveLength(3);
    const projection = projectSurface(snapshot);
    expect(projection.layers.quota.map((window) => window.kind)).toEqual([
      "session",
      "weekly",
      "monthly",
    ]);
    expect(projection.layers.quota.map((window) => window.label)).toEqual([
      "Chat",
      "Premium",
      "Completions",
    ]);
  });

  it("antigravityMultiFamily: one window per family+cycle, informational primary never drawn", () => {
    const snapshot = snapshotOf(FIXTURE_BRIDGES.antigravityMultiFamily);
    expect(snapshot.capabilities.hasQuota).toBe(true);
    expect(snapshot.capabilities.snapshotShape).toBe("overflow");
    const real = snapshot.windows.filter(isRealQuotaWindow);
    expect(real).toHaveLength(5);

    // The same clientModelConfigs[] must never explode into duplicate display
    // windows: each family+cycle combo appears exactly once.
    const keys = real.map(
      (window) => `${window.groupId ?? window.label}|${window.kind ?? ""}`,
    );
    expect(new Set(keys).size).toBe(keys.length);

    // A model name is not a quota card: window titles carry the family plus a
    // cycle word, never a bare model id.
    for (const window of real) {
      expect(
        window.label,
        `window label "${window.label}" must carry a cycle word (5h/weekly/monthly), not a bare model name`,
      ).toMatch(/(5h|weekly|monthly|daily|hour|日|周|月)/i);
    }

    const primary = snapshot.windows.find((window) => window.id === "primary");
    expect(primary?.isInformational).toBe(true);
    expect(primary?.usageKnown).toBe(false);
    expect(isRealQuotaWindow(primary!)).toBe(false);
    expect(quotaFillPercent(primary!)).toBeNull();

    const projection = projectSurface(snapshot);
    expect(projection.primary?.id).toBe("gemini-5h");
    const visible = [
      projection.primary,
      projection.secondary,
      ...projection.extras,
      ...projection.overflow,
    ];
    expect(visible.some((window) => window?.id === "primary")).toBe(false);
    expect(visible.some((window) => window?.fillPercent === 100)).toBe(false);
    expect(projection.layers.quota).toHaveLength(5);
  });

  it("unknown: fresh-but-empty snapshot is the unknown state, never 100%", () => {
    const snapshot = snapshotOf(FIXTURE_BRIDGES.unknown);
    expect(snapshot.displayState).toBe("unknown");
    expect(snapshot.sourceHealth).toBe("unknown");
    expect(snapshot.capabilities.hasQuota).toBe(false);
    expect(snapshot.windows.filter(isRealQuotaWindow)).toEqual([]);
    const projection = projectSurface(snapshot);
    expect(projection.primary).toBeNull();
    expect(projection.layers.quota).toEqual([]);
    for (const cell of projection.taskbarCells) {
      expect(cell.value).not.toBe("100%");
    }
  });
});
