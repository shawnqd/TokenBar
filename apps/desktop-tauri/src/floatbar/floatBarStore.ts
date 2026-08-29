import { useCallback, useSyncExternalStore } from "react";
import { type UsageStore } from "../core/usageStore";
import type { ProviderSnapshot } from "../core/snapshot";
import { getCoreBridgeStore } from "../core/useCoreBridge";
import { readProviderLocalCost } from "../core/enrichmentAccess";

const EMPTY_STORE_STATE = Object.freeze({
  version: 0,
  records: Object.freeze({}),
});

let testLocalCostFetcher:
  | ((providerId: string) => Promise<{ todayCost: number; thirtyDayCost: number } | null>)
  | null = null;

function createClearableStore(): UsageStore & {
  clearForTest: () => void;
  bind: (next: UsageStore) => void;
} {
  let current: UsageStore | null = null;
  const rebind = (store: UsageStore & { clearForTest: () => void; bind: (next: UsageStore) => void }) => {
    store.subscribe = (cb) => current?.subscribe(cb) ?? (() => {});
    store.getSnapshot = () => current?.getSnapshot() ?? EMPTY_STORE_STATE;
    store.getServerSnapshot = () => current?.getServerSnapshot() ?? EMPTY_STORE_STATE;
    store.get = (k) => current?.get(k);
    store.upsert = (s) => {
      if (!current) throw new Error("floatbar: core projection not attached");
      return current.upsert(s);
    };
    store.refresh = (k) => {
      if (!current) return Promise.reject(new Error("floatbar: core projection not attached"));
      return current.refresh(k);
    };
    store.setFetcher = (f) => {
      if (!current) throw new Error("floatbar: core projection not attached");
      current.setFetcher(f);
    };
  };
  const store = {} as UsageStore & { clearForTest: () => void; bind: (next: UsageStore) => void };
  rebind(store);
  store.bind = (next: UsageStore) => {
    current = next;
    rebind(store);
  };
  store.clearForTest = () => {
    throw new Error("floatbar: use __clearFloatBarStoreForTest from floatBarStore.testSupport");
  };
  return store;
}

export const floatBarStore: UsageStore & {
  clearForTest: () => void;
  bind: (next: UsageStore) => void;
} = createClearableStore();

/** Bind to the process-wide UsageStore so FloatBar is not a third cache. */
export function attachFloatBarStore(store: UsageStore): void {
  floatBarStore.bind(store);
}

/**
 * Bind to the process projection if the app runtime has attached one.
 * Tests that need cache fixtures must call `seedFloatBarFromCacheForTest`
 * themselves; this function never starts a listener or reads the backend.
 */
export function ensureFloatBarStoreSync(): void {
  const shared = getCoreBridgeStore();
  if (shared) {
    floatBarStore.bind(shared);
  }
}

export function stopFloatBarStoreSync(): void {}

/** Test seam only; production reads the process-owned Rust cache via the core adapter. */
export function setFloatBarLocalCostFetcher(
  fetcher:
    | ((providerId: string) => Promise<{ todayCost: number; thirtyDayCost: number } | null>)
    | null,
): void {
  if ((import.meta as { env?: { MODE?: string } }).env?.MODE !== "test") {
    throw new Error("floatbar: local cost injection is test-only");
  }
  testLocalCostFetcher = fetcher;
}

export function hasFloatBarLocalCostFetcher(): boolean {
  return testLocalCostFetcher != null || (import.meta as { env?: { MODE?: string } }).env?.MODE !== "test";
}

export function readFloatBarLocalCost(_providerId: string): { todayCost: number; thirtyDayCost: number } | null {
  return null;
}

export async function fetchFloatBarLocalCost(providerId: string): Promise<{ todayCost: number; thirtyDayCost: number } | null> {
  return testLocalCostFetcher
    ? testLocalCostFetcher(providerId)
    : readProviderLocalCost(providerId);
}

export function invalidateFloatBarLocalCosts(_providerIds: string[]): void {}

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
