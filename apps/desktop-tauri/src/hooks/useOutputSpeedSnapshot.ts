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
 * The bridge type declares every provider key, but the backend contract only
 * carries providers that actually have a reading; consumers must read via
 * `snapshot?.[providerId]`, which is what every call site does today.
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
 * Subscribe to core-driven output-speed enrichment results. While enabled,
 * refresh at the shared output-speed TTL so a session that becomes available
 * after the first render is picked up without reopening the surface. The
 * scheduler remains the single gate for capability and freshness; the timer
 * only asks it to run and does not create a second read path.
 */
export function useOutputSpeedSnapshot(
  enabled: boolean,
  intervalMs = 1_000,
): OutputSpeedSnapshot | null {
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
    let refreshPending = false;
    const refresh = () => {
      if (refreshPending) return;
      refreshPending = true;
      const keys = trayCoreStore.speedCapableKeys();
      void Promise.all(
        keys.map((key) =>
          trayCoreStore.triggerEnrichment("outputSpeed", key).catch(() => false),
        ),
      ).finally(() => {
        refreshPending = false;
      });
    };
    refresh();
    const timer = window.setInterval(refresh, Math.max(1_000, intervalMs));
    return () => window.clearInterval(timer);
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [enabled, capableSignature, intervalMs]);

  return enabled ? readOutputSpeedFromTray(trayState) : null;
}
