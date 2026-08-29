import { useCallback, useSyncExternalStore } from "react";
import { usageStoreKey, type UsageStoreKey, type UsageRecord, type UsageStore } from "./usageStore";
import { type RefreshCoordinator } from "./refreshCoordinator";
import { createActionDispatcher, type ActionDispatcher } from "./actionDispatcher";

let globalStore: UsageStore | null = null;
let globalCoordinator: RefreshCoordinator | null = null;
let globalDispatcher: ActionDispatcher | null = null;
let fallbackDispatcher: ActionDispatcher | null = null;

export function setCoreBridgeStore(store: UsageStore | null): void {
  globalStore = store;
}

export function setCoreBridgeCoordinator(coord: RefreshCoordinator | null): void {
  globalCoordinator = coord;
}

export function setCoreBridgeDispatcher(dispatcher: ActionDispatcher | null): void {
  globalDispatcher = dispatcher;
  fallbackDispatcher = null;
}

export function getCoreBridgeStore(): UsageStore | null {
  return globalStore;
}

function resolveStore(override?: UsageStore): UsageStore {
  const store = override ?? globalStore;
  if (!store) {
    throw new Error("core bridge store not attached");
  }
  return store;
}

function resolveCoordinator(override?: RefreshCoordinator): RefreshCoordinator {
  const coordinator = override ?? globalCoordinator;
  if (!coordinator) {
    throw new Error("core bridge coordinator not attached");
  }
  return coordinator;
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

function getFallbackDispatcher(): ActionDispatcher {
  if (!fallbackDispatcher) {
    fallbackDispatcher = createActionDispatcher(
      {},
      {
        fallback: async (action) => {
          const { invokeSurfaceAction } = await import("../lib/tauri");
          const data = await invokeSurfaceAction(action);
          return { status: "handled", data };
        },
      },
    );
  }
  return fallbackDispatcher;
}

export function useActionDispatcher(
  dispatcherOverride?: ActionDispatcher,
): ActionDispatcher | ((action: Parameters<ActionDispatcher["dispatch"]>[0]) => Promise<ReturnType<ActionDispatcher["dispatch"]>>) {
  return dispatcherOverride ?? globalDispatcher ?? getFallbackDispatcher();
}

// Convenience hook that directly returns dispatch function
export function useDispatchAction(
  dispatcherOverride?: ActionDispatcher,
): ActionDispatcher["dispatch"] {
  const d = useActionDispatcher(dispatcherOverride) as ActionDispatcher;
  return useCallback((action) => d.dispatch(action), [d]);
}
