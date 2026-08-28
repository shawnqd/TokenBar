import { useMemo } from "react";
import FloatBar from "../../../floatbar/FloatBar";
import { TASKBAR_PROVIDER_AUTO } from "../../../types/bridge";
import type { SettingsSnapshot } from "../../../types/bridge";
import { resolveFloatBarEntries, expandFloatBarEntries } from "../floatBarEntries";
import { previewProviderList } from "./catalogFixtures";
import { useFloatBarSnapshots, ensureFloatBarStoreSync } from "../../../floatbar/floatBarStore";
import { fromBridge } from "../../../core/fromBridge";
import type { ProviderUsageSnapshot } from "../../../types/bridge";
import type { ProviderSnapshot } from "../../../core/snapshot";

function snapshotToBridge(snapshot: ProviderSnapshot): ProviderUsageSnapshot {
  // Minimal bridge shape for preview: primary window from first quota window
  const quota = snapshot.windows.find((w) => w.usageKnown && !w.isInformational && w.displayKind === "quota");
  const primary = quota
    ? {
        usedPercent: quota.usedPercent,
        remainingPercent: quota.remainingPercent,
        kind: quota.kind,
        windowMinutes: quota.windowMinutes,
        resetsAt: quota.resetsAt,
        resetDescription: quota.resetDescription,
        isExhausted: quota.isExhausted,
        reservePercent: quota.reservePercent,
        reserveDescription: quota.reserveDescription,
      }
    : {
        usedPercent: 0,
        remainingPercent: 100,
        kind: null,
        windowMinutes: null,
        resetsAt: null,
        resetDescription: null,
        isExhausted: false,
        reservePercent: null,
        reserveDescription: null,
      };
  return {
    providerId: snapshot.providerId,
    displayName: snapshot.displayName,
    primary: primary as any,
    secondary: null,
    modelSpecific: null,
    tertiary: null,
    extraRateWindows: [],
    cost: snapshot.cost as any,
    planName: snapshot.planName,
    accountEmail: snapshot.accountEmail,
    sourceLabel: snapshot.sourceLabel,
    updatedAt: snapshot.updatedAt ?? new Date().toISOString(),
    error: snapshot.error,
    pace: snapshot.pace as any,
    accountOrganization: snapshot.accountOrganization,
    trayStatusLabel: snapshot.trayStatusLabel,
    capabilities: {
      outputSpeed: snapshot.capabilities.supportsOutputSpeed,
      localUsage: snapshot.capabilities.supportsLocalCost,
      providerDashboard: snapshot.capabilities.supportsProviderDashboard,
      statusPage: snapshot.capabilities.supportsStatusPage,
      login: snapshot.capabilities.supportsLogin,
    } as any,
  };
}

interface Props {
  settings: SettingsSnapshot;
}

export default function FloatBarPreview({ settings }: Props) {
  // Ensure store sync is started for preview as well (shares same singleton)
  ensureFloatBarStoreSync();
  const coreSnapshots = useFloatBarSnapshots();
  // Build a bridge list for previewProviderList helper: prefer core snapshots converted to bridge,
  // fallback to catalog fixtures when store empty.
  const liveBridges: ProviderUsageSnapshot[] = useMemo(() => {
    if (coreSnapshots.length > 0) {
      return coreSnapshots.map(snapshotToBridge);
    }
    return [];
  }, [coreSnapshots]);

  const catalog = useMemo(() => {
    // previewProviderList expects bridge snapshots; it returns live if non-empty else catalog fixtures
    return previewProviderList(liveBridges);
  }, [liveBridges]);

  const entries = useMemo(() => resolveFloatBarEntries(settings), [settings.floatBarEntries, settings.floatBarProviderIds]);

  const previewProviders = useMemo(() => {
    // Use expanded entries to determine which providers to show in preview
    const expanded = expandFloatBarEntries(entries, settings.enabledProviders ?? []);
    const specific = expanded.map((e) => e.providerId).filter((id) => id && id !== TASKBAR_PROVIDER_AUTO);
    // If specific entries exist, filter catalog by those ids
    if (specific.length > 0) {
      const filtered = catalog.filter((row) => specific.includes(row.providerId));
      // If filtering yields empty (e.g. store not yet populated but catalog contains all), return filtered or fallback
      return filtered.length > 0 ? filtered : catalog.filter((row) => specific.includes(row.providerId));
    }
    // Auto entries: show first N providers where N = expanded.length or entries.length
    const count = Math.max(1, expanded.length > 0 ? expanded.length : entries.length);
    return catalog.slice(0, count);
  }, [catalog, entries, settings.enabledProviders]);

  const previewSettings = useMemo(
    () => ({
      ...settings,
      floatBarProviderIds: previewProviders.map((row) => row.providerId),
      floatBarEntries: entries,
    }),
    [previewProviders, settings, entries],
  );
  const state = useMemo(
    () => ({
      contractVersion: "preview",
      providers: [],
      settings: previewSettings,
    }),
    [previewSettings],
  );

  return (
    <FloatBar
      state={state}
      preview={{ settings: previewSettings, providers: previewProviders }}
    />
  );
}
