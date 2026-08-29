import { useCallback, useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { ShortcutCapture } from "../../../components/ShortcutCapture";
import { useLocale } from "../../../hooks/useLocale";
import { invokeSurfaceAction } from "../../../lib/tauri";
import type { Language, LanguageOption } from "../../../types/bridge";
import type { LocaleKey } from "../../../i18n/keys";
import type { SettingsPageProps } from "./pageTypes";
import { V5Field, V5Section, V5Select, V5Toggle } from "./v5Controls";

const FALLBACK_LANGUAGE_OPTIONS: LanguageOption[] = [
  { value: "english", display: "English" },
  { value: "chinese", display: "中文" },
  { value: "chinesetraditional", display: "繁體中文（臺灣）" },
  { value: "japanese", display: "日本語" },
  { value: "korean", display: "한국어" },
  { value: "spanish", display: "Español" },
];

const REFRESH_CADENCE: { value: string; labelKey: LocaleKey }[] = [
  { value: "0", labelKey: "RefreshCadenceManual" },
  { value: "60", labelKey: "RefreshCadenceOneMinute" },
  { value: "300", labelKey: "RefreshCadenceFiveMinutes" },
  { value: "900", labelKey: "RefreshCadenceFifteenMinutes" },
  { value: "1800", labelKey: "RefreshCadenceThirtyMinutes" },
  { value: "3600", labelKey: "RefreshCadenceOneHour" },
];

export default function GeneralPage({
  settings,
  set,
  saving,
}: SettingsPageProps) {
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
        await invokeSurfaceAction({
          type: "registerGlobalShortcut",
          target: { kind: "app" },
          accelerator,
        }).catch(() => {});
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
      await invokeSurfaceAction({
        type: "unregisterGlobalShortcut",
        target: { kind: "app" },
      }).catch(() => {});
      set({ globalShortcut: "" });
    } catch (err: unknown) {
      setShortcutError(err instanceof Error ? err.message : String(err));
    }
  }, [set]);

  return (
    <>
      <V5Section title={t("SectionLanguage")}>
        <V5Field label={t("InterfaceLanguage")} help="切换后只换文案，不改变窗口大小">
          <V5Select
            value={settings.uiLanguage}
            disabled={saving}
            options={languageOptions.map((opt) => ({
              value: opt.value,
              label: opt.display,
            }))}
            onChange={(value) => set({ uiLanguage: value as Language })}
          />
        </V5Field>
      </V5Section>

      <V5Section title={t("StartupSettings")}>
        <V5Field label={t("StartAtLogin")} help={t("StartAtLoginHelper")}>
          <V5Toggle
            on={settings.startAtLogin}
            disabled={saving}
            onChange={(v) => set({ startAtLogin: v })}
            label={t("StartAtLogin")}
          />
        </V5Field>
        <V5Field label={t("StartMinimized")} help={t("StartMinimizedHelper")}>
          <V5Toggle
            on={settings.startMinimized}
            disabled={saving}
            onChange={(v) => set({ startMinimized: v })}
            label={t("StartMinimized")}
          />
        </V5Field>
      </V5Section>

      <V5Section title={t("SectionRefresh")}>
        <V5Field
          label={t("RefreshIntervalLabel")}
          help={t("RefreshIntervalHelper")}
        >
          <V5Select
            value={String(settings.refreshIntervalSecs)}
            disabled={saving}
            options={REFRESH_CADENCE.map((opt) => ({
              value: String(opt.value),
              label: t(opt.labelKey),
            }))}
            onChange={(value) => set({ refreshIntervalSecs: Number(value) })}
          />
        </V5Field>
        <V5Field
          label={t("ProviderTimeoutRecovery")}
          help={t("ProviderTimeoutRecoveryHelper")}
        >
          <V5Toggle
            on={settings.providerTimeoutRecoveryEnabled ?? true}
            disabled={saving}
            onChange={(v) => set({ providerTimeoutRecoveryEnabled: v })}
            label={t("ProviderTimeoutRecovery")}
          />
        </V5Field>
        <V5Field
          label={t("RefreshAllProvidersOnMenuOpen")}
          help={t("RefreshAllProvidersOnMenuOpenHelper")}
        >
          <V5Toggle
            on={settings.refreshAllProvidersOnMenuOpen}
            disabled={saving}
            onChange={(v) => set({ refreshAllProvidersOnMenuOpen: v })}
            label={t("RefreshAllProvidersOnMenuOpen")}
          />
        </V5Field>
      </V5Section>

      <V5Section title={t("SectionKeyboard")}>
        <V5Field
          label={t("GlobalShortcutFieldLabel")}
          help={t("GlobalShortcutToggleHelper")}
        >
          <div className="s5-shortcut">
            <ShortcutCapture
              value={settings.globalShortcut}
              disabled={saving}
              onCommit={(accel) => void commitShortcut(accel)}
              onClear={() => void clearShortcut()}
            />
          </div>
        </V5Field>
        {shortcutError ? (
          <p className="s5-hint">{shortcutError}</p>
        ) : (
          <p className="s5-hint">{t("ShortcutRecordingHint")}</p>
        )}
      </V5Section>
    </>
  );
}
