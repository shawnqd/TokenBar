import MenuCard from "../../../components/MenuCard";
import { useOutputSpeedSnapshot } from "../../../hooks/useOutputSpeedSnapshot";
import { useProviders } from "../../../hooks/useProviders";
import { outputSpeedProviderId } from "../../../lib/outputSpeed";
import { quotaDisplayContext } from "../../../lib/quotaDisplay";
import type { SettingsSnapshot } from "../../../types/bridge";
import { pickPreviewSnapshot } from "./catalogFixtures";

interface Props {
  settings: SettingsSnapshot;
}

export default function TrayPanelPreview({ settings }: Props) {
  const { providers } = useProviders({ refreshOnMount: false });
  const outputSpeedEnabled = settings.outputSpeedEnabled !== false;
  const outputSpeed = useOutputSpeedSnapshot(outputSpeedEnabled);
  const snapshot = pickPreviewSnapshot(providers);
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
