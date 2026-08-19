import { useCallback, useEffect, useState } from "react";
import { useLocale } from "../../../hooks/useLocale";
import { Field, Toggle } from "../../../components/FormControls";
import type { TabProps } from "../../Settings";

function formatCodexSessionsDirs(paths: string[]): string {
  return paths.join("; ");
}

function parseCodexSessionsDirs(value: string): string[] {
  return value
    .split(/[;\n]/)
    .map((path) => path.trim())
    .filter(Boolean);
}

export default function PrivacyTab({ settings, set, saving }: TabProps) {
  const { t } = useLocale();
  const [codexDirsDraft, setCodexDirsDraft] = useState(() =>
    formatCodexSessionsDirs(settings.codexCustomSessionsDirs),
  );

  useEffect(() => {
    if (!saving) {
      setCodexDirsDraft(formatCodexSessionsDirs(settings.codexCustomSessionsDirs));
    }
  }, [saving, settings.codexCustomSessionsDirs]);

  const commitCodexDirs = useCallback(() => {
    set({ codexCustomSessionsDirs: parseCodexSessionsDirs(codexDirsDraft) });
  }, [codexDirsDraft, set]);

  return (
    <>
      <section className="settings-section">
        <h3 className="settings-section__title">{t("PrivacyTitle")}</h3>
        <div className="settings-section__group">
          <Field
            label={t("HidePersonalInfo")}
            description={t("HidePersonalInfoHelper")}
          >
            <Toggle
              checked={settings.hidePersonalInfo}
              disabled={saving}
              onChange={(v) => set({ hidePersonalInfo: v })}
            />
          </Field>
        </div>
      </section>

      <section className="settings-section">
        <h3 className="settings-section__title settings-section__title--bold">
          {t("SectionKeychainAccess")}
        </h3>
        <p className="settings-section__caption">{t("KeychainAccessCaption")}</p>
        <div className="settings-section__group">
          <Field
            label={t("DisableAllKeychainLabel")}
            description={t("DisableAllKeychainHelper")}
          >
            <Toggle
              checked={settings.disableKeychainAccess}
              disabled={saving}
              onChange={(v) => set({ disableKeychainAccess: v })}
            />
          </Field>
          <Field
            label={t("AvoidKeychainPromptsLabel")}
            description={t("AvoidKeychainPromptsHelper")}
          >
            <Toggle
              checked={settings.claudeAvoidKeychainPrompts}
              disabled={saving || settings.disableKeychainAccess}
              onChange={(v) => set({ claudeAvoidKeychainPrompts: v })}
            />
          </Field>
        </div>
      </section>

      <section className="settings-section">
        <h3 className="settings-section__title settings-section__title--bold">
          {t("CodexLocalLogsTitle")}
        </h3>
        <p className="settings-section__caption">{t("CodexLocalLogsCaption")}</p>
        <div className="settings-section__group">
          <Field
            label={t("CodexLogPathsLabel")}
            description={t("CodexLogPathsHelper")}
          >
            <input
              type="text"
              className="text-input"
              value={codexDirsDraft}
              placeholder={String.raw`\\wsl.localhost\<distro>\home\<user>\.codex`}
              disabled={saving}
              onChange={(event) => setCodexDirsDraft(event.target.value)}
              onBlur={commitCodexDirs}
              onKeyDown={(event) => {
                if (event.key === "Enter") {
                  event.currentTarget.blur();
                }
              }}
            />
          </Field>
        </div>
      </section>

      <section className="settings-section">
        <h3 className="settings-section__title">{t("SettingsLocalCache")}</h3>
        <div className="settings-section__group">
          <Field
            label={t("SettingsClearCache")}
            description={t("SettingsNotWired")}
          >
            <button
              type="button"
              className="settings-v5-mock"
              disabled
              aria-disabled="true"
            >
              {t("SettingsNotWired")}
            </button>
          </Field>
          <Field
            label={t("SettingsExportDiagnostics")}
            description={t("SettingsNotWired")}
          >
            <button
              type="button"
              className="settings-v5-mock"
              disabled
              aria-disabled="true"
            >
              {t("SettingsNotWired")}
            </button>
          </Field>
        </div>
        <p className="settings-section__hint">{t("SettingsLocalDataSources")}</p>
      </section>
    </>
  );
}
