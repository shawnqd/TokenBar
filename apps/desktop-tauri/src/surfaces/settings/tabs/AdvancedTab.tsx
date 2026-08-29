import { useState } from "react";
import { useLocale } from "../../../hooks/useLocale";
import { Field, SegmentedControl, Toggle } from "../../../components/FormControls";
import type { UpdateChannel } from "../../../types/bridge";
import type { TabProps } from "../../Settings";
import { resetSettings } from "../../../lib/tauri";
import { useAdvancedDiagnostics } from "../AdvancedDiagnostics";

export default function AdvancedTab({ settings, set, saving }: TabProps) {
  const { t } = useLocale();
  const diagnostics = useAdvancedDiagnostics();
  const [resetNote, setResetNote] = useState<string | null>(null);

  return (
    <>
      <section className="settings-section">
        <h3 className="settings-section__title">{t("UpdatesTitle")}</h3>
        <div className="settings-section__group">
          <Field
            label={t("UpdateChannelChoice")}
            description={t("UpdateChannelChoiceHelper")}
          >
            <SegmentedControl
              value={settings.updateChannel}
              disabled={saving}
              options={[
                { value: "stable", label: t("UpdateChannelStableOption") },
                { value: "beta", label: t("UpdateChannelBetaOption") },
              ]}
              onChange={(value) => set({ updateChannel: value as UpdateChannel })}
            />
          </Field>
          <Field
            label={t("AutoDownloadUpdates")}
            description={t("AutoDownloadUpdatesHelper")}
          >
            <Toggle
              checked={settings.autoDownloadUpdates}
              disabled={saving}
              onChange={(v) => set({ autoDownloadUpdates: v })}
            />
          </Field>
          <Field
            label={t("InstallUpdatesOnQuit")}
            description={t("InstallUpdatesOnQuitHelper")}
          >
            <Toggle
              checked={settings.installUpdatesOnQuit}
              disabled={saving}
              onChange={(v) => set({ installUpdatesOnQuit: v })}
            />
          </Field>
        </div>
      </section>

      <section className="settings-section">
        <h3 className="settings-section__title">{t("SettingsRefreshTrace")}</h3>
        <div className="settings-section__group">
          <Field
            label={t("SettingsRefreshTrace")}
            description={t("SettingsRefreshTraceHelper")}
          >
            <button type="button" onClick={diagnostics.reload} disabled={diagnostics.loading}>
              {diagnostics.loading ? "…" : "Reload"}
            </button>
            <pre className="settings-v5-readonly" style={{ whiteSpace: "pre-wrap" }}>
              {diagnostics.refreshTrace}
            </pre>
          </Field>
          <Field
            label={t("SettingsSurfaceLifecycleTrace")}
            description={diagnostics.lifecycleTrace}
          >
            <pre className="settings-v5-readonly" style={{ whiteSpace: "pre-wrap" }}>
              {diagnostics.lifecycleTrace}
            </pre>
          </Field>
          <Field label={t("SettingsSchemaVersion")}>
            <span className="settings-v5-readonly">{diagnostics.schemaVersion}</span>
          </Field>
          <Field label="Cache / diagnostics">
            <span className="settings-v5-readonly">{diagnostics.cacheSummary}</span>
            {diagnostics.error ? <p>{diagnostics.error}</p> : null}
          </Field>
        </div>
      </section>

      <section className="settings-section">
        <h3 className="settings-section__title">{t("SettingsResetAll")}</h3>
        <div className="settings-section__group">
          <Field
            label={t("SettingsResetAll")}
            description={t("SettingsResetAllHelper")}
          >
            <button
              type="button"
              className="settings-v5-mock settings-v5-mock--danger"
              disabled={saving}
              onClick={() => {
                void resetSettings()
                  .then(() => setResetNote("reset"))
                  .catch((error) => setResetNote(String(error)));
              }}
            >
              {t("SettingsResetAll")}
            </button>
            {resetNote ? <p className="settings-v5-readonly">{resetNote}</p> : null}
          </Field>
        </div>
      </section>
    </>
  );
}
