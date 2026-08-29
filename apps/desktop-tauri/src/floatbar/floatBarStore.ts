import { useCallback, useSyncExternalStore } from "react";
import { listen } from "@tauri-apps/api/event";
import { createUsageStore, type UsageStore } from "../core/usageStore";
import { fromBridge } from "../core/fromBridge";
import type { ProviderSnapshot } from "../core/snapshot";
import { getCoreBridgeStore } from "../core/useCoreBridge";
import { getCachedProviders } from "../lib/tauri";
import type { ProviderUsageSnapshot } from "../types/bridge";

function createClearableStore(): UsageStore & {
  clearForTest: () => void;
  bind: (next: UsageStore) => void;
} {
  let current: UsageStore = createUsageStore();
  const rebind = (store: UsageStore & { clearForTest: () => void; bind: (next: UsageStore) => void }) => {
    store.subscribe = (cb) => current.subscribe(cb);
    store.getSnapshot = () => current.getSnapshot();
    store.getServerSnapshot = () => current.getServerSnapshot();
    store.get = (k) => current.get(k);
    store.upsert = (s) => current.upsert(s);
    store.refresh = (k) => current.refresh(k);
    store.setFetcher = (f) => current.setFetcher(f);
  };
  const store = {} as UsageStore & { clearForTest: () => void; bind: (next: UsageStore) => void };
  rebind(store);
  store.bind = (next: UsageStore) => {
    current = next;
    rebind(store);
  };
  store.clearForTest = () => {
    current = createUsageStore();
    rebind(store);
  };
  return store;
}

export const floatBarStore: UsageStore & {
  clearForTest: () => void;
  bind: (next: UsageStore) => void;
} = createClearableStore();

let syncStarted = false;
let stopSync: (() => void) | null = null;

let localCostFetcher: ((providerId: string) => Promise<{ todayCost: number; thirtyDayCost: number } | null>) | null = null;
const localCostsCache = new Map<string, { cost: { todayCost: number; thirtyDayCost: number } | null; at: number }>();

/**
 * Inject the backend local-usage loader (wired by appRuntime). Components must
 * never call the Tauri command themselves; they read the cache via
 * `readLocalCost` / `invalidateLocalCosts` below.
 */
export function setFloatBarLocalCostFetcher(
  fetcher:
    | ((providerId: string) => Promise<{ todayCost: number; thirtyDayCost: number } | null>)
    | null,
): void {
  if (fetcher == null) {
    localCostFetcher = null;
    localCostsCache.clear();
    return;
  }
  localCostFetcher = fetcher;
}



/** Bind to the process-wide UsageStore so FloatBar is not a third cache. */
export function attachFloatBarStore(store: UsageStore): void {
  floatBarStore.bind(store);
  stopFloatBarStoreSync();
}

export function ensureFloatBarStoreSync(): void {
  const shared = getCoreBridgeStore();
  if (shared) {
    floatBarStore.bind(shared);
    return;
  }
  if (syncStarted) return;
  syncStarted = true;
  let cancelled = false;
  void getCachedProviders()
    .then((cached) => {
      if (cancelled) return;
      for (const snap of cached) {
        try {
          floatBarStore.upsert(fromBridge(snap as unknown as ProviderUsageSnapshot & { displayState?: string } as any));
        } catch {
          // ignore malformed snapshot in tests
        }
      }
    })
    .catch(() => {});

  let unlisten: (() => void) | undefined;
  void listen<ProviderUsageSnapshot>("provider-updated", (event) => {
    if (cancelled) return;
    try {
      floatBarStore.upsert(fromBridge(event.payload as unknown as ProviderUsageSnapshot & { displayState?: string } as any));
    } catch {}
  })
    .then((fn) => {
      if (cancelled) fn();
      else unlisten = fn;
    })
    .catch(() => {});

  stopSync = () => {
    cancelled = true;
    syncStarted = false;
    if (unlisten) {
      try {
        unlisten();
      } catch {}
      unlisten = undefined;
    }
    stopSync = null;
  };
}

export function stopFloatBarStoreSync(): void {
  if (stopSync) stopSync();
}

export function hasFloatBarLocalCostFetcher(): boolean {
  return localCostFetcher != null;
}

export function readFloatBarLocalCost(providerId: string): { todayCost: number; thirtyDayCost: number } | null {
  const entry = localCostsCache.get(providerId);
  if (!entry) return null;
  if (Date.now() - entry.at > 15 * 60 * 1000) {
    localCostsCache.delete(providerId);
    return null;
  }
  return entry.cost;
}

export async function fetchFloatBarLocalCost(providerId: string): Promise<{ todayCost: number; thirtyDayCost: number } | null> {
  if (!localCostFetcher) return null;
  const hit = readFloatBarLocalCost(providerId);
  if (hit) return hit;
  try {
    const cost = await localCostFetcher(providerId);
    localCostsCache.set(providerId, { cost, at: Date.now() });
    return cost;
  } catch {
    return null;
  }
}

export function invalidateFloatBarLocalCosts(providerIds: string[]): void {
  for (const id of providerIds) localCostsCache.delete(id);
}

export function useFloatBarSnapshots(store: UsageStore = floatBarStore): ProviderSnapshot[] {
  const subscribe = useCallback((cb: () => void) => store.subscribe(cb), [store]);
  const getSnapshot = useCallback(() => store.getSnapshot(), [store]);
  const getServerSnapshot = useCallback(() => store.getServerSnapshot(), [store]);
  const state = useSyncExternalStore(subscribe, getSnapshot, getServerSnapshot);
  const records = state.records as Record<string, { snapshot: ProviderSnapshot | null }>;
  const list: ProviderSnapshot[] = [];
  for (const rec of Object.values(records)) {
    if (rec.snapshot) list.push(rec.snapshot);
  }
  return list;
}

export function __clearFloatBarStoreForTest(): void {
  try {
    (floatBarStore as any).clearForTest?.();
  } catch {}
  syncStarted = false;
  if (stopSync) {
    try { stopSync(); } catch {}
    stopSync = null;
  }
}
