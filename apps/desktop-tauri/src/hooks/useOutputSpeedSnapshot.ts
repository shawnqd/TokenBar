import { useEffect, useState } from "react";
import { getOutputSpeedSnapshot } from "../lib/tauri";
import type { OutputSpeedSnapshot } from "../types/bridge";

export function useOutputSpeedSnapshot(
  enabled: boolean,
  intervalMs = 1000,
): OutputSpeedSnapshot | null {
  const [snapshot, setSnapshot] = useState<OutputSpeedSnapshot | null>(null);

  useEffect(() => {
    if (!enabled) {
      setSnapshot(null);
      return;
    }
    let active = true;
    const refresh = () => {
      void getOutputSpeedSnapshot()
        .then((next) => {
          if (active) setSnapshot(next);
        })
        .catch(() => {});
    };
    refresh();
    const timer = window.setInterval(refresh, intervalMs);
    return () => {
      active = false;
      window.clearInterval(timer);
    };
  }, [enabled, intervalMs]);

  return snapshot;
}
