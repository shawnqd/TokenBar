import { useCallback, useEffect, useState } from "react";
import { useLocale } from "../../../hooks/useLocale";
import { invoke } from "@tauri-apps/api/core";
import { playNotificationSound } from "../../../lib/tauri";
import { Field, NumberInput, Select, Toggle } from "../../../components/FormControls";
import type { Language, LanguageOption } from "../../../types/bridge";
import type { LocaleKey } from "../../../i18n/keys";
import type { TabProps } from "../../Settings";

const FALLBACK_LANGUAGE_OPTIONS: LanguageOption[] = [
  { value: "english", display: "English" },
  { value: "chinese", display: "中文" },
  { value: "chinesetraditional", display: "繁體中文（臺灣）" },
  { value: "japanese", display: "日本語" },
  { value: "korean", display: "한국어" },
  { value: "spanish", display: "Español" },
];

const REFRESH_CADENCE_KEYS: { value: string; labelKey: LocaleKey }[] = [
  { value: "0", labelKey: "RefreshCadenceManual" },
  { value: "60", labelKey: "RefreshCadenceOneMinute" },
  { value: "300", labelKey: "RefreshCadenceFiveMinutes" },
  { value: "900", labelKey: "RefreshCadenceFifteenMinutes" },
  { value: "1800", labelKey: "RefreshCadenceThirtyMinutes" },
  { value: "3600", labelKey: "RefreshCadenceOneHour" },
];

export default function GeneralTab({
  mode = "general",
  settings,
  set,
  saving,
}: TabProps & { mode?: "general" | "notifications" }) {
  const { t } = useLocale();
  const [playingSound, setPlayingSound] = useState(false);
  const [languageOptions, setLanguageOptions] = useState<LanguageOption[]>(
    FALLBACK_LANGUAGE_OPTIONS,
  );

  useEffect(() => {
    invoke<LanguageOption[]>("get_available_languages")
      .then(setLanguageOptions)
      .catch(() => {}); // graceful fallback to static default
  }, []);

  const handleTestSound = useCallback(() => {
    setPlayingSound(true);
    void playNotificationSound().catch(() => {});
    window.setTimeout(() => setPlayingSound(false), 1500);
  }, []);

  return (
    <>
      {mode === "general" && <section className="settings-section">
        <h3 className="settings-section__title">{t("SectionLanguage")}</h3>
        <div className="settings-section__group">
          <Field label={t("InterfaceLanguage")}>
            <Select
              value={settings.uiLanguage}
              disabled={saving}
              options={languageOptions.map((opt) => ({
                value: opt.value,
                label: opt.display,
              }))}
              onChange={(v) => set({ uiLanguage: v as Language })}
            />
          </Field>
        </div>
      </section>}

      {mode === "general" && <section className="settings-section">
        <h3 className="settings-section__title">{t("StartupSettings")}</h3>
        <div className="settings-section__group">
          <Field label={t("StartAtLogin")} description={t("StartAtLoginHelper")}>
            <Toggle
              checked={settings.startAtLogin}
              disabled={saving}
              onChange={(v) => set({ startAtLogin: v })}
            />
          </Field>
          <Field
            label={t("StartMinimized")}
            description={t("StartMinimizedHelper")}
          >
            <Toggle
              checked={settings.startMinimized}
              disabled={saving}
              onChange={(v) => set({ startMinimized: v })}
            />
          </Field>
        </div>
      </section>}

      {mode === "notifications" && <section className="settings-section">
        <h3 className="settings-section__title">
          {t("SectionNotifications")}
        </h3>
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
          <Field label={t("SoundEnabled")} description={t("SoundEnabledHelper")}>
            <div className="sound-enabled-row">
              <Toggle
                checked={settings.soundEnabled}
                disabled={saving}
                onChange={(v) => set({ soundEnabled: v })}
              />
              <button
                type="button"
                className="shortcut-capture__button shortcut-capture__button--ghost"
                disabled={saving || !settings.soundEnabled || playingSound}
                onClick={handleTestSound}
              >
                {playingSound
                  ? t("NotificationTestSoundPlaying")
                  : t("NotificationTestSound")}
              </button>
            </div>
          </Field>
          {settings.soundEnabled && (
            <Field label={t("SoundVolume")} description={t("SoundVolumeHelper")}>
              <NumberInput
                value={settings.soundVolume}
                min={0}
                max={100}
                step={5}
                disabled={saving}
                onChange={(v) => set({ soundVolume: v })}
              />
            </Field>
          )}
        </div>
      </section>}

      {mode === "notifications" && <section className="settings-section">
        <h3 className="settings-section__title">
          {t("SectionUsageThresholds")}
        </h3>
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
              disabled={saving}
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
              disabled={saving}
              onChange={(v) => set({ criticalUsageThreshold: v })}
            />
          </Field>
        </div>
      </section>}

      {/* ── Automation ───────────────────────────────────────────── */}
      {mode === "general" && <section className="settings-section">
        <h3 className="settings-section__title">{t("SectionRefresh")}</h3>
        <div className="settings-section__group">
          <Field
            label={t("RefreshIntervalLabel")}
            description={t("RefreshIntervalHelper")}
          >
            <Select
              value={String(settings.refreshIntervalSecs)}
              disabled={saving}
              options={REFRESH_CADENCE_KEYS.map((o) => ({
                value: o.value,
                label: t(o.labelKey),
              }))}
              onChange={(v) => set({ refreshIntervalSecs: Number(v) })}
            />
          </Field>
          <Field
            label={t("RefreshAllProvidersOnMenuOpen")}
            description={t("RefreshAllProvidersOnMenuOpenHelper")}
          >
            <Toggle
              checked={settings.refreshAllProvidersOnMenuOpen}
              disabled={saving}
              onChange={(v) => set({ refreshAllProvidersOnMenuOpen: v })}
            />
          </Field>
        </div>
      </section>}
    </>
  );
}
