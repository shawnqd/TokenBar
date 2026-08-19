import { useCallback, useEffect, useState } from "react";
import { useLocale } from "../../../hooks/useLocale";
import { invoke } from "@tauri-apps/api/core";
import {
  registerGlobalShortcut,
  unregisterGlobalShortcut,
} from "../../../lib/tauri";
import { ShortcutCapture } from "../../../components/ShortcutCapture";
import { Field, Select, Toggle } from "../../../components/FormControls";
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

export default function GeneralTab({ settings, set, saving }: TabProps) {
  const { t } = useLocale();
  const [languageOptions, setLanguageOptions] = useState<LanguageOption[]>(
    FALLBACK_LANGUAGE_OPTIONS,
  );
  const [shortcutError, setShortcutError] = useState<string | null>(null);

  useEffect(() => {
    invoke<LanguageOption[]>("get_available_languages")
      .then(setLanguageOptions)
      .catch(() => {});
  }, []);

  const commitShortcut = useCallback(
    async (accelerator: string) => {
      setShortcutError(null);
      try {
        await registerGlobalShortcut(accelerator).catch(() => {});
        set({ globalShortcut: accelerator });
      } catch (err: unknown) {
        setShortcutError(err instanceof Error ? err.message : String(err));
      }
    },
    [set],
  );

  const clearShortcut = useCallback(async () => {
    setShortcutError(null);
    try {
      await unregisterGlobalShortcut().catch(() => {});
      set({ globalShortcut: "" });
    } catch (err: unknown) {
      setShortcutError(err instanceof Error ? err.message : String(err));
    }
  }, [set]);

  return (
    <>
      <section className="settings-section">
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
      </section>

      <section className="settings-section">
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
      </section>

      <section className="settings-section">
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
          <Field
            label={t("ProviderTimeoutRecovery")}
            description={t("ProviderTimeoutRecoveryHelper")}
          >
            <Toggle
              checked={settings.providerTimeoutRecoveryEnabled ?? true}
              disabled={saving}
              onChange={(v) => set({ providerTimeoutRecoveryEnabled: v })}
            />
          </Field>
        </div>
      </section>

      <section className="settings-section">
        <h3 className="settings-section__title">{t("SectionKeyboard")}</h3>
        <div className="settings-section__group">
          <Field
            label={t("GlobalShortcutFieldLabel")}
            description={t("GlobalShortcutToggleHelper")}
          >
            <ShortcutCapture
              value={settings.globalShortcut}
              disabled={saving}
              onCommit={(accel) => void commitShortcut(accel)}
              onClear={() => void clearShortcut()}
            />
          </Field>
        </div>
        {shortcutError && (
          <p className="settings-section__error">{shortcutError}</p>
        )}
        <p className="settings-section__hint">{t("ShortcutRecordingHint")}</p>
      </section>
    </>
  );
}
