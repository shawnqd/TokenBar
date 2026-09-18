import { useOutputSpeedSnapshot } from "../../../hooks/useOutputSpeedSnapshot";
import { quotaDisplayContext } from "../../../lib/quotaDisplay";
import { coreSnapshotToBridge } from "../../../lib/trayProviders";
import { fromBridge, type ProviderSnapshot } from "../../../core";
import type { ProviderUsageSnapshot, SettingsSnapshot } from "../../../types/bridge";
import { trayCoreStore, useTrayCoreRecords } from "../../../surfaces/tray/trayCoreStore";
import { pickPreviewSnapshot } from "./catalogFixtures";
import TrayPanelContent from "../../../surfaces/tray/TrayPanelContent";
import TrayPanelFooter from "../../../surfaces/tray/TrayPanelFooter";

interface Props {
  settings: SettingsSnapshot;
}

/**
 * Settings preview for the tray panel. Same read model as the flyout:
 * records come from the tray core store (unified core read model) and chart /
 * speed slots render injected enrichment results. The host is retained for
 * compatibility with older settings callers, while the inner surface is the
 * same TrayPanelContent used by the real flyout.
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
  const liveRecord = records.find(
    (record) => record.snapshot?.providerId === snapshot.providerId,
  );
  const coreById = new Map<string, ProviderSnapshot>([
    [snapshot.providerId, liveRecord?.snapshot ?? fromBridge(snapshot)],
  ]);
  const chart = liveRecord ? trayCoreStore.getChartData(liveRecord.key) : null;

  return (
    <div className="settings-tray-preview">
      <div
        className="settings-tray-preview__scale s5-tray-flyout"
        style={{ transform: `scale(${scale})` }}
      >
        <TrayPanelContent
          gridProviders={[snapshot]}
          visibleProviders={[snapshot]}
          selectedProviderId={null}
          display={display}
          densityMode={settings.menuBarDisplayMode}
          showProviderIcons={settings.switcherShowsIcons}
          outputSpeed={outputSpeed}
          outputSpeedEnabled={outputSpeedEnabled}
          localUsagePeriod={settings.localUsagePeriod ?? "7d"}
          chartByProviderId={
            chart ? new Map([[snapshot.providerId, chart]]) : undefined
          }
          coreById={coreById}
          onSelect={() => {}}
          footer={<TrayPanelFooter interactive={false} />}
        />
      </div>
    </div>
  );
}
