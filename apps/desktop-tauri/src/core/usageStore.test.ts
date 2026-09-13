import { describe, expect, it, vi } from "vitest";
import {
  createUsageStore,
  keyFromSnapshot,
  type ProviderSnapshot,
} from "./index";
import {
  FIXTURE_SNAPSHOTS,
} from "./fixtures";

function deferred<T>() {
  let resolve!: (value: T) => void;
  let reject!: (reason?: unknown) => void;
  const promise = new Promise<T>((res, rej) => {
    resolve = res;
    reject = rej;
  });
  return { promise, resolve, reject };
}

describe("UsageStore", () => {
  it("exposes subscribe/getSnapshot for useSyncExternalStore", () => {
    const store = createUsageStore();
    const seen: number[] = [];
    const unsubscribe = store.subscribe(() => {
      seen.push(store.getSnapshot().version);
    });
    store.upsert(FIXTURE_SNAPSHOTS.kimiSessionWeekly);
    expect(store.getSnapshot()).toBe(store.getServerSnapshot());
    expect(seen.length).toBe(1);
    unsubscribe();
    store.upsert(FIXTURE_SNAPSHOTS.balanceOnly);
    expect(seen.length).toBe(1);
  });

  it("keys records by providerId+accountKey+sourceKey", () => {
    const store = createUsageStore();
    const first = store.upsert(FIXTURE_SNAPSHOTS.kimiSessionWeekly);
    const otherAccount: ProviderSnapshot = {
      ...FIXTURE_SNAPSHOTS.kimiSessionWeekly,
      accountKey: "other",
    };
    store.upsert(otherAccount);
    expect(store.get(keyFromSnapshot(first.snapshot!))?.snapshotVersion).toBe(1);
    expect(store.get(keyFromSnapshot(otherAccount))?.snapshotVersion).toBe(1);
    expect(Object.keys(store.getSnapshot().records)).toHaveLength(2);
  });

  it("merges concurrent refresh of the same key into one fetch", async () => {
    const pending = deferred<ProviderSnapshot>();
    const fetcher = vi.fn(() => pending.promise);
    const store = createUsageStore({ fetcher });
    const key = keyFromSnapshot(FIXTURE_SNAPSHOTS.kimiSessionWeekly);

    const first = store.refresh(key);
    const second = store.refresh(key);
    expect(second).toBe(first);
    expect(fetcher).toHaveBeenCalledTimes(1);

    pending.resolve(FIXTURE_SNAPSHOTS.kimiSessionWeekly);
    const [a, b] = await Promise.all([first, second]);
    expect(a.snapshotVersion).toBe(1);
    expect(b.snapshotVersion).toBe(1);
    expect(fetcher).toHaveBeenCalledTimes(1);
  });

  it("keeps last-good data when a later refresh fails", async () => {
    const store = createUsageStore({
      fetcher: async () => {
        throw new Error("timeout");
      },
    });
    const ready = store.upsert(FIXTURE_SNAPSHOTS.kimiSessionWeekly);
    expect(ready.lastGood?.displayState).toBe("ready");
    expect(ready.snapshotVersion).toBe(1);

    const failed = await store.refresh(keyFromSnapshot(FIXTURE_SNAPSHOTS.kimiSessionWeekly));
    expect(failed.displayState).toBe("stale");
    expect(failed.lastGood?.windows[0]?.kind).toBe("weekly");
    expect(failed.snapshot?.providerId).toBe("kimi-like");
    expect(failed.snapshotVersion).toBe(1);
    expect(failed.error).toBe("timeout");
  });

  it("marks the first in-flight fetch as loading and a repeat as refreshing", async () => {
    const pending = deferred<ProviderSnapshot>();
    const store = createUsageStore({ fetcher: () => pending.promise });
    const key = keyFromSnapshot(FIXTURE_SNAPSHOTS.overflowFourPlus);

    const loading = store.refresh(key);
    expect(store.get(key)?.displayState).toBe("loading");
    pending.resolve(FIXTURE_SNAPSHOTS.overflowFourPlus);
    await loading;

    const later = deferred<ProviderSnapshot>();
    store.setFetcher(() => later.promise);
    const refreshing = store.refresh(key);
    expect(store.get(key)?.displayState).toBe("refreshing");
    later.resolve(FIXTURE_SNAPSHOTS.overflowFourPlus);
    await refreshing;
  });

  it("does not let a stale in-flight fetch overwrite a newer upsert", async () => {
    const pending = deferred<ProviderSnapshot>();
    const store = createUsageStore({ fetcher: () => pending.promise });
    const key = keyFromSnapshot(FIXTURE_SNAPSHOTS.kimiSessionWeekly);
    const inflight = store.refresh(key);
    store.upsert({
      ...FIXTURE_SNAPSHOTS.kimiSessionWeekly,
      planName: "newer-upsert",
    });
    pending.resolve(FIXTURE_SNAPSHOTS.kimiSessionWeekly);
    await inflight;
    expect(store.get(key)?.snapshot?.planName).toBe("newer-upsert");
  });

  it("accepts an injected fetcher and never invents a quota on error", async () => {
    const store = createUsageStore({
      fetcher: async () => FIXTURE_SNAPSHOTS.error,
    });
    const record = await store.refresh(keyFromSnapshot(FIXTURE_SNAPSHOTS.error));
    expect(record.displayState).toBe("error");
    expect(record.lastGood).toBeNull();
    expect(record.snapshot?.windows.some((window) => window.remainingPercent === 56)).toBe(
      true,
    );
  });
});
