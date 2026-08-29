import { useCallback, useState } from "react";
import { useLocale } from "../../../hooks/useLocale";
import { invokeSurfaceAction } from "../../../lib/tauri";
import { Field, NumberInput, Toggle } from "../../../components/FormControls";
import type { TabProps } from "../../Settings";

export default function NotificationsTab({ settings, set, saving }: TabProps) {
  const { t } = useLocale();
  const [playingSound, setPlayingSound] = useState(false);
  const notifyOff = !settings.showNotifications;

  const handleTestSound = useCallback(() => {
    setPlayingSound(true);
    void invokeSurfaceAction({
      type: "playNotificationSound",
      target: { kind: "settings" },
    }).catch(() => {});
    window.setTimeout(() => setPlayingSound(false), 1500);
  }, []);

  return (
    <>
      <section className="settings-section">
        <h3 className="settings-section__title">{t("SectionNotifications")}</h3>
        <div className="settings-section__group">
          <Field
            label={t("ShowNotifications")}
            description={t("ShowNotificationsHelper")}
          >
            <Toggle
              checked={settings.showNotifications}
              disabled={saving}
              onChange={(v) => set({ showNotifications: v })}
            />
          </Field>
          <Field
            label={t("SoundEnabled")}
            description={t("SoundEnabledHelper")}
          >
            <div className="sound-enabled-row">
              <Toggle
                checked={settings.soundEnabled}
                disabled={saving || notifyOff}
                onChange={(v) => set({ soundEnabled: v })}
              />
              <button
                type="button"
                className="shortcut-capture__button shortcut-capture__button--ghost"
                disabled={
                  saving || notifyOff || !settings.soundEnabled || playingSound
                }
                onClick={handleTestSound}
              >
                {playingSound
                  ? t("NotificationTestSoundPlaying")
                  : t("NotificationTestSound")}
              </button>
            </div>
          </Field>
          <Field
            label={t("SoundVolume")}
            description={t("SoundVolumeHelper")}
          >
            <NumberInput
              value={settings.soundVolume}
              min={0}
              max={100}
              step={5}
              disabled={saving || notifyOff || !settings.soundEnabled}
              onChange={(v) => set({ soundVolume: v })}
            />
          </Field>
        </div>
      </section>

      <section className="settings-section">
        <h3 className="settings-section__title">{t("SectionUsageThresholds")}</h3>
        <p className="settings-section__hint">
          {t("SettingsUsageThresholdsCaption")}
        </p>
        <div className="settings-section__group">
          <Field
            label={t("HighUsageAlert")}
            description={t("HighUsageWarningHelper")}
          >
            <NumberInput
              value={settings.highUsageThreshold}
              min={0}
              max={100}
              step={5}
              disabled={saving || notifyOff}
              onChange={(v) => set({ highUsageThreshold: v })}
            />
          </Field>
          <Field
            label={t("CriticalUsageAlert")}
            description={t("CriticalUsageWarningHelper")}
          >
            <NumberInput
              value={settings.criticalUsageThreshold}
              min={0}
              max={100}
              step={5}
              disabled={saving || notifyOff}
              onChange={(v) => set({ criticalUsageThreshold: v })}
            />
          </Field>
        </div>
      </section>
    </>
  );
}
