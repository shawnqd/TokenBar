import { getProviderIcon } from "../components/providers/providerIcons";
import {
  TASKBAR_PROVIDER_AUTO,
  type TaskbarEntry,
  type TaskbarWindowKind,
} from "../types/bridge";
import {
  formatCycleTag,
  isRealQuotaWindow,
  isShortTimeWindow,
  quotaFillPercent,
  type DisplayState,
  type ProviderCapability,
  type ProviderSnapshot,
  type RateWindowSnapshot,
  type SourceHealth,
  type WindowDisplayKind,
} from "./snapshot";

export type SurfaceArchetype = "quota" | "balance" | "hybrid";

/** Visible quota slots: hero + secondary + two tiles. Further windows overflow. */
export const VISIBLE_QUOTA_WINDOW_LIMIT = 4;
export const MAX_VISIBLE_TASKBAR_CELLS = 4;

export interface ProjectedWindow {
  id: string;
  label: string;
  kind: RateWindowSnapshot["kind"];
  windowMinutes: number | null;
  usedPercent: number;
  remainingPercent: number;
  resetsAt: string | null;
  resetDescription: string | null;
  usageKnown: boolean;
  isInformational: boolean;
  isExhausted: boolean;
  displayKind: WindowDisplayKind;
  groupId?: string;
  groupTitle?: string;
  fullWidth?: boolean;
  /** Null when a surface must not draw a quota bar. */
  fillPercent: number | null;
}

export interface ProjectedBalance {
  amountText: string;
  subText: string | null;
  currencyCode: string | null;
  used: number | null;
  remaining: number | null;
  limit: number | null;
}

export interface ProjectedTelemetry {
  headline: string;
  gatewayStatus: string;
  modelCount: number;
  requests: number;
  tokens: number;
  savedPercent: number;
}

export interface TaskbarStripIcon {
  providerId: string;
  assetId: string;
  brandColor: string;
  fallbackGlyph: string | null;
}

export interface TaskbarStripCell {
  providerId: string;
  window: TaskbarWindowKind;
  icon: TaskbarStripIcon | null;
  tag: string;
  value: string;
  state: DisplayState;
  reason: string | null;
}

export interface SurfaceLayers {
  quota: ProjectedWindow[];
  balance: ProjectedBalance | null;
  telemetry: ProjectedTelemetry | null;
}

export interface SurfaceProjection {
  providerId: string;
  displayName: string;
  displayState: DisplayState;
  sourceHealth: SourceHealth;
  archetype: SurfaceArchetype;
  updatedAt: string | null;
  fetchedAt: string | null;
  error: string | null;
  primary: ProjectedWindow | null;
  secondary: ProjectedWindow | null;
  extras: ProjectedWindow[];
  overflow: ProjectedWindow[];
  balance: ProjectedBalance | null;
  telemetry: ProjectedTelemetry | null;
  layers: SurfaceLayers;
  capabilities: ProviderCapability;
  taskbarCells: TaskbarStripCell[];
}

export interface ProjectSurfaceOptions {
  showAsUsed?: boolean;
  taskbarEntries?: TaskbarEntry[];
  resolveIcon?: (providerId: string) => TaskbarStripIcon | null;
}

export function projectSurface(
  snapshot: ProviderSnapshot,
  options: ProjectSurfaceOptions = {},
): SurfaceProjection {
  const showAsUsed = options.showAsUsed ?? true;
  const sorted = sortQuotaWindows(snapshot.windows.filter(isRealQuotaWindow));
  const projected = sorted.map((window) => toProjectedWindow(window, showAsUsed));

  const primary = projected[0] ?? null;
  const secondary = projected[1] ?? null;
  const rest = projected.slice(2);
  const extraLimit = Math.max(0, VISIBLE_QUOTA_WINDOW_LIMIT - 2);
  const extras = rest.slice(0, extraLimit).map((window, index, list) => ({
    ...window,
    fullWidth: list.length % 2 === 1 && index === list.length - 1,
  }));
  const overflow = rest.slice(extraLimit);

  const balance = projectBalance(snapshot);
  const telemetry = projectTelemetry(snapshot);
  const archetype = resolveArchetype(projected.length > 0, balance != null);

  const layers: SurfaceLayers = {
    quota: projected,
    balance,
    telemetry,
  };

  return {
    providerId: snapshot.providerId,
    displayName: snapshot.displayName,
    displayState: snapshot.displayState,
    sourceHealth: snapshot.sourceHealth,
    archetype,
    updatedAt: snapshot.updatedAt,
    fetchedAt: snapshot.fetchedAt,
    error: snapshot.error,
    primary,
    secondary,
    extras,
    overflow,
    balance,
    telemetry,
    layers,
    capabilities: snapshot.capabilities,
    taskbarCells: projectTaskbarCells(snapshot, {
      showAsUsed,
      entries: options.taskbarEntries,
      resolveIcon: options.resolveIcon ?? defaultResolveIcon,
      hero: primary,
      sorted,
      balance,
    }),
  };
}

export function sortQuotaWindows(windows: RateWindowSnapshot[]): RateWindowSnapshot[] {
  // Multi-family providers (Antigravity's Gemini / Claude-GPT pairs) publish
  // their windows already grouped: family-major, five-hour before weekly. A
  // global cycle sort interleaves the families (all 5h rows, then all weekly
  // rows) and destroys the pair layout. When every window carries a shared
  // title groupId, keep families adjacent — group first-appearance order,
  // cycle sort within each group. Mixed providers keep the global cycle sort.
  const hasGroups =
    windows.length > 1 && windows.some((window) => window.groupId != null);
  const groupOrder = new Map<string, number>();
  if (hasGroups) {
    for (const window of windows) {
      const id = window.groupId ?? `__ungrouped_${window.id}__`;
      if (!groupOrder.has(id)) groupOrder.set(id, groupOrder.size);
    }
  }
  return windows
    .map((window, index) => ({ window, index }))
    .sort((a, b) => {
      if (hasGroups) {
        const groupA = groupOrder.get(a.window.groupId ?? `__ungrouped_${a.window.id}__`) as number;
        const groupB = groupOrder.get(b.window.groupId ?? `__ungrouped_${b.window.id}__`) as number;
        if (groupA !== groupB) return groupA - groupB;
      }
      const rank = windowSortRank(a.window) - windowSortRank(b.window);
      if (rank !== 0) return rank;
      const minutesA = a.window.windowMinutes ?? Number.POSITIVE_INFINITY;
      const minutesB = b.window.windowMinutes ?? Number.POSITIVE_INFINITY;
      if (minutesA !== minutesB) return minutesA - minutesB;
      return a.window.order - b.window.order || a.index - b.index;
    })
    .map((entry) => entry.window);
}

function windowSortRank(window: RateWindowSnapshot): number {
  if (isShortTimeWindow(window)) return 0;
  if (window.kind === "daily") return 1;
  if (window.kind === "weekly") return 2;
  if (window.kind === "monthly") return 3;
  return 4;
}

function toProjectedWindow(
  window: RateWindowSnapshot,
  showAsUsed: boolean,
): ProjectedWindow {
  return {
    id: window.id,
    label: window.label,
    kind: window.kind,
    windowMinutes: window.windowMinutes,
    usedPercent: window.usedPercent,
    remainingPercent: window.remainingPercent,
    resetsAt: window.resetsAt,
    resetDescription: window.resetDescription,
    usageKnown: window.usageKnown,
    isInformational: window.isInformational,
    isExhausted: window.isExhausted,
    displayKind: window.displayKind,
    groupId: window.groupId,
    groupTitle: window.groupTitle,
    fillPercent: quotaFillPercent(window, showAsUsed),
  };
}

function resolveArchetype(hasQuota: boolean, hasBalance: boolean): SurfaceArchetype {
  if (hasQuota && hasBalance) return "hybrid";
  if (hasQuota) return "quota";
  return "balance";
}

function projectBalance(snapshot: ProviderSnapshot): ProjectedBalance | null {
  const cost = snapshot.cost;
  if (!cost) return null;
  return {
    amountText: cost.formattedUsed,
    subText: cost.formattedLimit ?? (cost.period ? cost.period : null),
    currencyCode: cost.currencyCode || null,
    used: Number.isFinite(cost.used) ? cost.used : null,
    remaining: cost.remaining,
    limit: cost.limit,
  };
}

function projectTelemetry(snapshot: ProviderSnapshot): ProjectedTelemetry | null {
  const telemetry = snapshot.telemetry;
  if (!telemetry) return null;
  return {
    headline: telemetry.gatewayStatus,
    gatewayStatus: telemetry.gatewayStatus,
    modelCount: telemetry.modelCount,
    requests: telemetry.requests,
    tokens: telemetry.tokens,
    savedPercent: telemetry.savedPercent,
  };
}

function projectTaskbarCells(
  snapshot: ProviderSnapshot,
  ctx: {
    showAsUsed: boolean;
    entries: TaskbarEntry[] | undefined;
    resolveIcon: (providerId: string) => TaskbarStripIcon | null;
    hero: ProjectedWindow | null;
    sorted: RateWindowSnapshot[];
    balance: ProjectedBalance | null;
  },
): TaskbarStripCell[] {
  const entries = (ctx.entries ?? defaultTaskbarEntries(snapshot)).slice(
    0,
    MAX_VISIBLE_TASKBAR_CELLS,
  );
  return entries.map((entry) => {
    const providerId =
      entry.providerId === TASKBAR_PROVIDER_AUTO
        ? snapshot.providerId
        : entry.providerId;
    return cellForEntry(snapshot, entry.window, {
      providerId,
      showAsUsed: ctx.showAsUsed,
      resolveIcon: ctx.resolveIcon,
      hero: ctx.hero,
      sorted: ctx.sorted,
      balance: ctx.balance,
    });
  });
}

function defaultTaskbarEntries(snapshot: ProviderSnapshot): TaskbarEntry[] {
  const entries: TaskbarEntry[] = [];
  if (snapshot.capabilities.hasQuota) {
    entries.push({ providerId: snapshot.providerId, window: "primary" });
  }
  if (snapshot.capabilities.hasBalance) {
    entries.push({ providerId: snapshot.providerId, window: "balance" });
  }
  if (entries.length === 0) {
    entries.push({ providerId: snapshot.providerId, window: "primary" });
  }
  return entries;
}

function cellForEntry(
  snapshot: ProviderSnapshot,
  windowKind: TaskbarWindowKind,
  ctx: {
    providerId: string;
    showAsUsed: boolean;
    resolveIcon: (providerId: string) => TaskbarStripIcon | null;
    hero: ProjectedWindow | null;
    sorted: RateWindowSnapshot[];
    balance: ProjectedBalance | null;
  },
): TaskbarStripCell {
  const icon = ctx.resolveIcon(ctx.providerId);
  const base = {
    providerId: ctx.providerId,
    window: windowKind,
    icon,
  };

  if (windowKind === "balance") {
    if (!ctx.balance) {
      return {
        ...base,
        tag: "余",
        value: "",
        state: "unsupported",
        reason: "no-balance",
      };
    }
    return {
      ...base,
      tag: "余",
      value: ctx.balance.amountText,
      state: snapshot.displayState,
      reason: null,
    };
  }

  if (windowKind === "speed") {
    return {
      ...base,
      tag: "速",
      value: "",
      state: snapshot.capabilities.supportsOutputSpeed ? "unknown" : "unsupported",
      reason: snapshot.capabilities.supportsOutputSpeed ? "no-speed-sample" : "no-speed",
    };
  }

  const window = selectTaskbarWindow(windowKind, ctx.hero, ctx.sorted);
  if (!window) {
    return {
      ...base,
      tag: tagForKind(windowKind),
      value: "",
      state: "unsupported",
      reason: "no-window",
    };
  }

  const state = window.usageKnown ? snapshot.displayState : "unknown";
  return {
    ...base,
    tag: formatCycleTag(window),
    value: formatQuotaValue(window, ctx.showAsUsed),
    state,
    reason: window.usageKnown ? null : "usage-unknown",
  };
}

function selectTaskbarWindow(
  windowKind: TaskbarWindowKind,
  hero: ProjectedWindow | null,
  sorted: RateWindowSnapshot[],
): RateWindowSnapshot | ProjectedWindow | null {
  if (windowKind === "primary") {
    return sorted[0] ?? hero;
  }
  return sorted.find((window) => window.kind === windowKind) ?? null;
}

function tagForKind(windowKind: TaskbarWindowKind): string {
  if (windowKind === "session") return "时";
  if (windowKind === "daily") return "日";
  if (windowKind === "weekly") return "周";
  if (windowKind === "monthly") return "月";
  if (windowKind === "balance") return "余";
  if (windowKind === "speed") return "速";
  return "额";
}

function formatQuotaValue(
  window: Pick<RateWindowSnapshot, "usageKnown" | "usedPercent" | "remainingPercent">,
  showAsUsed: boolean,
): string {
  if (!window.usageKnown) return "";
  const percent = showAsUsed ? window.usedPercent : window.remainingPercent;
  if (!Number.isFinite(percent)) return "";
  const clamped = Math.min(100, Math.max(0, percent));
  return `${Number(clamped.toFixed(1))}%`;
}

function defaultResolveIcon(providerId: string): TaskbarStripIcon | null {
  const icon = getProviderIcon(providerId);
  return {
    providerId,
    assetId: icon.id,
    brandColor: icon.brandColor,
    fallbackGlyph: icon.fallbackLetter ?? null,
  };
}
