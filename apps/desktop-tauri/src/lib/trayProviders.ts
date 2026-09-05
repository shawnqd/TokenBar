/**
 * Tray provider slot helpers — read-only now.
 *
 * The tray flyout no longer sources provider data here: usage comes from the
 * unified core read model (`surfaces/tray/trayCoreStore`). What remains in
 * this module is pure helper/settings plumbing: ordering enabled providers for
 * the dense grid, a placeholder for enabled-but-unknown slots, hydrating the
 * slots from a provider map, and the one-way adapter that feeds legacy
 * bridge-shaped components (ProviderGrid, ordering, Settings preview) with a
 * view of a core snapshot. No function here fakes usage data or invents a
 * provider reading.
 */
import type {
  ProviderCatalogEntry,
  ProviderUsageSnapshot,
  RateWindowSnapshot,
} from "../types/bridge";
import type {
  ProviderCapability,
  ProviderSnapshot,
  RateWindowSnapshot as CoreRateWindow,
} from "../core";

export interface TrayProviderSlot {
  id: string;
  displayName: string;
}

const EMPTY_RATE_WINDOW: RateWindowSnapshot = {
  usedPercent: 0,
  remainingPercent: 100,
  // A placeholder for a provider that has reported nothing yet: no length
  // and no real data, so nothing may name it a real quota. Per CORE-03 the
  // unknown window stays informational (never fabricate 0%/100%).
  isInformational: true,
  kind: null,
  windowMinutes: null,
  resetsAt: null,
  resetDescription: null,
  isExhausted: false,
  reservePercent: null,
  reserveDescription: null,
};

export function orderedEnabledProviderSlots(
  catalog: ProviderCatalogEntry[],
  enabledProviderIds: string[],
  snapshots: ProviderUsageSnapshot[],
  providerOrder: string[] = [],
): TrayProviderSlot[] {
  const enabled = new Set(enabledProviderIds);
  const catalogById = new Map(catalog.map((provider) => [provider.id, provider]));
  const snapshotNames = new Map(
    snapshots.map((provider) => [provider.providerId, provider.displayName]),
  );
  const slots: TrayProviderSlot[] = [];
  const seen = new Set<string>();
  const orderedIds = providerOrder.length > 0
    ? providerOrder
    : catalog.map((provider) => provider.id);

  for (const providerId of orderedIds) {
    if (!enabled.has(providerId)) continue;
    seen.add(providerId);
    slots.push({
      id: providerId,
      displayName:
        catalogById.get(providerId)?.displayName ?? snapshotNames.get(providerId) ?? providerId,
    });
  }

  for (const providerId of enabledProviderIds) {
    if (seen.has(providerId)) continue;
    slots.push({
      id: providerId,
      displayName: snapshotNames.get(providerId) ?? providerId,
    });
  }

  return slots;
}

export function providerPlaceholder(
  providerId: string,
  displayName: string,
): ProviderUsageSnapshot {
  return {
    providerId,
    displayName,
    primary: { ...EMPTY_RATE_WINDOW },
    primaryLabel: "Usage",
    secondary: null,
    modelSpecific: null,
    tertiary: null,
    extraRateWindows: [],
    cost: null,
    planName: null,
    accountEmail: null,
    sourceLabel: "pending",
    updatedAt: new Date(0).toISOString(),
    error: "Loading provider data...",
    pace: null,
    accountOrganization: null,
    trayStatusLabel: null,
    fetchDurationMs: null,
  };
}

export function hydrateProviderSlots(
  slots: TrayProviderSlot[],
  providersById: Map<string, ProviderUsageSnapshot>,
): ProviderUsageSnapshot[] {
  return slots.map((slot) =>
    providersById.get(slot.id) ?? providerPlaceholder(slot.id, slot.displayName),
  );
}

/* ── Core → bridge compatibility view ────────────────────────────────── */

function toBridgeRateWindow(window: CoreRateWindow): RateWindowSnapshot {
  return {
    usedPercent: window.usedPercent,
    remainingPercent: window.remainingPercent,
    kind: window.kind,
    windowMinutes: window.windowMinutes,
    resetsAt: window.resetsAt,
    resetDescription: window.resetDescription,
    isExhausted: window.isExhausted,
    reservePercent: window.reservePercent,
    reserveDescription: window.reserveDescription,
  };
}

function bridgeCapabilities(caps: ProviderCapability): {
  outputSpeed: boolean;
  localUsage: boolean;
  providerDashboard: boolean;
  statusPage: boolean;
  login: boolean;
} {
  return {
    outputSpeed: caps.supportsOutputSpeed,
    localUsage: caps.supportsLocalCost || caps.supportsCharts,
    providerDashboard: caps.supportsProviderDashboard,
    statusPage: caps.supportsStatusPage,
    login: caps.supportsLogin,
  };
}

/**
 * One-way read-only adapter from a core snapshot to the legacy bridge shape.
 *
 * Used only to keep out-of-domain bridge consumers (ProviderGrid, ordering,
 * MenuCard previews) working; the quota cards render the projection of the
 * core snapshot instead. The adapter never copies credentials and never
 * invents usage: a missing window role simply falls back to an informational
 * empty primary window (the same "unknown" state the bridge used).
 */
export function coreSnapshotToBridge(
  snapshot: ProviderSnapshot,
): ProviderUsageSnapshot {
  const primaryWindow =
    snapshot.windows.find((window) => window.role === "primary") ??
    snapshot.windows[0] ??
    null;
  const secondaryWindow = snapshot.windows.find(
    (window) => window.role === "secondary",
  );
  const modelSpecificWindow = snapshot.windows.find(
    (window) => window.role === "additional" && window.order === 2,
  );
  const tertiaryWindow = snapshot.windows.find(
    (window) => window.role === "additional" && window.order === 3,
  );
  const extras = snapshot.windows
    .filter((window) => window.role === "additional" && window.order >= 4)
    .map((window) => ({
      id: window.id,
      title: window.label,
      window: toBridgeRateWindow(window),
      usageKnown: window.usageKnown || undefined,
      inventoryExpiresAt: window.inventoryExpiresAt,
    }));

  return {
    providerId: snapshot.providerId,
    displayName: snapshot.displayName,
    primary: primaryWindow ? toBridgeRateWindow(primaryWindow) : { ...EMPTY_RATE_WINDOW },
    primaryLabel: primaryWindow?.label,
    secondary: secondaryWindow ? toBridgeRateWindow(secondaryWindow) : null,
    secondaryLabel: secondaryWindow?.label,
    modelSpecific: modelSpecificWindow ? toBridgeRateWindow(modelSpecificWindow) : null,
    tertiary: tertiaryWindow ? toBridgeRateWindow(tertiaryWindow) : null,
    extraRateWindows: extras,
    cost: snapshot.cost,
    planName: snapshot.planName,
    accountEmail: snapshot.accountEmail,
    sourceLabel: snapshot.sourceLabel,
    updatedAt: snapshot.updatedAt ?? new Date(0).toISOString(),
    error: snapshot.error,
    pace: snapshot.pace,
    accountOrganization: snapshot.accountOrganization,
    trayStatusLabel: snapshot.trayStatusLabel,
    capabilities: bridgeCapabilities(snapshot.capabilities),
  };
}
