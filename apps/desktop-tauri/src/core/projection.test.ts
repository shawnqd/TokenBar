import { describe, expect, it } from "vitest";
import {
  fromBridge,
  projectSurface,
  type SurfaceProjection,
} from "./index";
import {
  FIXTURE_BRIDGES,
  FIXTURE_SNAPSHOTS,
  bridgeRateWindow,
  bridgeSnapshot,
} from "./fixtures";

function values(projection: SurfaceProjection): string[] {
  return projection.taskbarCells.map((cell) => cell.value);
}

describe("projectSurface", () => {
  it("sorts a Kimi-like session/time window above the weekly slot", () => {
    const projection = projectSurface(FIXTURE_SNAPSHOTS.kimiSessionWeekly);
    expect(projection.archetype).toBe("quota");
    expect(projection.primary).toMatchObject({
      id: "secondary",
      kind: "session",
      usageKnown: true,
      fillPercent: 40,
    });
    expect(projection.secondary).toMatchObject({
      id: "primary",
      kind: "weekly",
      fillPercent: 62,
    });
    expect(projection.layers.quota.map((window) => window.kind)).toEqual([
      "session",
      "weekly",
    ]);
  });

  it("still sorts session first after the display name is renamed", () => {
    const renamed = projectSurface(
      fromBridge({
        ...FIXTURE_BRIDGES.kimiSessionWeekly,
        providerId: "renamed-session",
        displayName: "Not The Original Name",
      }),
    );
    const original = projectSurface(FIXTURE_SNAPSHOTS.kimiSessionWeekly);
    expect(renamed.primary?.kind).toBe(original.primary?.kind);
    expect(renamed.primary?.windowMinutes ?? 300).toBe(300);
    expect(renamed.secondary?.kind).toBe("weekly");
  });

  it("drops an informational primary and does not paint it as a 100% bar", () => {
    const projection = projectSurface(FIXTURE_SNAPSHOTS.antigravityInformational);
    expect(projection.primary?.id).toBe("gemini-session");
    expect(projection.primary?.fillPercent).toBe(22);
    // Family-major order: the Gemini 5h+weekly pair stays together and the
    // Claude pair follows — the tray reads these as grouped pairs like the
    // official Antigravity Models screen, not as a global cycle sort.
    expect(projection.secondary?.id).toBe("gemini-weekly");
    expect(projection.extras.map((window) => window.id)).toEqual([
      "claude-session",
      "claude-weekly",
    ]);
    const all = [
      projection.primary,
      projection.secondary,
      ...projection.extras,
      ...projection.overflow,
    ];
    expect(all.some((window) => window?.id === "primary")).toBe(false);
    expect(all.some((window) => window?.fillPercent === 100)).toBe(false);
    expect(projection.layers.quota.every((window) => window.usageKnown)).toBe(true);
  });

  it("keeps informational extras from becoming a real 100% bar when renamed", () => {
    const renamed = projectSurface(
      fromBridge({
        ...FIXTURE_BRIDGES.antigravityInformational,
        providerId: "renamed-family",
        displayName: "Definitely Not A Special Case",
      }),
    );
    expect(renamed.primary?.label).toBe("Gemini 5h");
    expect(renamed.primary?.fillPercent).not.toBe(100);
    expect(renamed.layers.quota).toHaveLength(4);
  });

  it("projects balance-only as a balance layer with no quota bar", () => {
    const projection = projectSurface(FIXTURE_SNAPSHOTS.balanceOnly);
    expect(projection.archetype).toBe("balance");
    expect(projection.primary).toBeNull();
    expect(projection.layers.quota).toEqual([]);
    expect(projection.layers.balance?.amountText).toBe("¥69.21");
    expect(projection.taskbarCells[0]).toMatchObject({
      window: "balance",
      tag: "余",
      value: "¥69.21",
    });
    expect(values(projection).some((value) => value === "100%" || value === "0%")).toBe(
      false,
    );
  });

  it("projects telemetry-only without inventing a quota bar", () => {
    const projection = projectSurface(FIXTURE_SNAPSHOTS.telemetryOnly);
    expect(projection.archetype).toBe("balance");
    expect(projection.primary).toBeNull();
    expect(projection.layers.quota).toEqual([]);
    expect(projection.layers.telemetry?.headline).toBe("ok");
    expect(projection.layers.balance).toBeNull();
    expect(values(projection).includes("100%")).toBe(false);
  });

  it("keeps unknown usage empty instead of coercing 100%", () => {
    const remaining = projectSurface(FIXTURE_SNAPSHOTS.unknownUsage, {
      showAsUsed: false,
    });
    expect(remaining.layers.quota).toEqual([]);
    expect(remaining.primary).toBeNull();
    for (const cell of remaining.taskbarCells) {
      expect(cell.value).toBe("");
      expect(cell.value).not.toBe("100%");
      expect(cell.value).not.toBe("1.0");
    }
    expect(
      remaining.taskbarCells.some(
        (cell) => cell.state === "unknown" || cell.state === "unsupported",
      ),
    ).toBe(true);
  });

  it("sends windows beyond the first four into overflow", () => {
    const projection = projectSurface(FIXTURE_SNAPSHOTS.overflowFourPlus);
    expect(projection.primary).not.toBeNull();
    expect(projection.secondary).not.toBeNull();
    expect(projection.extras).toHaveLength(2);
    expect(projection.overflow.length).toBeGreaterThanOrEqual(2);
    expect(
      2 + projection.extras.length + projection.overflow.length,
    ).toBe(projection.layers.quota.length);
    expect(projection.primary?.kind).toBe("session");
    expect(projection.primary?.windowMinutes).toBe(240);
    expect(projection.primary?.label).toBe("4h");
  });

  it("surfaces notConfigured and error states without a fake quota", () => {
    const missing = projectSurface(FIXTURE_SNAPSHOTS.notConfigured);
    expect(missing.displayState).toBe("notConfigured");
    expect(missing.primary).toBeNull();

    const failed = projectSurface(FIXTURE_SNAPSHOTS.error);
    expect(failed.displayState).toBe("error");
    expect(failed.primary?.usedPercent).toBe(44);
    expect(failed.error).toBe("upstream 500");
  });

  it("pins a GLM Start Plan extra as an unnamed primary cell tagged 体", () => {
    const snapshot = fromBridge(
      bridgeSnapshot({
        providerId: "zai",
        displayName: "GLM",
        updatedAt: new Date().toISOString(),
        primary: bridgeRateWindow({
          usedPercent: 0,
          remainingPercent: 100,
          isInformational: true,
        }),
        extraRateWindows: [
          {
            id: "zai-zcode-0",
            title: "体验套餐 · GLM-5.3-Flash",
            usageKnown: true,
            window: bridgeRateWindow({
              usedPercent: 0,
              remainingPercent: 100,
              resetDescription: "剩余 300000000",
            }),
          },
        ],
      }),
    );
    const projection = projectSurface(snapshot, {
      taskbarEntries: [{ providerId: "zai", window: "primary" }],
    });
    expect(projection.taskbarCells[0]).toMatchObject({
      window: "primary",
      tag: "体",
      value: "0%",
    });
  });

  it("emits left-packed taskbar cells, not glyph+sentence text", () => {
    const projection = projectSurface(FIXTURE_SNAPSHOTS.kimiSessionWeekly, {
      showAsUsed: false,
      taskbarEntries: [
        { providerId: "kimi-like", window: "session" },
        { providerId: "kimi-like", window: "weekly" },
      ],
    });
    expect(projection.taskbarCells).toHaveLength(2);
    expect(projection.taskbarCells[0]).toMatchObject({
      tag: "5h",
      value: "60%",
      state: "ready",
    });
    expect(projection.taskbarCells[1]).toMatchObject({
      tag: "周",
      value: "38%",
    });
    for (const cell of projection.taskbarCells) {
      expect(cell).toHaveProperty("icon");
      expect(cell).toHaveProperty("tag");
      expect(cell).toHaveProperty("value");
      expect(cell).toHaveProperty("state");
      expect(cell.value.includes(" ")).toBe(false);
    }
  });

  it("keeps Gemini pair on top and Claude/GPT pair in extras with live titles containing slashes", () => {
    const liveModel = {
      providerId: "antigravity",
      displayName: "Antigravity",
      primary: {
        kind: "unknown" as const,
        usedPercent: 0,
        isInformational: true,
      },
      extraRateWindows: [
        {
          id: "antigravity-quota-summary-gemini-5-hour",
          title: "Gemini 5-hour",
          usageKnown: true,
          window: {
            kind: "session" as const,
            windowMinutes: 300,
            usedPercent: 23.6,
          },
        },
        {
          id: "antigravity-quota-summary-gemini-weekly",
          title: "Gemini weekly",
          usageKnown: true,
          window: {
            kind: "weekly" as const,
            windowMinutes: 10080,
            usedPercent: 20.6,
          },
        },
        {
          id: "antigravity-quota-summary-claude-gpt-5-hour",
          title: "Claude/GPT 5-hour",
          usageKnown: true,
          window: {
            kind: "session" as const,
            windowMinutes: 300,
            usedPercent: 0,
          },
        },
        {
          id: "antigravity-quota-summary-claude-gpt-weekly",
          title: "Claude/GPT weekly",
          usageKnown: true,
          window: {
            kind: "weekly" as const,
            windowMinutes: 10080,
            usedPercent: 33.5,
          },
        },
      ],
    };
    const projection = projectSurface(fromBridge(liveModel as any));
    expect(projection.primary?.label).toBe("Gemini 5-hour");
    expect(projection.secondary?.label).toBe("Gemini weekly");
    expect(projection.extras.map((w) => w.label)).toEqual([
      "Claude/GPT 5-hour",
      "Claude/GPT weekly",
    ]);
  });
});
