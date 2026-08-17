import { useState } from "react";
import { resetSettings } from "../../../lib/tauri";
import { useLocale } from "../../../hooks/useLocale";
import type { UpdateChannel } from "../../../types/bridge";
import type { SettingsPageProps } from "./pageTypes";
import { ConfirmDialog, V5Field, V5Section, V5Seg, V5Toggle } from "./v5Controls";

export default function AdvancedPage({
  settings,
  set,
  saving,
}: SettingsPageProps) {
  const { t } = useLocale();
  const [confirmReset, setConfirmReset] = useState(false);
  const [note, setNote] = useState<string | null>(null);

  return (
    <>
      <V5Section title={t("UpdatesTitle")}>
        <V5Field label={t("UpdateChannelChoice")}>
          <V5Seg
            value={settings.updateChannel}
            disabled={saving}
            options={[
              { value: "stable", label: t("UpdateChannelStableOption") },
              { value: "beta", label: t("UpdateChannelBetaOption") },
            ]}
            onChange={(value) => set({ updateChannel: value as UpdateChannel })}
          />
        </V5Field>
        <V5Field label={t("AutoDownloadUpdates")}>
          <V5Toggle
            on={settings.autoDownloadUpdates}
            disabled={saving}
            onChange={(v) => set({ autoDownloadUpdates: v })}
            label={t("AutoDownloadUpdates")}
          />
        </V5Field>
        <V5Field label={t("InstallUpdatesOnQuit")}>
          <V5Toggle
            on={settings.installUpdatesOnQuit}
            disabled={saving}
            onChange={(v) => set({ installUpdatesOnQuit: v })}
            label={t("InstallUpdatesOnQuit")}
          />
        </V5Field>
      </V5Section>

      <V5Section title="诊断">
        <V5Field
          label={t("SettingsRefreshTrace")}
          help={t("SettingsRefreshTraceHelper")}
        >
          <button type="button" className="s5-ghost" disabled>
            {t("SettingsNotWired")}
          </button>
        </V5Field>
        <V5Field label={t("SettingsSurfaceLifecycleTrace")}>
          <button type="button" className="s5-ghost" disabled>
            {t("SettingsNotWired")}
          </button>
        </V5Field>
        <V5Field label={t("SettingsSchemaVersion")}>
          <span className="s5-unit">{t("SettingsNotWired")}</span>
        </V5Field>
      </V5Section>

      <V5Section title="重置">
        <V5Field
          label={t("SettingsResetAll")}
          help={t("SettingsResetAllHelper")}
        >
          <button
            type="button"
            className="s5-ghost danger"
            disabled={saving}
            onClick={() => setConfirmReset(true)}
          >
            重置设置
          </button>
        </V5Field>
        {note ? <p className="s5-hint">{note}</p> : null}
      </V5Section>

      {confirmReset ? (
        <ConfirmDialog
          text="将恢复全部设置默认值，不会删除服务商凭据。"
          onCancel={() => setConfirmReset(false)}
          onConfirm={() => {
            setConfirmReset(false);
            void resetSettings()
              .then(() => setNote("已重置全部设置"))
              .catch((error) => setNote(String(error)));
          }}
        />
      ) : null}
    </>
  );
}
