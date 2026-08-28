import MenuCard from "../../../components/MenuCard";
import { useOutputSpeedSnapshot } from "../../../hooks/useOutputSpeedSnapshot";
import { outputSpeedProviderId } from "../../../lib/outputSpeed";
import { quotaDisplayContext } from "../../../lib/quotaDisplay";
import { coreSnapshotToBridge } from "../../../lib/trayProviders";
import type { ProviderUsageSnapshot, SettingsSnapshot } from "../../../types/bridge";
import { useTrayCoreRecords } from "../../../surfaces/tray/trayCoreStore";
import { pickPreviewSnapshot } from "./catalogFixtures";

interface Props {
  settings: SettingsSnapshot;
}

/**
 * Settings preview for the tray panel. Same read model as the flyout:
 * records come from the tray core store (unified core read model) and chart /
 * speed slots render injected enrichment results. The preview keeps the legacy
 * MenuCard surface (bridge-shaped data view only — no credential data, no
 * fake usage).
 */
export default function TrayPanelPreview({ settings }: Props) {
  const records = useTrayCoreRecords();
  const liveProviders: ProviderUsageSnapshot[] = records
    .map((record) =>
      record.snapshot ? coreSnapshotToBridge(record.snapshot) : null,
    )
    .filter((snapshot): snapshot is ProviderUsageSnapshot => snapshot != null);
  const outputSpeedEnabled = settings.outputSpeedEnabled !== false;
  const outputSpeed = useOutputSpeedSnapshot(outputSpeedEnabled);
  const snapshot = pickPreviewSnapshot(liveProviders);
  const display = quotaDisplayContext(settings, "dashboard");
  const scale = Math.max(1, Math.min(2, (settings.trayScalePercent ?? 100) / 100));
  const speedId = outputSpeedProviderId(snapshot.providerId);
  const speed = outputSpeedEnabled && speedId ? outputSpeed?.[speedId] ?? null : null;

  return (
    <div className="settings-tray-preview">
      <div
        className="settings-tray-preview__scale menu-surface menu-surface--tray"
        style={{ transform: `scale(${scale})` }}
      >
        <MenuCard
          provider={snapshot}
          display={display}
          densityMode={settings.menuBarDisplayMode}
          localUsagePeriod={settings.localUsagePeriod ?? "today"}
          showProviderIcon={settings.switcherShowsIcons}
          outputSpeed={speed}
          hideLocalUsage={settings.menuBarDisplayMode !== "detailed"}
        />
      </div>
    </div>
  );
}