import { describe, expect, it, vi } from "vitest";
import { createEnrichmentScheduler, type EnrichmentKind } from "./enrichmentScheduler";
import type { ProviderCapability } from "./snapshot";
import type { UsageStoreKey } from "./usageStore";

function cap(overrides: Partial<ProviderCapability> = {}): ProviderCapability {
  return {
    hasQuota: true,
    hasBalance: false,
    hasTelemetry: false,
    hasExtraWindows: false,
    supportsCharts: false,
    supportsLocalCost: false,
    supportsOutputSpeed: false,
    supportsProviderDashboard: false,
    supportsStatusPage: false,
    supportsLogin: false,
    snapshotShape: "standard",
    ...overrides,
  };
}

describe("EnrichmentScheduler", () => {
  it("gates by capability: no chart capability skips chart enrichment", async () => {
    const runner = vi.fn(async () => {});
    const capabilities = vi.fn(() => ({
      "no-chart": cap({ supportsCharts: false }),
      "has-chart": cap({ supportsCharts: true, hasQuota: true }),
    }));
    const sched = createEnrichmentScheduler({ capabilities, ttlMs: {}, runner });
    const keyNoChart: UsageStoreKey = { providerId: "no-chart", accountKey: "default", sourceKey: "default" };
    const keyHasChart: UsageStoreKey = { providerId: "has-chart", accountKey: "default", sourceKey: "default" };
    const r1 = await sched.trigger("chart", keyNoChart);
    const r2 = await sched.trigger("chart", keyHasChart);
    expect(r1).toBe(false);
    expect(r2).toBe(true);
    expect(runner).toHaveBeenCalledTimes(1);
    expect(runner).toHaveBeenCalledWith("chart", keyHasChart);
  });

  it("caches by TTL and not re-run before expiry", async () => {
    let now = 1000;
    const nowFn = () => now;
    const runner = vi.fn(async () => {});
    const capabilities = () => ({
      p1: cap({ supportsCharts: true, hasQuota: true }),
    });
    const sched = createEnrichmentScheduler({
      capabilities,
      ttlMs: { chart: 1000 } as Record<EnrichmentKind, number>,
      runner,
      now: nowFn,
    });
    const key: UsageStoreKey = { providerId: "p1", accountKey: "default", sourceKey: "default" };
    expect(await sched.trigger("chart", key)).toBe(true);
    expect(runner).toHaveBeenCalledTimes(1);
    // within TTL, second trigger should be skipped
    expect(await sched.trigger("chart", key)).toBe(false);
    expect(runner).toHaveBeenCalledTimes(1);
    now += 1500;
    expect(await sched.trigger("chart", key)).toBe(true);
    expect(runner).toHaveBeenCalledTimes(2);
    // getLastRun reflects last successful run
    expect(sched.getLastRun("chart", key)).toBe(now);
  });

  it("mode gating: chart only in detailed/full, not in minimal/compact", async () => {
    const runner = vi.fn(async () => {});
    const capabilities = () => ({
      p1: cap({ supportsCharts: true, hasQuota: true, supportsLocalCost: true, supportsOutputSpeed: true, hasBalance: true }),
    });
    const sched = createEnrichmentScheduler({ capabilities, ttlMs: {}, runner });
    const key: UsageStoreKey = { providerId: "p1", accountKey: "default", sourceKey: "default" };

    await sched.tick("minimal", [key]);
    // minimal should not call chart
    const hasChartMinimal = (runner.mock.calls as unknown as Array<[EnrichmentKind, UsageStoreKey]>).some((c) => c[0] === "chart");
    expect(hasChartMinimal).toBe(false);
    // but may have called outputSpeed / credits etc.
    const calledKindsMinimal = (runner.mock.calls as unknown as Array<[EnrichmentKind, UsageStoreKey]>).map((c) => c[0]);
    expect(calledKindsMinimal).not.toContain("chart");
    expect(calledKindsMinimal).toContain("outputSpeed");

    runner.mockClear();
    await sched.tick("compact", [key]);
    const hasChartCompact = (runner.mock.calls as unknown as Array<[EnrichmentKind, UsageStoreKey]>).some((c) => c[0] === "chart");
    expect(hasChartCompact).toBe(false);

    runner.mockClear();
    await sched.tick("detailed", [key]);
    expect(runner).toHaveBeenCalledWith("chart", key);
  });

  it("manual trigger bypasses TTL", async () => {
    let now = 5000;
    const nowFn = () => now;
    const runner = vi.fn(async () => {});
    const capabilities = () => ({
      p1: cap({ supportsCharts: true, hasQuota: true }),
    });
    const sched = createEnrichmentScheduler({
      capabilities,
      ttlMs: { chart: 10000 } as Record<EnrichmentKind, number>,
      runner,
      now: nowFn,
    });
    const key: UsageStoreKey = { providerId: "p1", accountKey: "default", sourceKey: "default" };
    await sched.trigger("chart", key);
    expect(runner).toHaveBeenCalledTimes(1);
    // within TTL, non-manual blocked
    expect(await sched.trigger("chart", key)).toBe(false);
    // manual forces
    expect(await sched.trigger("chart", key, { manual: true })).toBe(true);
    expect(runner).toHaveBeenCalledTimes(2);
  });

  it("trigger with mode respects capability and mode gating together", async () => {
    const runner = vi.fn(async () => {});
    const capabilities = () => ({
      p1: cap({ supportsCharts: true, hasQuota: true }),
    });
    const sched = createEnrichmentScheduler({ capabilities, ttlMs: {}, runner });
    const key: UsageStoreKey = { providerId: "p1", accountKey: "default", sourceKey: "default" };
    // trigger chart with minimal mode should be blocked even manual false
    expect(await sched.trigger("chart", key, { mode: "minimal" })).toBe(false);
    expect(runner).toHaveBeenCalledTimes(0);
    expect(await sched.trigger("chart", key, { mode: "detailed" })).toBe(true);
    expect(runner).toHaveBeenCalledTimes(1);
  });

  it("tick supports auto batch and failure isolation", async () => {
    const runner = vi.fn(async (kind: EnrichmentKind) => {
      if (kind === "outputSpeed") throw new Error("speed fail");
    });
    const capabilities = () => ({
      p1: cap({ supportsCharts: true, hasQuota: true, supportsOutputSpeed: true, supportsLocalCost: true, hasBalance: true }),
    });
    const sched = createEnrichmentScheduler({ capabilities, ttlMs: {}, runner });
    const key: UsageStoreKey = { providerId: "p1", accountKey: "default", sourceKey: "default" };
    // tick should not throw even though one runner fails
    await expect(sched.tick("detailed", [key])).resolves.toBeUndefined();
    // other kinds should still have been attempted
    expect(runner).toHaveBeenCalledWith("chart", key);
    expect(runner).toHaveBeenCalledWith("outputSpeed", key);
  });

  it("clear resets TTL cache", async () => {
    let now = 0;
    const nowFn = () => now;
    const runner = vi.fn(async () => {});
    const capabilities = () => ({ p1: cap({ supportsCharts: true, hasQuota: true }) });
    const sched = createEnrichmentScheduler({
      capabilities,
      ttlMs: { chart: 1000 } as Record<EnrichmentKind, number>,
      runner,
      now: nowFn,
    });
    const key: UsageStoreKey = { providerId: "p1", accountKey: "default", sourceKey: "default" };
    await sched.trigger("chart", key);
    expect(runner).toHaveBeenCalledTimes(1);
    expect(await sched.trigger("chart", key)).toBe(false);
    sched.clear("chart", key);
    expect(await sched.trigger("chart", key)).toBe(true);
    expect(runner).toHaveBeenCalledTimes(2);
  });
});
