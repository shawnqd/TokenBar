import { useEffect, useMemo } from "react";
import { OUTPUT_SPEED_PROVIDER_IDS } from "../lib/outputSpeed";
import {
  trayCoreStore,
  useTrayCoreState,
  type TrayCoreStoreState,
} from "../surfaces/tray/trayCoreStore";
import type { OutputSpeedSnapshot } from "../types/bridge";

/**
 * Compose the bridge-shaped speed snapshot from per-provider enrichment
 * results committed into the tray core store.
 *
 * The bridge type declares all three provider keys, but the backend contract
 * only carries providers that actually have a reading; consumers must read
 * via `snapshot?.[providerId]`, which is what every call site does today.
 * Returns null when no provider has an enrichment result (no fake static
 * speed is ever produced).
 */
export function readOutputSpeedFromTray(
  trayState: TrayCoreStoreState,
): OutputSpeedSnapshot | null {
  const composed: Partial<OutputSpeedSnapshot> = {};
  let found = false;
  for (const providerId of OUTPUT_SPEED_PROVIDER_IDS) {
    const speed = trayCoreStore.latestOutputSpeedFor(providerId);
    if (speed) {
      composed[providerId] = speed;
      found = true;
    }
  }
  // `trayState` is a subscription dependency signal: the committed results are
  // read through the store singleton, which is why re-renders on version
  // change always observe fresh data here.
  void trayState;
  return found ? (composed as OutputSpeedSnapshot) : null;
}

/**
 * Subscribe to core-driven output-speed enrichment results.
 *
 * The signature stays compatible with the pre-core hook (an enabled flag plus
 * an interval that the core scheduler now owns — local polling is gone, so
 * the interval parameter is accepted and ignored). While enabled, the hook
 * asks the tray enrichment scheduler to run the output-speed runner for every
 * provider whose core record declares the capability; the runner commits its
 * result into the store and this hook re-reads it after the scheduler runs.
 */
export function useOutputSpeedSnapshot(
  enabled: boolean,
  intervalMs = 1000,
): OutputSpeedSnapshot | null {
  void intervalMs;
  const trayState = useTrayCoreState();
  const capableSignature = useMemo(
    () =>
      enabled
        ? trayCoreStore
            .speedCapableKeys()
            .map(
              (key) => `${key.providerId}:${key.accountKey}:${key.sourceKey}`,
            )
            .join("|")
        : "",
    // Re-derive only when new records (or a re-render cycle) change the set.
    // eslint-disable-next-line react-hooks/exhaustive-deps
    [enabled, trayState.version],
  );

  useEffect(() => {
    if (!enabled) return;
    const keys = trayCoreStore.speedCapableKeys();
    for (const key of keys) {
      void trayCoreStore
        .triggerEnrichment("outputSpeed", key, { manual: true })
        .catch(() => {});
    }
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [enabled, capableSignature]);

  return enabled ? readOutputSpeedFromTray(trayState) : null;
}