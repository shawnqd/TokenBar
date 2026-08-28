import { useCallback, useSyncExternalStore } from "react";
import { listen } from "@tauri-apps/api/event";
import { createUsageStore, type UsageStore } from "../core/usageStore";
import { fromBridge } from "../core/fromBridge";
import type { ProviderSnapshot } from "../core/snapshot";
import { getCachedProviders } from "../lib/tauri";
import type { ProviderUsageSnapshot } from "../types/bridge";

function createClearableStore(): UsageStore & { clearForTest: () => void } {
  let current: UsageStore = createUsageStore();
  const store: UsageStore & { clearForTest: () => void } = {
    subscribe: (cb) => current.subscribe(cb),
    getSnapshot: () => current.getSnapshot(),
    getServerSnapshot: () => current.getServerSnapshot(),
    get: (k) => current.get(k),
    upsert: (s) => current.upsert(s),
    refresh: (k) => (current as any).refresh?.(k),
    setFetcher: (f) => (current as any).setFetcher?.(f),
  } as UsageStore & { clearForTest: () => void };
  store.clearForTest = () => {
    current = createUsageStore();
    store.subscribe = (cb) => current.subscribe(cb);
    store.getSnapshot = () => current.getSnapshot();
    store.getServerSnapshot = () => current.getServerSnapshot();
    store.get = (k) => current.get(k);
    store.upsert = (s) => current.upsert(s);
    store.refresh = (k) => (current as any).refresh?.(k);
    store.setFetcher = (f) => (current as any).setFetcher?.(f);
  };
  return store;
}

export const floatBarStore: UsageStore & { clearForTest: () => void } =
  createClearableStore();

let syncStarted = false;
let stopSync: (() => void) | null = null;

export function ensureFloatBarStoreSync(): void {
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
