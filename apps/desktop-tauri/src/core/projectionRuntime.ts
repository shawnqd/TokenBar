import { fromBridge } from "./fromBridge";
import type {
  ProviderUsageSnapshot,
  VersionedProviderProjection,
} from "../types/bridge";
import {
  keyFromSnapshot,
  recordFromSnapshot,
  usageStoreKey,
  type UsageFetcher,
  type UsageRecord,
  type UsageStore,
  type UsageStoreKey,
  type UsageStoreState,
} from "./usageStore";
import type {
  EnrichmentKind,
  EnrichmentScheduler,
  SurfaceDensity,
} from "./enrichmentScheduler";
import type {
  RefreshCoordinator,
  RefreshEvent,
  RefreshEventKind,
} from "./refreshCoordinator";

/**
 * Read-only process projection consumed by every WebView.
 *
 * Rust owns refresh/enrichment and publishes a monotonically increasing full
 * projection.  This object is only a versioned local replica: it never owns a
 * fetcher, timer, enrichment runner, or authoritative cache.  `upsert` and
 * `refresh` remain on the legacy UsageStore shape for compatibility with
 * existing selectors, but fail closed so a surface cannot become a writer by
 * accident.
 */
export interface ProjectionStore extends UsageStore {
  applyProjection: (projection: VersionedProviderProjection) => boolean;
  projectionVersion: () => number;
  isReadOnlyProjection: true;
}

function emptyRecord(key: UsageStoreKey): UsageRecord {
  return {
    key,
    snapshot: null,
    lastGood: null,
    snapshotVersion: 0,
    displayState: "unknown",
    sourceHealth: "unknown",
    error: null,
  };
}

function projectionVersionOf(value: unknown): number | null {
  if (typeof value !== "number" || !Number.isSafeInteger(value) || value < 0) {
    return null;
  }
  return value;
}

export function buildProjectionStore(): ProjectionStore {
  let state: UsageStoreState = {
    version: 0,
    records: Object.freeze({}),
  };
  let version = -1;
  const listeners = new Set<() => void>();

  const subscribe = (listener: () => void): (() => void) => {
    listeners.add(listener);
    return () => listeners.delete(listener);
  };

  const applyProjection = (projection: VersionedProviderProjection): boolean => {
    const nextVersion = projectionVersionOf(projection?.version);
    if (nextVersion == null || nextVersion < version) return false;
    if (nextVersion === version) return false;

    const next: Record<string, UsageRecord> = {};
    for (const bridgeSnapshot of projection.snapshots ?? []) {
      try {
        const snapshot = fromBridge(
          bridgeSnapshot as unknown as Parameters<typeof fromBridge>[0],
        );
        const key = keyFromSnapshot(snapshot);
        const id = usageStoreKey(key);
        next[id] = recordFromSnapshot(
          key,
          state.records[id],
          snapshot,
          snapshot.error,
        );
      } catch {
        // One malformed row must not poison the rest of the process snapshot.
      }
    }

    version = nextVersion;
    state = {
      version: nextVersion,
      records: Object.freeze(next),
    };
    for (const listener of [...listeners]) {
      try {
        listener();
      } catch {
        // Consumer errors must not stop other WebViews from observing updates.
      }
    }
    return true;
  };

  const readOnlyWrite = (): never => {
    throw new Error(
      "core projection is read-only; refresh/enrichment must be dispatched to Rust",
    );
  };

  return {
    isReadOnlyProjection: true,
    subscribe,
    getSnapshot: () => state,
    getServerSnapshot: () => state,
    get: (key) => state.records[usageStoreKey(key)],
    upsert: readOnlyWrite,
    refresh: async () => readOnlyWrite(),
    setFetcher: () => readOnlyWrite(),
    applyProjection,
    projectionVersion: () => version,
  };
}

export interface ProjectionCoordinatorOptions {
  store: ProjectionStore;
  dispatchRefresh: (opts?: { force?: boolean }) => Promise<unknown>;
  readProjection: () => Promise<VersionedProviderProjection>;
  now?: () => number;
}

/**
 * Coordinator facade for existing hooks.  It contains no fetch/inflight
 * state: the only refresh request is canonical `surface_action`, while Rust
 * coalesces concurrent process-wide refreshes and emits the projection.
 */
export function buildProjectionCoordinator(
  options: ProjectionCoordinatorOptions,
): RefreshCoordinator {
  const now = options.now ?? (() => Date.now());
  const handlers = new Map<RefreshEventKind, Set<(event: RefreshEvent) => void>>();
  const trace: RefreshEvent[] = [];

  const emit = (
    kind: RefreshEventKind,
    key: UsageStoreKey,
    error?: string,
  ): void => {
    const event: RefreshEvent = {
      kind,
      key,
      providerId: key.providerId,
      at: now(),
      ...(error ? { error } : {}),
    };
    trace.push(event);
    if (trace.length > 48) trace.shift();
    for (const handler of [...(handlers.get(kind) ?? [])]) {
      try {
        handler(event);
      } catch {
        // Diagnostics are best effort and cannot affect the refresh boundary.
      }
    }
  };

  const sync = async (): Promise<void> => {
    try {
      options.store.applyProjection(await options.readProjection());
    } catch {
      // The event listener or the next boot read will repair this replica.
    }
  };

  const refresh = async (
    key: UsageStoreKey,
    opts?: { manual?: boolean },
  ): Promise<UsageRecord> => {
    emit("started", key);
    try {
      await options.dispatchRefresh({ force: Boolean(opts?.manual) });
      await sync();
      emit("core-complete", key);
      return options.store.get(key) ?? emptyRecord(key);
    } catch (error) {
      const message = error instanceof Error ? error.message : String(error);
      emit("failed", key, message);
      return options.store.get(key) ?? emptyRecord(key);
    }
  };

  const refreshAll = async (
    keys: UsageStoreKey[],
    opts?: { manual?: boolean },
  ): Promise<UsageRecord[]> => {
    if (keys.length === 0) return [];
    emit("started", keys[0]);
    try {
      await options.dispatchRefresh({ force: Boolean(opts?.manual) });
      await sync();
      for (const key of keys) emit("core-complete", key);
    } catch (error) {
      const message = error instanceof Error ? error.message : String(error);
      for (const key of keys) emit("failed", key, message);
    }
    return keys.map((key) => options.store.get(key) ?? emptyRecord(key));
  };

  const on = (
    kind: RefreshEventKind,
    handler: (event: RefreshEvent) => void,
  ): (() => void) => {
    let set = handlers.get(kind);
    if (!set) {
      set = new Set();
      handlers.set(kind, set);
    }
    set.add(handler);
    return () => set?.delete(handler);
  };

  const off = (
    kind: RefreshEventKind,
    handler: (event: RefreshEvent) => void,
  ): void => {
    handlers.get(kind)?.delete(handler);
  };

  return {
    getSnapshot: options.store.getSnapshot,
    getServerSnapshot: options.store.getServerSnapshot,
    subscribe: options.store.subscribe,
    get: options.store.get,
    refresh,
    refreshAll,
    // Automatic refresh is a Rust process service. These methods intentionally
    // do nothing so a WebView cannot create a second timer or writer.
    startScheduler: () => undefined,
    stopScheduler: () => undefined,
    on,
    off,
    cancel: () => undefined,
    destroy: () => {
      handlers.clear();
      trace.length = 0;
    },
    getStore: () => options.store,
    getTrace: () => [...trace],
  };
}

/** No-op facade retained for selectors while Rust owns enrichment scheduling. */
export function buildProjectionEnrichmentScheduler(): EnrichmentScheduler {
  return {
    trigger: async (
      _kind: EnrichmentKind,
      _key: UsageStoreKey,
      _opts?: { manual?: boolean; mode?: SurfaceDensity },
    ) => false,
    tick: async () => undefined,
    getLastRun: () => null,
    clear: () => undefined,
    destroy: () => undefined,
  };
}

// Keep these type imports visible at the boundary so callers do not fall back
// to the legacy writable runtime types for process actions.
void (undefined as unknown as UsageFetcher);
