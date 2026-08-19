import {
  sourceHealthFromDisplayState,
  type DisplayState,
  type ProviderSnapshot,
  type SourceHealth,
} from "./snapshot";

export interface UsageStoreKey {
  providerId: string;
  accountKey: string;
  sourceKey: string;
}

export interface UsageRecord {
  key: UsageStoreKey;
  snapshot: ProviderSnapshot | null;
  lastGood: ProviderSnapshot | null;
  snapshotVersion: number;
  displayState: DisplayState;
  sourceHealth: SourceHealth;
  error: string | null;
}

export interface UsageStoreState {
  version: number;
  records: Readonly<Record<string, UsageRecord>>;
}

export type UsageFetcher = (key: UsageStoreKey) => Promise<ProviderSnapshot>;

export function usageStoreKey(key: UsageStoreKey): string {
  return `${key.providerId}::${key.accountKey}::${key.sourceKey}`;
}

export function keyFromSnapshot(snapshot: ProviderSnapshot): UsageStoreKey {
  return {
    providerId: snapshot.providerId,
    accountKey: snapshot.accountKey,
    sourceKey: snapshot.sourceKey,
  };
}

export interface UsageStore {
  subscribe: (listener: () => void) => () => void;
  getSnapshot: () => UsageStoreState;
  getServerSnapshot: () => UsageStoreState;
  get: (key: UsageStoreKey) => UsageRecord | undefined;
  upsert: (snapshot: ProviderSnapshot) => UsageRecord;
  refresh: (key: UsageStoreKey) => Promise<UsageRecord>;
  setFetcher: (fetcher: UsageFetcher | undefined) => void;
}

export function createUsageStore(options: { fetcher?: UsageFetcher } = {}): UsageStore {
  let state: UsageStoreState = { version: 0, records: Object.freeze({}) };
  let fetcher = options.fetcher;
  const listeners = new Set<() => void>();
  const inflight = new Map<string, Promise<UsageRecord>>();
  const generations = new Map<string, number>();

  const subscribe = (listener: () => void): (() => void) => {
    listeners.add(listener);
    return () => {
      listeners.delete(listener);
    };
  };

  const getSnapshot = (): UsageStoreState => state;
  const getServerSnapshot = (): UsageStoreState => state;

  const get = (key: UsageStoreKey): UsageRecord | undefined =>
    state.records[usageStoreKey(key)];

  const setFetcher = (next: UsageFetcher | undefined): void => {
    fetcher = next;
  };

  const upsert = (snapshot: ProviderSnapshot): UsageRecord => {
    const key = keyFromSnapshot(snapshot);
    const id = usageStoreKey(key);
    generations.set(id, (generations.get(id) ?? 0) + 1);
    const previous = state.records[id];
    const record = writeRecord(key, previous, snapshot, snapshot.error);
    commit(id, record);
    return record;
  };

  const refresh = (key: UsageStoreKey): Promise<UsageRecord> => {
    const id = usageStoreKey(key);
    const pending = inflight.get(id);
    if (pending) return pending;

    const generation = (generations.get(id) ?? 0) + 1;
    generations.set(id, generation);

    const run = (async (): Promise<UsageRecord> => {
      markRefreshStart(key);
      if (!fetcher) {
        return commitFailure(key, generation, "UsageStore has no fetcher");
      }
      try {
        const snapshot = await fetcher(key);
        if (generations.get(id) !== generation) {
          return state.records[id] ?? emptyRecord(key);
        }
        return commitSuccess(key, generation, snapshot);
      } catch (error) {
        if (generations.get(id) !== generation) {
          return state.records[id] ?? emptyRecord(key);
        }
        return commitFailure(key, generation, messageOf(error));
      }
    })();

    const tracked = run.finally(() => {
      if (inflight.get(id) === tracked) inflight.delete(id);
    });
    inflight.set(id, tracked);
    return tracked;
  };

  function markRefreshStart(key: UsageStoreKey): void {
    const id = usageStoreKey(key);
    const previous = state.records[id];
    const displayState: DisplayState = previous?.lastGood ? "refreshing" : "loading";
    const record: UsageRecord = {
      key,
      snapshot: previous?.snapshot ?? null,
      lastGood: previous?.lastGood ?? null,
      snapshotVersion: previous?.snapshotVersion ?? 0,
      displayState,
      sourceHealth: sourceHealthFromDisplayState(displayState),
      error: previous?.error ?? null,
    };
    commit(id, record);
  }

  function commitSuccess(
    key: UsageStoreKey,
    generation: number,
    snapshot: ProviderSnapshot,
  ): UsageRecord {
    const id = usageStoreKey(key);
    if (generations.get(id) !== generation) {
      return state.records[id] ?? emptyRecord(key);
    }
    const previous = state.records[id];
    const record = writeRecord(key, previous, snapshot, snapshot.error);
    commit(id, record);
    return record;
  }

  function commitFailure(
    key: UsageStoreKey,
    generation: number,
    error: string,
  ): UsageRecord {
    const id = usageStoreKey(key);
    if (generations.get(id) !== generation) {
      return state.records[id] ?? emptyRecord(key);
    }
    const previous = state.records[id];
    const displayState: DisplayState = previous?.lastGood ? "stale" : "error";
    const record: UsageRecord = {
      key,
      snapshot: previous?.lastGood ?? previous?.snapshot ?? null,
      lastGood: previous?.lastGood ?? null,
      snapshotVersion: previous?.snapshotVersion ?? 0,
      displayState,
      sourceHealth: sourceHealthFromDisplayState(displayState),
      error,
    };
    commit(id, record);
    return record;
  }

  function commit(id: string, record: UsageRecord): void {
    state = {
      version: state.version + 1,
      records: Object.freeze({
        ...state.records,
        [id]: record,
      }),
    };
    listeners.forEach((listener) => listener());
  }

  return {
    subscribe,
    getSnapshot,
    getServerSnapshot,
    get,
    upsert,
    refresh,
    setFetcher,
  };
}

function writeRecord(
  key: UsageStoreKey,
  previous: UsageRecord | undefined,
  snapshot: ProviderSnapshot,
  error: string | null,
): UsageRecord {
  const usable = isUsableSnapshot(snapshot);
  const lastGood = usable ? snapshot : previous?.lastGood ?? null;
  const snapshotVersion = usable
    ? (previous?.snapshotVersion ?? 0) + 1
    : previous?.snapshotVersion ?? 0;
  return {
    key,
    snapshot,
    lastGood,
    snapshotVersion,
    displayState: snapshot.displayState,
    sourceHealth: snapshot.sourceHealth,
    error,
  };
}

function isUsableSnapshot(snapshot: ProviderSnapshot): boolean {
  if (snapshot.displayState === "error" || snapshot.displayState === "authRequired") {
    return false;
  }
  if (snapshot.displayState === "notConfigured" || snapshot.displayState === "unsupported") {
    return false;
  }
  return (
    snapshot.windows.length > 0 ||
    snapshot.cost != null ||
    snapshot.telemetry != null ||
    snapshot.displayState === "ready" ||
    snapshot.displayState === "stale"
  );
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

function messageOf(error: unknown): string {
  if (error instanceof Error && error.message) return error.message;
  return String(error);
}
