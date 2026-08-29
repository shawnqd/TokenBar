/**
 * App-level core runtime (CORE-05/07).
 *
 * Single UsageStore + RefreshCoordinator + EnrichmentScheduler +
 * ActionDispatcher shared by every surface. Created once per WebView entry;
 * the app shell wires the real Tauri fetcher/enrichment runner in, seeds the
 * store from cached bridge snapshots, and subscribes to provider-updated
 * events. `useCoreBridge` and `trayCoreStore` consume the same instances so
 * tray, float bar, taskbar preview and Settings read one snapshot.
 */
import { listen } from "@tauri-apps/api/event";
import {
  createUsageStore,
  type UsageFetcher,
  type UsageStore,
  type UsageStoreKey,
} from "./core/usageStore";
import {
  createRefreshCoordinator,
  type RefreshCoordinator,
} from "./core/refreshCoordinator";
import {
  createEnrichmentScheduler,
  type EnrichmentScheduler,
} from "./core/enrichmentScheduler";
import {
  createActionDispatcher,
  type ActionDispatcher,
} from "./core/actionDispatcher";
import {
  setCoreBridgeStore,
  setCoreBridgeCoordinator,
  setCoreBridgeDispatcher,
} from "./core/useCoreBridge";
import { fromBridge } from "./core/fromBridge";
import { trayCoreStore } from "./surfaces/tray/trayCoreStore";
import { setFloatBarLocalCostFetcher } from "./floatbar/floatBarStore";
import type { ProviderCapability, ProviderSnapshot } from "./core/snapshot";
import { canActivate, listSurfaces, type SurfaceDescriptor } from "./core/surfaceRegistry";
import type { ProviderUsageSnapshot } from "./types/bridge";
import {
  getCachedProviders,
  getOutputSpeedSnapshot,
  getProviderChartData,
  openSettingsWindow,
  openFlyoutWindow,
  openProviderDashboard,
  openProviderStatusPage,
  triggerProviderLogin,
  refreshProviders,
  refreshProvidersIfStale,
  quitApp,
} from "./lib/tauri";

export interface AppRuntime {
  store: UsageStore;
  coordinator: RefreshCoordinator;
  scheduler: EnrichmentScheduler;
  dispatcher: ActionDispatcher;
  fetchProvider: UsageFetcher;
  seedFromBridge: (snapshots: ProviderUsageSnapshot[]) => void;
  refreshAll: (opts?: { force?: boolean }) => Promise<void>;
  activeSurfaces: (settings: Record<string, unknown>) => SurfaceDescriptor[];
  dispose: () => void;
}

function capabilitiesOf(store: UsageStore): () => Record<string, ProviderCapability> {
  return () => {
    const map: Record<string, ProviderCapability> = {};
    for (const record of Object.values(store.getSnapshot().records)) {
      if (record.snapshot) map[record.snapshot.providerId] = record.snapshot.capabilities;
    }
    return map;
  };
}

/**
 * Resolve one provider snapshot for a store key.
 *
 * - The backend cache is authoritative for everything the bridge exposes:
 *   we look up by providerId first and refine with sourceKey/accountKey
 *   when the bridge snapshot carries them, so multi-account providers do
 *   not collapse into the first matching row (CORE-03 key isolation).
 * - When the store has no snapshot or the cached one is stale, we trigger
 *   one stale-aware backend round before reading the cache again, so a
 *   manual refresh (Ctrl+R / refresh on open) actually reaches the backend
 *   while automatic TTL reads stay cheap.
 * - Unknown data returns the record/error, never a fabricated percentage.
 */
async function fetchForKey(
  store: UsageStore,
  key: UsageStoreKey,
): Promise<ProviderSnapshot> {
  const record = store.get(key);
  const staleMs = 15 * 60 * 1000;
  const stale =
    record == null ||
    record.snapshot == null ||
    record.snapshot.updatedAt == null ||
    Date.now() - Date.parse(record.snapshot.updatedAt) > staleMs;
  if (stale) {
    await refreshProvidersIfStale().catch(() => {});
  }
  const cached = await getCachedProviders();
  const matches = cached.filter((s) => s.providerId === key.providerId);
  // The backend cache stores one row per provider (upsert by providerId), so
  // exact multi-account matching cannot be honored on this bridge surface;
  // match the closest row by the fields the bridge actually carries, then
  // fall back to the sole cached row.
  const found =
    matches.find((s) => s.accountEmail === key.accountKey) ??
    matches.find((s) => s.sourceLabel === key.sourceKey) ??
    matches[0];
  if (found) {
    try {
      return (fromBridge(found as unknown as Parameters<typeof fromBridge>[0]) as unknown) as ProviderSnapshot;
    } catch {
      // fall through to the cached record below
    }
  }
  if (record?.snapshot) return record.snapshot;
  throw new Error(
    "app-runtime: no snapshot for " + key.providerId,
  );
}

/**
 * Generation-domain note (review round 2, M5).
 *
 * UsageStore and RefreshCoordinator keep independent generation counters:
 * the coordinator bumps per refresh round, the store per upsert/seed. An
 * in-flight coordinator fetch resolving AFTER a fresh seed /
 * provider-updated event could overwrite the newer snapshot with the
 * older result.
 *
 * Accepted risk, no live repro observed: the fetcher re-reads the same
 * backend cache the seed used, the Rust side coalesces refetches, and
 * surfaces re-render from the store. Coupling the two counters would
 * serialize seeds, so this is documented instead.
 */

export function buildAppRuntime(): AppRuntime {
  const store = createUsageStore();

  const scheduler = createEnrichmentScheduler({
    capabilities: capabilitiesOf(store),
    ttlMs: {},
    runner: async (kind, key) => {
      const snapshot = store.get(key)?.snapshot;
      if (!snapshot) return;
      if (kind === "chart" && snapshot.capabilities.supportsCharts) {
        await getProviderChartData(key.providerId);
      } else if (kind === "outputSpeed" && snapshot.capabilities.supportsOutputSpeed) {
        await getOutputSpeedSnapshot();
      }
    },
  });

  const coordinator = createRefreshCoordinator({
    store,
    fetcher: (key) => fetchForKey(store, key),
    enrich: async (snapshot) => {
      const key: UsageStoreKey = {
        providerId: snapshot.providerId,
        accountKey: snapshot.accountKey,
        sourceKey: snapshot.sourceKey,
      };
      await scheduler.trigger("outputSpeed", key, { manual: true }).catch(() => {});
      await scheduler.trigger("chart", key, { manual: true }).catch(() => {});
    },
  });

  const dispatcher = createActionDispatcher({
    refresh: async () => {
      await refreshProviders();
      return { status: "handled" };
    },
    openSettings: async (action) => {
      const target = action.target as { kind: string; tab?: string } | undefined;
      await openSettingsWindow(target?.tab ?? "general");
      return { status: "handled" };
    },
    quit: async () => {
      await quitApp();
      return { status: "handled" };
    },
    selectProvider: async () => {
      // Opens the flyout; the tray panel reads core records directly and
      // listens for flyout-select-provider to focus a specific provider.
      await openFlyoutWindow().catch(() => {});
      return { status: "handled" };
    },
    openProviderDetail: async () => {
      await openSettingsWindow("providers");
      return { status: "handled" };
    },
    openExternalUsage: async (action) => {
      const t = action.target as { providerId?: string } | undefined;
      if (!t?.providerId) return { status: "error", error: "missing providerId" };
      await openProviderDashboard(t.providerId);
      return { status: "handled" };
    },
    openExternalStatus: async (action) => {
      const t = action.target as { providerId?: string } | undefined;
      if (!t?.providerId) return { status: "error", error: "missing providerId" };
      await openProviderStatusPage(t.providerId);
      return { status: "handled" };
    },
    triggerLogin: async (action) => {
      const t = action.target as { providerId?: string } | undefined;
      if (!t?.providerId) return { status: "error", error: "missing providerId" };
      await triggerProviderLogin(t.providerId);
      return { status: "handled" };
    },
  });

  setCoreBridgeStore(store);
  setCoreBridgeCoordinator(coordinator);
  setCoreBridgeDispatcher(dispatcher);

  const runtime: AppRuntime = {
    store,
    coordinator,
    scheduler,
    dispatcher,
    fetchProvider: (key) => fetchForKey(store, key),
    seedFromBridge: (snapshots) => {
      for (const s of snapshots) {
        try {
          store.upsert((fromBridge(s as unknown as Parameters<typeof fromBridge>[0]) as unknown) as never);
        } catch {
          // malformed snapshot: skip, never fabricate
        }
      }
    },
    activeSurfaces: (settings) =>
      listSurfaces().filter((s) => canActivate(s.kind, settings)),
    refreshAll: async (opts) => {
      if (opts?.force) await refreshProviders();
      else await refreshProvidersIfStale();
    },
    dispose: () => {
      setCoreBridgeStore(null);
      setCoreBridgeDispatcher(null);
    },
  };
  return runtime;
}

let runtime: AppRuntime | null = null;
let stopListeners: Array<() => void> = [];

export function getAppRuntime(): AppRuntime {
  if (!runtime) runtime = buildAppRuntime();
  return runtime;
}

export function hasAppRuntime(): boolean {
  return runtime != null;
}

/** Seed the shared store from the backend cache on boot. */
export async function seedAppRuntime(r: AppRuntime): Promise<void> {
  try {
    const cached = await getCachedProviders();
    r.seedFromBridge(cached);
  } catch {
    // backend not ready yet: surfaces render empty/loading states
  }
}

/** Wire shared provider events into the store and tray store. */
export function startAppRuntimeWiring(r: AppRuntime): void {
  trayCoreStore.setFetcher(r.fetchProvider);
  setFloatBarLocalCostFetcher(async (providerId) => {
    const summary = await import("./lib/tauri").then((m) =>
      m.getProviderLocalUsageSummary(providerId),
    );
    return summary == null ||
      summary.todayCost == null && summary.thirtyDayCost == null
      ? null
      : { todayCost: summary.todayCost ?? 0, thirtyDayCost: summary.thirtyDayCost ?? 0 };
  });
  trayCoreStore.setEnrichmentRunner(async (kind, key) => {
    const snapshot = r.store.get(key)?.snapshot;
    if (!snapshot) return;
    if (kind === "chart" && snapshot.capabilities.supportsCharts) {
      const data = await getProviderChartData(key.providerId);
      trayCoreStore.commitEnrichment("chart", key, { ok: true, chartData: data });
    } else if (kind === "outputSpeed" && snapshot.capabilities.supportsOutputSpeed) {
      const out = await getOutputSpeedSnapshot();
      const perProvider = (out as unknown as Record<string, import("./types/bridge").ProviderOutputSpeed>)[key.providerId];
      if (perProvider) {
        trayCoreStore.commitEnrichment("outputSpeed", key, { ok: true, outputSpeed: perProvider });
      }
    }
  });

  void listen<ProviderUsageSnapshot>("provider-updated", (evt) => {
    try {
      const snapshot = fromBridge(evt.payload as unknown as Parameters<typeof fromBridge>[0]) as unknown as never;
      r.seedFromBridge([evt.payload]);
      trayCoreStore.seed(snapshot);
    } catch {
      // malformed event payload: ignore
    }
  })
    .then((unlisten) => stopListeners.push(unlisten))
    .catch(() => {
      // running outside Tauri (tests/web): events unavailable, surfaces stay empty
    });
}

export function stopAppRuntimeWiring(): void {
  for (const fn of stopListeners) {
    try {
      fn();
    } catch {
      // ignore
    }
  }
  stopListeners = [];
}

export function disposeAppRuntime(): void {
  stopAppRuntimeWiring();
  if (runtime) {
    runtime.dispose();
    runtime = null;
  }
}

/** Idempotent boot entry used by App.tsx. */
export async function ensureAppRuntimeBooted(): Promise<AppRuntime> {
  const r = getAppRuntime();
  await seedAppRuntime(r).catch(() => {});
  if (stopListeners.length === 0) startAppRuntimeWiring(r);
  return r;
}
