import { classifyProviderFetchIssue } from "../lib/providerFetchIssue";
import { parseBalanceText } from "../lib/providerBalance";
import { STALE_AFTER_MS } from "../lib/quotaDisplay";
import type {
  CostSnapshotBridge,
  RateWindowSnapshot as BridgeRateWindow,
} from "../types/bridge";
import {
  formatWindowMinutes,
  sourceHealthFromDisplayState,
  telemetryFromWayfinder,
  type DisplayState,
  type ProviderCapability,
  type ProviderDisplayModel,
  type ProviderSnapshot,
  type RateWindowSnapshot,
  type SnapshotShape,
  type WindowDisplayKind,
  type WindowRole,
} from "./snapshot";

type BridgeWindowSlot = {
  id: string;
  label: string | null | undefined;
  window: BridgeRateWindow;
  usageKnown?: boolean;
  role: WindowRole;
  order: number;
};

export function fromBridge(
  model: ProviderDisplayModel,
  nowMs: number = Date.now(),
): ProviderSnapshot {
  const windows = collectWindows(model);
  applySharedTitleGroups(windows);

  const cost = resolveCost(model, windows);
  const telemetry = model.wayfinderUsage
    ? telemetryFromWayfinder(model.wayfinderUsage)
    : null;

  const realQuotaCount = windows.filter(
    (window) =>
      window.usageKnown &&
      !window.isInformational &&
      !window.isSynthetic &&
      window.displayKind === "quota",
  ).length;
  const hasQuota = realQuotaCount > 0;
  const hasBalance = cost != null;
  const hasTelemetry = telemetry != null;
  const hasExtraWindows = windows.some((window) => window.role === "additional");

  const inferred: ProviderCapability = {
    hasQuota,
    hasBalance,
    hasTelemetry,
    hasExtraWindows,
    supportsCharts: false,
    supportsLocalCost: false,
    supportsOutputSpeed: false,
    supportsProviderDashboard: false,
    supportsStatusPage: false,
    supportsLogin: false,
    snapshotShape: snapshotShape({
      realQuotaCount,
      hasQuota,
      hasBalance,
      hasTelemetry,
    }),
  };
  const capabilities: ProviderCapability = {
    ...inferred,
    ...model.capabilities,
    snapshotShape: model.capabilities?.snapshotShape ?? inferred.snapshotShape,
    hasQuota: model.capabilities?.hasQuota ?? inferred.hasQuota,
    hasBalance: model.capabilities?.hasBalance ?? inferred.hasBalance,
    hasTelemetry: model.capabilities?.hasTelemetry ?? inferred.hasTelemetry,
    hasExtraWindows: model.capabilities?.hasExtraWindows ?? inferred.hasExtraWindows,
  };

  const displayState = resolveDisplayState(model, nowMs, {
    hasQuota,
    hasBalance,
    hasTelemetry,
  });

  return {
    providerId: model.providerId,
    displayName: model.displayName,
    accountKey:
      model.accountKey ??
      model.accountEmail ??
      model.accountOrganization ??
      "default",
    sourceKey: model.sourceKey ?? model.sourceLabel ?? "default",
    planName: model.planName,
    accountEmail: model.accountEmail,
    accountOrganization: model.accountOrganization,
    sourceLabel: model.sourceLabel,
    sourceHealth: sourceHealthFromDisplayState(displayState),
    displayState,
    updatedAt: model.updatedAt || null,
    fetchedAt: model.fetchedAt ?? model.updatedAt ?? null,
    error: model.error,
    windows,
    cost,
    telemetry,
    pace: model.pace,
    capabilities,
    trayStatusLabel: model.trayStatusLabel,
  };
}

function collectWindows(model: ProviderDisplayModel): RateWindowSnapshot[] {
  const slots: BridgeWindowSlot[] = [
    {
      id: "primary",
      label: model.primaryLabel,
      window: model.primary,
      role: "primary",
      order: 0,
    },
  ];
  if (model.secondary) {
    slots.push({
      id: "secondary",
      label: model.secondaryLabel,
      window: model.secondary,
      role: "secondary",
      order: 1,
    });
  }
  if (model.modelSpecific) {
    slots.push({
      id: "modelSpecific",
      label: null,
      window: model.modelSpecific,
      role: "additional",
      order: 2,
    });
  }
  if (model.tertiary) {
    slots.push({
      id: "tertiary",
      label: null,
      window: model.tertiary,
      role: "additional",
      order: 3,
    });
  }
  model.extraRateWindows.forEach((extra, index) => {
    slots.push({
      id: extra.id,
      label: extra.title,
      window: extra.window,
      usageKnown: extra.usageKnown,
      role: "additional",
      order: 4 + index,
    });
  });
  return slots.map(adaptWindow);
}

function adaptWindow(slot: BridgeWindowSlot): RateWindowSnapshot {
  const { window } = slot;
  const usedPercent = Number.isFinite(window.usedPercent) ? window.usedPercent : 0;
  const remainingPercent = Number.isFinite(window.remainingPercent)
    ? window.remainingPercent
    : 100 - usedPercent;
  const balanceCarrier = isBalanceCarrier(window);
  const usageKnown = resolveUsageKnown(slot, balanceCarrier);
  const displayKind = resolveDisplayKind(window, usageKnown, balanceCarrier);
  const isSynthetic =
    balanceCarrier ||
    (window.isInformational === true && !slot.label?.trim() && !window.resetDescription?.trim());

  return {
    id: slot.id,
    label: resolveLabel(slot),
    usedPercent,
    remainingPercent,
    kind: window.kind,
    windowMinutes: window.windowMinutes,
    resetsAt: window.resetsAt,
    resetDescription: window.resetDescription,
    isExhausted: window.isExhausted,
    isInformational: Boolean(window.isInformational) || !usageKnown || displayKind !== "quota",
    usageKnown,
    displayKind,
    role: slot.role,
    order: slot.order,
    resetKnown: Boolean(window.resetsAt || window.resetDescription?.trim()),
    isSynthetic,
    reservePercent: window.reservePercent,
    reserveDescription: window.reserveDescription,
    reserveWillLastToReset: window.reserveWillLastToReset,
    reserveEtaSeconds: window.reserveEtaSeconds,
  };
}

function resolveUsageKnown(slot: BridgeWindowSlot, balanceCarrier: boolean): boolean {
  if (slot.usageKnown === false) return false;
  if (balanceCarrier) return false;
  if (slot.window.isInformational) return false;
  if (slot.usageKnown === true) return true;
  return true;
}

function resolveDisplayKind(
  window: BridgeRateWindow,
  usageKnown: boolean,
  balanceCarrier: boolean,
): WindowDisplayKind {
  if (balanceCarrier) return "balance";
  if (!usageKnown || window.isInformational) return "informational";
  return "quota";
}

function resolveLabel(slot: BridgeWindowSlot): string {
  const explicit = slot.label?.trim();
  if (explicit) return explicit;
  const minutes = formatWindowMinutes(slot.window.windowMinutes);
  if (slot.window.kind === "session") return minutes ?? "session";
  if (slot.window.kind === "daily") return "daily";
  if (slot.window.kind === "weekly") return "weekly";
  if (slot.window.kind === "monthly") return "monthly";
  return minutes ?? slot.id;
}

function isBalanceCarrier(window: BridgeRateWindow): boolean {
  return parseBalanceText(window.resetDescription) != null;
}

function resolveCost(
  model: ProviderDisplayModel,
  windows: RateWindowSnapshot[],
): CostSnapshotBridge | null {
  if (model.cost) return model.cost;
  for (const window of windows) {
    const parsed = parseBalanceText(window.resetDescription);
    if (!parsed || parsed.kind !== "balance" || parsed.unavailable) continue;
    const amount = parseLeadingAmount(parsed.amount);
    return {
      used: amount ?? 0,
      limit: null,
      remaining: amount,
      currencyCode: inferCurrencyCode(parsed.amount),
      period: "",
      resetsAt: window.resetsAt,
      formattedUsed: parsed.amount,
      formattedLimit: null,
    };
  }
  return null;
}

function parseLeadingAmount(text: string): number | null {
  const match = text.replace(/,/g, "").match(/-?\d+(?:\.\d+)?/);
  if (!match) return null;
  const value = Number(match[0]);
  return Number.isFinite(value) ? value : null;
}

function inferCurrencyCode(text: string): string {
  if (/[¥￥]|CNY/i.test(text)) return "CNY";
  if (/\$|USD/i.test(text)) return "USD";
  return "";
}

function applySharedTitleGroups(windows: RateWindowSnapshot[]): void {
  const prefixOf = (label: string): string | null => {
    const match = label.trim().match(/^([A-Za-z][A-Za-z0-9+._-]*)\s+/);
    return match?.[1] ?? null;
  };
  const counts = new Map<string, number>();
  for (const window of windows) {
    const prefix = prefixOf(window.label);
    if (prefix) counts.set(prefix, (counts.get(prefix) ?? 0) + 1);
  }
  for (const window of windows) {
    const prefix = prefixOf(window.label);
    if (prefix && (counts.get(prefix) ?? 0) >= 2) {
      window.groupId = prefix.toLowerCase();
      window.groupTitle = prefix;
    }
  }
}

function snapshotShape(input: {
  realQuotaCount: number;
  hasQuota: boolean;
  hasBalance: boolean;
  hasTelemetry: boolean;
}): SnapshotShape {
  if (input.realQuotaCount >= 4) return "overflow";
  if (input.hasQuota && (input.hasBalance || input.hasTelemetry)) return "mixed";
  if (input.hasTelemetry && !input.hasQuota) return "telemetry";
  return "standard";
}

function resolveDisplayState(
  model: ProviderDisplayModel,
  nowMs: number,
  presence: { hasQuota: boolean; hasBalance: boolean; hasTelemetry: boolean },
): DisplayState {
  if (model.displayState) return model.displayState;

  const error = model.error?.trim() ?? "";
  if (error) {
    if (/not configured|not enabled|no provider configured/i.test(error)) {
      return "notConfigured";
    }
    if (/unsupported|not supported/i.test(error)) return "unsupported";
    if (classifyProviderFetchIssue(error).category === "auth") return "authRequired";
    return "error";
  }

  if (!model.updatedAt) return "loading";
  const updatedMs = Date.parse(model.updatedAt);
  if (!Number.isFinite(updatedMs)) return "loading";
  if (nowMs - updatedMs > STALE_AFTER_MS) return "stale";

  if (!presence.hasQuota && !presence.hasBalance && !presence.hasTelemetry) {
    return "unknown";
  }
  return "ready";
}
