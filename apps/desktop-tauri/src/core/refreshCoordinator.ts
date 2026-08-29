import {
  createUsageStore,
  usageStoreKey,
  type UsageFetcher,
  type UsageRecord,
  type UsageStore,
  type UsageStoreKey,
  type UsageStoreState,
} from "./usageStore";
import type { ProviderSnapshot } from "./snapshot";

export type { UsageFetcher, UsageRecord, UsageStoreKey, UsageStoreState };

export type EnrichmentRunner = (snapshot: ProviderSnapshot) => Promise<void>;

export type RefreshEventKind =
  | "started"
  | "core-complete"
  | "enrichment-complete"
  | "failed";

export interface RefreshEvent {
  kind: RefreshEventKind;
  key: UsageStoreKey;
  providerId: string;
  at: number;
  error?: string;
}

export interface RefreshCoordinatorOptions {
  store?: UsageStore;
  fetcher: UsageFetcher;
  enrich?: EnrichmentRunner;
  ttlMs?: number;
  timeoutMs?: number;
  maxRetries?: number;
  now?: () => number;
}

export interface RefreshCoordinator {
  getSnapshot: () => UsageStoreState;
  getServerSnapshot: () => UsageStoreState;
  subscribe: (listener: () => void) => () => void;
  get: (key: UsageStoreKey) => UsageRecord | undefined;
  refresh: (
    key: UsageStoreKey,
    opts?: { manual?: boolean },
  ) => Promise<UsageRecord>;
  refreshAll: (keys: UsageStoreKey[]) => Promise<UsageRecord[]>;
  startScheduler: (
    intervalMs: number,
    keysProvider?: () => UsageStoreKey[],
  ) => void;
  stopScheduler: () => void;
  on: (
    kind: RefreshEventKind,
    handler: (e: RefreshEvent) => void,
  ) => () => void;
  off: (kind: RefreshEventKind, handler: (e: RefreshEvent) => void) => void;
  cancel: (key: UsageStoreKey) => void;
  destroy: () => void;
  getStore: () => UsageStore;
  getTrace: () => RefreshEvent[];
}

function withTimeout<T>(promise: Promise<T>, ms: number): Promise<T> {
  return new Promise<T>((resolve, reject) => {
    const timer = setTimeout(
      () => reject(new Error(`timeout after ${ms}ms`)),
      ms,
    );
    promise.then(
      (v) => {
        clearTimeout(timer);
        resolve(v);
      },
      (e) => {
        clearTimeout(timer);
        reject(e);
      },
    );
  });
}

async function fetchWithRetry(
  key: UsageStoreKey,
  fetcher: UsageFetcher,
  timeoutMs: number | undefined,
  maxRetries: number | undefined,
): Promise<ProviderSnapshot> {
  const retries = maxRetries ?? 0;
  let attempt = 0;
  while (true) {
    try {
      const raw = fetcher(key);
      const result =
        timeoutMs != null && timeoutMs > 0
          ? await withTimeout(raw, timeoutMs)
          : await raw;
      return result;
    } catch (e) {
      if (attempt >= retries) throw e;
      attempt += 1;
    }
  }
}

function createEmptyRecord(key: UsageStoreKey): UsageRecord {
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

export function createRefreshCoordinator(
  options: RefreshCoordinatorOptions,
): RefreshCoordinator {
  const store = options.store ?? createUsageStore();
  const ttlMs = options.ttlMs;
  const timeoutMs = options.timeoutMs;
  const maxRetries = options.maxRetries;
  const enrich = options.enrich;
  const nowFn = options.now ?? (() => Date.now());

  const inflight = new Map<string, Promise<UsageRecord>>();
  const generations = new Map<string, number>();
  const lastFetchedAt = new Map<string, number>();
  const failureOverlay = new Map<string, UsageRecord>();

  const eventListeners = new Map<
    RefreshEventKind,
    Set<(e: RefreshEvent) => void>
  >();

  let schedulerTimer: ReturnType<typeof setInterval> | null = null;
  let schedulerKeysProvider: (() => UsageStoreKey[]) | null = null;
  const trace: RefreshEvent[] = [];

  function emit(
    kind: RefreshEventKind,
    key: UsageStoreKey,
    extra?: { error?: string },
  ) {
    const ev: RefreshEvent = {
      kind,
      key,
      providerId: key.providerId,
      at: nowFn(),
      ...(extra?.error ? { error: extra.error } : {}),
    };
    trace.push(ev);
    if (trace.length > 48) trace.shift();
    const set = eventListeners.get(kind);
    if (!set || set.size === 0) return;
    for (const h of [...set]) {
      try {
        h(ev);
      } catch {
        // isolate handler errors
      }
    }
  }

  const subscribe = (listener: () => void): (() => void) =>
    store.subscribe(listener);

  const wrappedGet = (key: UsageStoreKey): UsageRecord | undefined => {
    const id = usageStoreKey(key);
    if (failureOverlay.has(id)) return failureOverlay.get(id);
    return store.get(key);
  };

  const wrappedGetSnapshot = (): UsageStoreState => {
    const base = store.getSnapshot();
    if (failureOverlay.size === 0) return base;
    const merged: Record<string, UsageRecord> = {
      ...(base.records as Record<string, UsageRecord>),
    };
    for (const [id, rec] of failureOverlay.entries()) {
      merged[id] = rec;
    }
    return {
      version: base.version + failureOverlay.size,
      records: Object.freeze(merged),
    };
  };

  const wrappedGetServerSnapshot = (): UsageStoreState => wrappedGetSnapshot();
  const getStore = (): UsageStore => store;

  const on = (
    kind: RefreshEventKind,
    handler: (e: RefreshEvent) => void,
  ): (() => void) => {
    let set = eventListeners.get(kind);
    if (!set) {
      set = new Set();
      eventListeners.set(kind, set);
    }
    set.add(handler);
    return () => {
      set!.delete(handler);
    };
  };

  const off = (
    kind: RefreshEventKind,
    handler: (e: RefreshEvent) => void,
  ): void => {
    eventListeners.get(kind)?.delete(handler);
  };

  const cancel = (key: UsageStoreKey): void => {
    const id = usageStoreKey(key);
    generations.set(id, (generations.get(id) ?? 0) + 1);
    inflight.delete(id);
  };

  const refresh = (
    key: UsageStoreKey,
    opts?: { manual?: boolean },
  ): Promise<UsageRecord> => {
    const id = usageStoreKey(key);
    const manual = Boolean(opts?.manual);
    const now = nowFn();

    if (!manual && ttlMs != null && ttlMs > 0 && lastFetchedAt.has(id)) {
      const last = lastFetchedAt.get(id)!;
      if (now - last < ttlMs) {
        const cached = wrappedGet(key);
        if (cached) return Promise.resolve(cached);
      }
    }

    const pending = inflight.get(id);
    if (pending) {
      return pending;
    }

    const generation = (generations.get(id) ?? 0) + 1;
    generations.set(id, generation);

    const run = (async (): Promise<UsageRecord> => {
      emit("started", key);

      let snapshot: ProviderSnapshot | null = null;
      try {
        snapshot = await fetchWithRetry(
          key,
          options.fetcher,
          timeoutMs,
          maxRetries,
        );
      } catch (error) {
        if (generations.get(id) !== generation) {
          return wrappedGet(key) ?? createEmptyRecord(key);
        }
        const msg =
          error instanceof Error ? error.message : String(error);
        const previous = store.get(key) ?? failureOverlay.get(id);
        if (previous) {
          const overlay: UsageRecord = {
            key,
            snapshot: previous.lastGood ?? previous.snapshot,
            lastGood: previous.lastGood,
            snapshotVersion: previous.snapshotVersion,
            displayState: previous.lastGood ? "stale" : "error",
            sourceHealth: previous.lastGood ? "stale" : "error",
            error: msg,
          };
          failureOverlay.set(id, overlay);
          emit("failed", key, { error: msg });
          return overlay;
        }
        const emptyFailure: UsageRecord = {
          key,
          snapshot: null,
          lastGood: null,
          snapshotVersion: 0,
          displayState: "error",
          sourceHealth: "error",
          error: msg,
        };
        failureOverlay.set(id, emptyFailure);
        emit("failed", key, { error: msg });
        return emptyFailure;
      }

      if (generations.get(id) !== generation) {
        return wrappedGet(key) ?? createEmptyRecord(key);
      }

      lastFetchedAt.set(id, nowFn());
      failureOverlay.delete(id);
      const snapshotNonNull = snapshot!;
      const record = store.upsert(snapshotNonNull);
      emit("core-complete", key);

      if (enrich) {
        try {
          await enrich(snapshotNonNull);
          emit("enrichment-complete", key);
        } catch (e) {
          const emsg = e instanceof Error ? e.message : String(e);
          emit("failed", key, { error: emsg });
        }
      }

      return record;
    })();

    const tracked = run.finally(() => {
      if (inflight.get(id) === tracked) inflight.delete(id);
    });
    inflight.set(id, tracked);
    return tracked;
  };

  const refreshAll = async (
    keys: UsageStoreKey[],
  ): Promise<UsageRecord[]> => {
    const results = await Promise.allSettled(keys.map((k) => refresh(k)));
    return results.map((r, idx) => {
      if (r.status === "fulfilled") return r.value;
      const k = keys[idx];
      return wrappedGet(k) ?? createEmptyRecord(k);
    });
  };

  const startScheduler = (
    intervalMs: number,
    keysProvider?: () => UsageStoreKey[],
  ): void => {
    stopScheduler();
    schedulerKeysProvider = keysProvider ?? null;
    schedulerTimer = setInterval(() => {
      const keys = schedulerKeysProvider
        ? schedulerKeysProvider()
        : Object.values(wrappedGetSnapshot().records).map((r) => r.key);
      void refreshAll(keys);
    }, intervalMs);
    const maybeUnref = schedulerTimer as unknown as {
      unref?: () => void;
    };
    if (maybeUnref.unref) maybeUnref.unref();
  };

  const stopScheduler = (): void => {
    if (schedulerTimer != null) {
      clearInterval(schedulerTimer as unknown as number);
      schedulerTimer = null;
    }
    schedulerKeysProvider = null;
  };

  const destroy = (): void => {
    stopScheduler();
    inflight.clear();
    failureOverlay.clear();
    eventListeners.clear();
    trace.length = 0;
  };

  return {
    getSnapshot: wrappedGetSnapshot,
    getServerSnapshot: wrappedGetServerSnapshot,
    subscribe,
    get: wrappedGet,
    refresh,
    refreshAll,
    startScheduler,
    stopScheduler,
    on,
    off,
    cancel,
    destroy,
    getStore,
    getTrace: () => [...trace],
  };
}
