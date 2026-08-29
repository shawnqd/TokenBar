import {
  createEnrichmentScheduler,
  createRefreshCoordinator,
  createUsageStore,
  keyFromSnapshot,
  type ProviderCapability,
  type RefreshCoordinator,
} from "../../core";
import { attachTrayCoreRuntime, trayCoreStore } from "./trayCoreStore";

/** Isolated writable tray runtime for tests. Production never imports this file. */
export function resetTrayCoreForTest(): void {
  const store = createUsageStore();
  let coordinator: RefreshCoordinator | undefined;
  const scheduler = createEnrichmentScheduler({
    capabilities: () => {
      const map: Record<string, ProviderCapability> = {};
      if (!coordinator) return map;
      for (const record of Object.values(coordinator.getSnapshot().records)) {
        if (record.snapshot) {
          map[record.snapshot.providerId] = record.snapshot.capabilities;
        }
      }
      return map;
    },
    ttlMs: {},
    runner: async (kind, key) => {
      const runner = trayCoreStore.getEnrichmentRunner();
      if (!runner) throw new Error("tray: no enrichment runner wired");
      await runner(kind, key);
    },
  });
  coordinator = createRefreshCoordinator({
    store,
    fetcher: (key) => {
      const fetcher = trayCoreStore.getFetcher();
      if (!fetcher) {
        return Promise.reject(new Error("tray: no fetcher wired"));
      }
      return fetcher(key);
    },
    enrich: async (snapshot) => {
      const key = keyFromSnapshot(snapshot);
      await scheduler.trigger("outputSpeed", key, { manual: true }).catch(() => {});
      await scheduler.trigger("chart", key, { manual: true }).catch(() => {});
    },
  });
  attachTrayCoreRuntime({ store, coordinator, scheduler });
}
