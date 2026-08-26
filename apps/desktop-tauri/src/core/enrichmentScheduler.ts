import type { ProviderCapability } from "./snapshot";
import type { UsageStoreKey } from "./usageStore";
import { usageStoreKey } from "./usageStore";

export type EnrichmentKind =
  | "chart"
  | "localCost"
  | "outputSpeed"
  | "credits"
  | "forecast"
  | "diagnostics";

export type SurfaceDensity = "minimal" | "compact" | "detailed" | "full";

export interface EnrichmentSchedulerOptions {
  capabilities: () => Record<string, ProviderCapability>;
  ttlMs: Partial<Record<EnrichmentKind, number>>;
  runner: (kind: EnrichmentKind, key: UsageStoreKey) => Promise<void>;
  now?: () => number;
}

export interface EnrichmentScheduler {
  trigger: (
    kind: EnrichmentKind,
    key: UsageStoreKey,
    opts?: { manual?: boolean; mode?: SurfaceDensity },
  ) => Promise<boolean>;
  tick: (mode: SurfaceDensity, keys: UsageStoreKey[]) => Promise<void>;
  getLastRun: (kind: EnrichmentKind, key: UsageStoreKey) => number | null;
  clear: (kind?: EnrichmentKind, key?: UsageStoreKey) => void;
  destroy: () => void;
}

type GateFn = (cap: ProviderCapability) => boolean;

const CAPABILITY_GATE: Record<EnrichmentKind, GateFn> = {
  chart: (cap) => cap.supportsCharts,
  localCost: (cap) => cap.supportsLocalCost,
  outputSpeed: (cap) => cap.supportsOutputSpeed,
  credits: (cap) => cap.hasBalance || cap.supportsLocalCost,
  forecast: (cap) => cap.hasQuota,
  diagnostics: () => true,
};

function needsForMode(kind: EnrichmentKind, mode: SurfaceDensity): boolean {
  if (kind === "chart") {
    return mode === "detailed" || mode === "full";
  }
  if (kind === "localCost") {
    // chart already gated, localCost not needed in minimal
    return mode !== "minimal";
  }
  if (kind === "forecast") {
    return mode === "detailed" || mode === "full";
  }
  if (kind === "diagnostics") {
    return mode === "detailed" || mode === "full";
  }
  // outputSpeed and credits are needed in all densities where capability allows
  return true;
}

function enrichmentKey(kind: EnrichmentKind, key: UsageStoreKey): string {
  return `${kind}::${usageStoreKey(key)}`;
}

export function createEnrichmentScheduler(
  options: EnrichmentSchedulerOptions,
): EnrichmentScheduler {
  const nowFn = options.now ?? (() => Date.now());
  const ttlMs = options.ttlMs;
  const runner = options.runner;
  const capabilitiesFn = options.capabilities;

  const lastRun = new Map<string, number>();
  const inflight = new Map<string, Promise<void>>();

  const getLastRun = (
    kind: EnrichmentKind,
    key: UsageStoreKey,
  ): number | null => {
    const k = enrichmentKey(kind, key);
    return lastRun.get(k) ?? null;
  };

  const clear = (kind?: EnrichmentKind, key?: UsageStoreKey): void => {
    if (kind && key) {
      lastRun.delete(enrichmentKey(kind, key));
      inflight.delete(enrichmentKey(kind, key));
      return;
    }
    if (kind && !key) {
      for (const k of [...lastRun.keys()]) {
        if (k.startsWith(`${kind}::`)) lastRun.delete(k);
      }
      for (const k of [...inflight.keys()]) {
        if (k.startsWith(`${kind}::`)) inflight.delete(k);
      }
      return;
    }
    if (!kind && key) {
      const suffix = usageStoreKey(key);
      for (const k of [...lastRun.keys()]) {
        if (k.endsWith(suffix)) lastRun.delete(k);
      }
      for (const k of [...inflight.keys()]) {
        if (k.endsWith(suffix)) inflight.delete(k);
      }
      return;
    }
    lastRun.clear();
    inflight.clear();
  };

  const destroy = (): void => {
    lastRun.clear();
    inflight.clear();
  };

  const canRun = (
    kind: EnrichmentKind,
    key: UsageStoreKey,
    opts?: { manual?: boolean; mode?: SurfaceDensity },
  ): boolean => {
    const caps = capabilitiesFn();
    const cap = caps[key.providerId];
    if (!cap) return false;
    const gate = CAPABILITY_GATE[kind];
    if (!gate(cap)) return false;
    if (opts?.mode && !needsForMode(kind, opts.mode)) return false;
    // TTL check
    if (!opts?.manual) {
      const ttl = ttlMs[kind];
      if (ttl != null && ttl > 0) {
        const last = lastRun.get(enrichmentKey(kind, key));
        if (last != null && nowFn() - last < ttl) return false;
      }
    }
    return true;
  };

  const trigger = async (
    kind: EnrichmentKind,
    key: UsageStoreKey,
    opts?: { manual?: boolean; mode?: SurfaceDensity },
  ): Promise<boolean> => {
    if (!canRun(kind, key, opts)) return false;

    const k = enrichmentKey(kind, key);
    const pending = inflight.get(k);
    if (pending) {
      try {
        await pending;
      } catch {
        // ignore, caller cares only whether we attempted
      }
      return true;
    }

    const run = (async (): Promise<void> => {
      await runner(kind, key);
      lastRun.set(k, nowFn());
    })();

    const tracked = run.finally(() => {
      if (inflight.get(k) === tracked) inflight.delete(k);
    });
    inflight.set(k, tracked);
    try {
      await tracked;
      return true;
    } catch {
      // failure isolation: do not throw, but return false and don't update lastRun
      return false;
    }
  };

  const tick = async (
    mode: SurfaceDensity,
    keys: UsageStoreKey[],
  ): Promise<void> => {
    const kinds: EnrichmentKind[] = [
      "chart",
      "localCost",
      "outputSpeed",
      "credits",
      "forecast",
      "diagnostics",
    ];
    const needed = kinds.filter((k) => needsForMode(k, mode));
    const tasks: Promise<boolean>[] = [];
    for (const kind of needed) {
      for (const key of keys) {
        // tick is automatic, not manual, so TTL and capability gate apply
        if (!canRun(kind, key, { mode })) continue;
        tasks.push(trigger(kind, key, { mode }));
      }
    }
    await Promise.allSettled(tasks);
  };

  return {
    trigger,
    tick,
    getLastRun,
    clear,
    destroy,
  };
}
