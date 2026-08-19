import { useCallback, useState } from "react";
import { useLocale } from "../../../hooks/useLocale";
import { playNotificationSound } from "../../../lib/tauri";
import type { SettingsPageProps } from "./pageTypes";
import { V5Field, V5Num, V5Section, V5Toggle } from "./v5Controls";

export default function NotificationsPage({
  settings,
  set,
  saving,
}: SettingsPageProps) {
  const { t } = useLocale();
  const [playing, setPlaying] = useState(false);
  const notifyOff = !settings.showNotifications;

  const testSound = useCallback(() => {
    setPlaying(true);
    void playNotificationSound().catch(() => {});
    window.setTimeout(() => setPlaying(false), 1500);
  }, []);

  return (
    <>
      <V5Section title={t("SectionNotifications")}>
        <V5Field label={t("ShowNotifications")} help={t("ShowNotificationsHelper")}>
          <V5Toggle
            on={settings.showNotifications}
            disabled={saving}
            onChange={(v) => set({ showNotifications: v })}
            label={t("ShowNotifications")}
          />
        </V5Field>
        <V5Field label={t("SoundEnabled")} help={t("SoundEnabledHelper")} off={notifyOff}>
          <div className="s5-actions">
            <V5Toggle
              on={settings.soundEnabled}
              disabled={saving || notifyOff}
              onChange={(v) => set({ soundEnabled: v })}
              label={t("SoundEnabled")}
            />
            <button
              type="button"
              className="s5-ghost"
              disabled={saving || notifyOff || !settings.soundEnabled || playing}
              onClick={testSound}
            >
              测试声音
            </button>
          </div>
        </V5Field>
        <V5Field label="音量" help="0 到 100，每格 5" off={notifyOff}>
          <V5Num
            value={settings.soundVolume}
            min={0}
            max={100}
            step={5}
            disabled={saving || notifyOff}
            onChange={(v) => set({ soundVolume: v })}
          />
        </V5Field>
      </V5Section>

      <V5Section title="用量阈值">
        <V5Field label="高用量提醒" help="0 到 100，每格 5" off={notifyOff}>
          <V5Num
            value={settings.highUsageThreshold}
            min={0}
            max={100}
            step={5}
            unit="%"
            disabled={saving || notifyOff}
            onChange={(v) => set({ highUsageThreshold: v })}
          />
        </V5Field>
        <V5Field
          label="临界用量提醒"
          help="0 到 100，每格 5，应高于高用量"
          off={notifyOff}
        >
          <V5Num
            value={settings.criticalUsageThreshold}
            min={0}
            max={100}
            step={5}
            unit="%"
            disabled={saving || notifyOff}
            onChange={(v) => set({ criticalUsageThreshold: v })}
          />
        </V5Field>
      </V5Section>
    </>
  );
}
