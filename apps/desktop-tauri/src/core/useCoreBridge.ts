import { useCallback, useSyncExternalStore } from "react";
import { usageStoreKey, type UsageStoreKey, type UsageRecord, type UsageStore, createUsageStore } from "./usageStore";
import { createRefreshCoordinator, type RefreshCoordinator } from "./refreshCoordinator";
import { createActionDispatcher, type ActionDispatcher, type DispatchHandler } from "./actionDispatcher";
import type { SurfaceActionKind } from "./actions";

// Global defaults for surface consumption without explicit injection
const defaultStore: UsageStore = createUsageStore();
const defaultCoordinator: RefreshCoordinator = createRefreshCoordinator({
  store: defaultStore,
  fetcher: async () => {
    throw new Error("RefreshCoordinator has no fetcher - provide one via setCoreBridgeStore or createRefreshCoordinator");
  },
});

let globalStore: UsageStore | null = null;
let globalCoordinator: RefreshCoordinator | null = null;
let globalDispatcher: ActionDispatcher | null = null;

export function setCoreBridgeStore(store: UsageStore | null): void {
  globalStore = store;
}

export function setCoreBridgeCoordinator(coord: RefreshCoordinator | null): void {
  globalCoordinator = coord;
}

export function setCoreBridgeDispatcher(dispatcher: ActionDispatcher | null): void {
  globalDispatcher = dispatcher;
}

function resolveStore(override?: UsageStore): UsageStore {
  return override ?? globalStore ?? defaultStore;
}

function resolveCoordinator(override?: RefreshCoordinator): RefreshCoordinator {
  return override ?? globalCoordinator ?? defaultCoordinator;
}

export function useCoreSnapshot(
  key: UsageStoreKey,
  storeOverride?: UsageStore,
): UsageRecord | undefined {
  const store = resolveStore(storeOverride);
  const subscribe = useCallback(
    (cb: () => void) => store.subscribe(cb),
    [store],
  );
  const getSnapshot = useCallback(() => store.getSnapshot(), [store]);
  const getServerSnapshot = useCallback(() => store.getServerSnapshot(), [store]);
  const state = useSyncExternalStore(subscribe, getSnapshot, getServerSnapshot);
  const id = usageStoreKey(key);
  return (state.records as Record<string, UsageRecord>)[id];
}

export function useRefreshCommand(
  coordinatorOverride?: RefreshCoordinator,
): (key: UsageStoreKey, opts?: { manual?: boolean }) => Promise<UsageRecord> {
  const coordinator = resolveCoordinator(coordinatorOverride);
  return useCallback(
    (key: UsageStoreKey, opts?: { manual?: boolean }) => coordinator.refresh(key, opts),
    [coordinator],
  );
}

export function useActionDispatcher(
  dispatcherOverride?: ActionDispatcher,
): ActionDispatcher | ((action: Parameters<ActionDispatcher["dispatch"]>[0]) => Promise<ReturnType<ActionDispatcher["dispatch"]>>) {
  const dispatcher = dispatcherOverride ?? globalDispatcher;
  if (!dispatcher) {
    // fallback no-op dispatcher that returns unknown
    const noop = createActionDispatcher({});
    return noop;
  }
  return dispatcher;
}

// Convenience hook that directly returns dispatch function
export function useDispatchAction(
  dispatcherOverride?: ActionDispatcher,
): ActionDispatcher["dispatch"] {
  const d = useActionDispatcher(dispatcherOverride) as ActionDispatcher;
  return useCallback((action) => d.dispatch(action), [d]);
}

// For testing: create isolated bridge with custom store/coordinator
export function createCoreBridgeForTest(opts: {
  store?: UsageStore;
  coordinator?: RefreshCoordinator;
  handlers?: Partial<Record<SurfaceActionKind, DispatchHandler>>;
}): {
  store: UsageStore;
  coordinator: RefreshCoordinator;
  dispatcher: ActionDispatcher;
} {
  const store = opts.store ?? createUsageStore();
  const coordinator =
    opts.coordinator ??
    createRefreshCoordinator({
      store,
      fetcher: async (k) => {
        const rec = store.get(k);
        if (rec?.snapshot) return rec.snapshot;
        throw new Error("no snapshot");
      },
    });
  const dispatcher = createActionDispatcher(opts.handlers ?? {});
  return { store, coordinator, dispatcher };
}
