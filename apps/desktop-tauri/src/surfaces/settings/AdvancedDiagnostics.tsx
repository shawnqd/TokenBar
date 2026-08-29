import { useCallback, useEffect, useState } from "react";
import { getSafeDiagnostics } from "../../lib/tauri";
import {
  CORE_SCHEMA_VERSION,
  getLifecycleTrace,
  getRefreshTrace,
  type RuntimeTraceEvent,
} from "../../core/runtimeDiagnostics";
import type { SafeDiagnostics } from "../../types/bridge";

function formatTrace(events: RuntimeTraceEvent[]): string {
  if (events.length === 0) return "暂无记录";
  return events
    .slice(-12)
    .map((event) => {
      const time = new Date(event.at).toLocaleTimeString();
      const err = event.error ? ` error=${event.error}` : "";
      return `${time} ${event.kind} ${event.detail}${err}`;
    })
    .join("\n");
}

export function useAdvancedDiagnostics() {
  const [diagnostics, setDiagnostics] = useState<SafeDiagnostics | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [loading, setLoading] = useState(false);
  const [refreshTrace, setRefreshTrace] = useState("");
  const [lifecycleTrace, setLifecycleTrace] = useState("");

  const reload = useCallback(() => {
    setLoading(true);
    setError(null);
    setRefreshTrace(formatTrace(getRefreshTrace()));
    setLifecycleTrace(formatTrace(getLifecycleTrace()));
    void getSafeDiagnostics()
      .then((payload) => {
        setDiagnostics(payload);
      })
      .catch((cause) => {
        setDiagnostics(null);
        setError(cause instanceof Error ? cause.message : String(cause));
      })
      .finally(() => setLoading(false));
  }, []);

  useEffect(() => {
    reload();
  }, [reload]);

  const schemaVersion = diagnostics?.schemaVersion ?? CORE_SCHEMA_VERSION;
  const cacheSummary = diagnostics
    ? `${diagnostics.enabledProviders.length} providers · refresh ${diagnostics.refreshIntervalSecs}s · ${diagnostics.platform}`
    : loading
      ? "loading"
      : error ?? "unavailable";

  return {
    diagnostics,
    error,
    loading,
    refreshTrace,
    lifecycleTrace,
    schemaVersion,
    cacheSummary,
    reload,
  };
}
