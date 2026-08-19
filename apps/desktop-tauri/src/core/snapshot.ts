import type {
  CostSnapshotBridge,
  PaceSnapshot,
  ProviderUsageSnapshot,
  QuotaCycleKind,
  WayfinderUsageSnapshot,
} from "../types/bridge";

/** Lifecycle shown on every surface. Unknown usage is a state, never a 100% bar. */
export type DisplayState =
  | "loading"
  | "ready"
  | "refreshing"
  | "stale"
  | "error"
  | "notConfigured"
  | "authRequired"
  | "unsupported"
  | "unknown";

export type SourceHealth =
  | "fresh"
  | "stale"
  | "error"
  | "notConfigured"
  | "unknown";

export type WindowDisplayKind = "quota" | "balance" | "informational" | "telemetry";

export type WindowRole = "primary" | "secondary" | "additional" | "summary";

export type SnapshotShape =
  | "standard"
  | "mixed"
  | "telemetry"
  | "polymorphic"
  | "overflow";

export interface ProviderCapability {
  hasQuota: boolean;
  hasBalance: boolean;
  hasTelemetry: boolean;
  hasExtraWindows: boolean;
  supportsCharts: boolean;
  supportsLocalCost: boolean;
  supportsOutputSpeed: boolean;
  supportsProviderDashboard: boolean;
  supportsStatusPage: boolean;
  supportsLogin: boolean;
  snapshotShape: SnapshotShape;
}

/**
 * One published window. `usageKnown: false` keeps raw percents for diagnostics
 * but must never be rendered as 1.0 / 100%.
 */
export interface RateWindowSnapshot {
  id: string;
  label: string;
  usedPercent: number;
  remainingPercent: number;
  kind: QuotaCycleKind | null;
  windowMinutes: number | null;
  resetsAt: string | null;
  resetDescription: string | null;
  isExhausted: boolean;
  isInformational: boolean;
  usageKnown: boolean;
  displayKind: WindowDisplayKind;
  role: WindowRole;
  order: number;
  resetKnown: boolean;
  isSynthetic: boolean;
  groupId?: string;
  groupTitle?: string;
  reservePercent: number | null;
  reserveDescription: string | null;
  reserveWillLastToReset?: boolean;
  reserveEtaSeconds?: number | null;
}

export interface TelemetrySnapshot {
  gatewayStatus: string;
  offline: boolean;
  dryRun: boolean;
  missingKeys: string[];
  modelCount: number;
  models: string[];
  requests: number;
  estimatedRequests: number;
  tokens: number;
  realized: number;
  baseline: number;
  saved: number;
  savedPercent: number;
  periodDays: number;
  unit: string;
  priced: boolean;
}

export interface ProviderSnapshot {
  providerId: string;
  displayName: string;
  accountKey: string;
  sourceKey: string;
  planName: string | null;
  accountEmail: string | null;
  accountOrganization: string | null;
  sourceLabel: string;
  sourceHealth: SourceHealth;
  displayState: DisplayState;
  updatedAt: string | null;
  fetchedAt: string | null;
  error: string | null;
  windows: RateWindowSnapshot[];
  cost: CostSnapshotBridge | null;
  telemetry: TelemetrySnapshot | null;
  pace: PaceSnapshot | null;
  capabilities: ProviderCapability;
  trayStatusLabel: string | null;
}

/**
 * Current frontend-facing usage model. Structurally the bridge snapshot plus
 * optional core hints; pages must not treat this as a projection.
 */
export type ProviderDisplayModel = ProviderUsageSnapshot & {
  accountKey?: string;
  sourceKey?: string;
  fetchedAt?: string | null;
  displayState?: DisplayState;
  capabilities?: Partial<ProviderCapability>;
};

export function sourceHealthFromDisplayState(state: DisplayState): SourceHealth {
  switch (state) {
    case "ready":
    case "refreshing":
      return "fresh";
    case "stale":
      return "stale";
    case "error":
    case "authRequired":
      return "error";
    case "notConfigured":
      return "notConfigured";
    default:
      return "unknown";
  }
}

export function sourceHealth(
  snapshot: Pick<ProviderSnapshot, "sourceHealth" | "displayState">,
): SourceHealth {
  return snapshot.sourceHealth ?? sourceHealthFromDisplayState(snapshot.displayState);
}

export function isRealQuotaWindow(window: RateWindowSnapshot): boolean {
  return (
    window.usageKnown &&
    !window.isInformational &&
    !window.isSynthetic &&
    window.displayKind === "quota"
  );
}

/**
 * Bar fill for a window. Returns null when a surface must not draw a quota bar
 * — including unknown usage, even when remainingPercent is 100.
 */
export function quotaFillPercent(
  window: RateWindowSnapshot,
  showAsUsed = true,
): number | null {
  if (!isRealQuotaWindow(window) || !Number.isFinite(window.usedPercent)) {
    return null;
  }
  const used = Math.min(100, Math.max(0, window.usedPercent));
  return showAsUsed ? used : 100 - used;
}

export function formatCycleTag(
  window: Pick<RateWindowSnapshot, "kind" | "windowMinutes" | "label">,
): string {
  const minutes = window.windowMinutes;
  if (window.kind === "session" || isShortTimeWindow(window)) {
    return formatWindowMinutes(minutes) ?? "时";
  }
  if (window.kind === "daily") return "日";
  if (window.kind === "weekly") return "周";
  if (window.kind === "monthly") return "月";
  const label = window.label?.trim();
  if (label && label.length > 0 && label.length <= 6) return label;
  return formatWindowMinutes(minutes) ?? "额";
}

export function isShortTimeWindow(
  window: Pick<RateWindowSnapshot, "kind" | "windowMinutes">,
): boolean {
  return (
    window.kind === "session" ||
    (window.kind == null &&
      window.windowMinutes != null &&
      window.windowMinutes < 24 * 60)
  );
}

export function formatWindowMinutes(minutes: number | null | undefined): string | null {
  if (minutes == null || !Number.isFinite(minutes) || minutes <= 0) return null;
  if (minutes < 60) return `${Math.round(minutes)}m`;
  if (minutes % 60 === 0 && minutes < 24 * 60) return `${minutes / 60}h`;
  if (minutes === 24 * 60) return "日";
  if (minutes === 7 * 24 * 60) return "周";
  if (minutes % (24 * 60) === 0) return `${minutes / (24 * 60)}d`;
  return `${Math.round(minutes / 60)}h`;
}

export function telemetryFromWayfinder(
  usage: WayfinderUsageSnapshot,
): TelemetrySnapshot {
  return {
    gatewayStatus: usage.gatewayStatus,
    offline: usage.offline,
    dryRun: usage.dryRun,
    missingKeys: usage.missingKeys,
    modelCount: usage.modelCount,
    models: usage.models,
    requests: usage.requests,
    estimatedRequests: usage.estimatedRequests,
    tokens: usage.tokens,
    realized: usage.realized,
    baseline: usage.baseline,
    saved: usage.saved,
    savedPercent: usage.savedPercent,
    periodDays: usage.periodDays,
    unit: usage.unit,
    priced: usage.priced,
  };
}
