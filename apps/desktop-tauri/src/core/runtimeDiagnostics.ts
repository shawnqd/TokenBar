import type { RefreshEvent } from "./refreshCoordinator";

export const CORE_SCHEMA_VERSION = 1;

export interface RuntimeTraceEvent {
  at: number;
  source: "refresh" | "lifecycle";
  kind: string;
  detail: string;
  error?: string;
}

const refreshTrace: RuntimeTraceEvent[] = [];
const lifecycleTrace: RuntimeTraceEvent[] = [];
const TRACE_CAP = 48;

function push(buf: RuntimeTraceEvent[], event: RuntimeTraceEvent): void {
  buf.push(event);
  if (buf.length > TRACE_CAP) buf.shift();
}

export function recordRefreshTrace(event: RefreshEvent): void {
  push(refreshTrace, {
    at: event.at,
    source: "refresh",
    kind: event.kind,
    detail: `${event.providerId} ${event.key.accountKey}/${event.key.sourceKey}`,
    error: event.error,
  });
}

export function recordLifecycleTrace(
  surface: string,
  phase: string,
  error?: string,
): void {
  push(lifecycleTrace, {
    at: Date.now(),
    source: "lifecycle",
    kind: phase,
    detail: surface,
    error,
  });
}

export function getRefreshTrace(): RuntimeTraceEvent[] {
  return [...refreshTrace];
}

export function getLifecycleTrace(): RuntimeTraceEvent[] {
  return [...lifecycleTrace];
}

export function clearRuntimeTracesForTest(): void {
  refreshTrace.length = 0;
  lifecycleTrace.length = 0;
}
