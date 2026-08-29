import { createUsageStore } from "../core/usageStore";
import { fromBridge } from "../core/fromBridge";
import { floatBarStore } from "./floatBarStore";
import type { ProviderUsageSnapshot } from "../types/bridge";

export function __clearFloatBarStoreForTest(): void {
  floatBarStore.bind(createUsageStore());
}

export async function seedFloatBarFromCacheForTest(): Promise<void> {
  if (typeof window === "undefined") return;
  try {
    const { getCachedProviders } = await import("../lib/tauri");
    const snapshots = await getCachedProviders();
    if (!Array.isArray(snapshots)) return;
    for (const snapshot of snapshots as ProviderUsageSnapshot[]) {
      try {
        floatBarStore.upsert(fromBridge(snapshot));
      } catch {
        // A malformed fixture must not poison the test store.
      }
    }
  } catch {
    // Tests that unmount jsdom before this promise settles must not fail the suite.
  }
}
