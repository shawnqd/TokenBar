import { useState } from "react";
import { useDispatchAction } from "../../../core/useCoreBridge";
import { requireActionResult } from "../../../core/actionDispatcher";
import { useLocale } from "../../../hooks/useLocale";
import type { UpdateChannel } from "../../../types/bridge";
import type { SettingsPageProps } from "./pageTypes";
import { ConfirmDialog, V5Field, V5Section, V5Seg, V5Toggle } from "./v5Controls";
import { useAdvancedDiagnostics } from "../AdvancedDiagnostics";
import { useUpdateState } from "../../../hooks/useUpdateState";
import UpdateBanner from "../../../components/UpdateBanner";

export default function AdvancedPage({
  settings,
  set,
  saving,
}: SettingsPageProps) {
  const { t } = useLocale();
  const dispatch = useDispatchAction();
  const [confirmReset, setConfirmReset] = useState(false);
  const [note, setNote] = useState<string | null>(null);
  const diagnostics = useAdvancedDiagnostics();
  const updater = useUpdateState();
  const updateCheckBusy =
    updater.updateState.status === "checking" ||
    updater.updateState.status === "downloading";
  const updateStatus =
    updater.updateState.status === "checking"
      ? t("BannerCheckingForUpdates")
      : updater.updateState.status === "idle" &&
          updater.updateState.lastCheckedAt !== null
        ? t("AboutUpToDate")
        : undefined;

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
        <V5Field
          label={t("TrayCheckForUpdates")}
        >
          <div className="s5-update-check">
            <button
              type="button"
              className="s5-ghost"
              onClick={updater.checkNow}
              disabled={saving || updateCheckBusy}
            >
              {updateCheckBusy
                ? t("BannerCheckingForUpdates")
                : t("TrayCheckForUpdates")}
            </button>
            {updateStatus ? <span className="s5-unit">{updateStatus}</span> : null}
          </div>
        </V5Field>
        <UpdateBanner
          updateState={updater.updateState}
          onCheck={updater.checkNow}
          onDownload={updater.download}
          onApply={updater.apply}
          onDismiss={updater.dismiss}
          onOpenRelease={updater.openRelease}
        />
        {/* 退出时安装更新：退出链路当前直接退出、未实现安装，
            死开关暂时下架（2026-09-06 复核），实现后恢复。 */}
      </V5Section>

      <V5Section title="诊断">
        <V5Field
          label={t("SettingsRefreshTrace")}
          help={t("SettingsRefreshTraceHelper")}
        >
          <button type="button" className="s5-ghost" onClick={diagnostics.reload} disabled={diagnostics.loading}>
            {diagnostics.loading ? "…" : "刷新诊断"}
          </button>
          <pre className="s5-unit" style={{ whiteSpace: "pre-wrap", marginTop: 8 }}>
            {diagnostics.refreshTrace}
          </pre>
        </V5Field>
        <V5Field label={t("SettingsSurfaceLifecycleTrace")}>
          <pre className="s5-unit" style={{ whiteSpace: "pre-wrap" }}>
            {diagnostics.lifecycleTrace}
          </pre>
        </V5Field>
        <V5Field label={t("SettingsSchemaVersion")}>
          <span className="s5-unit">{diagnostics.schemaVersion}</span>
        </V5Field>
        <V5Field label="运行摘要" help="服务商数量、刷新间隔与平台信息（追踪记录仅存于当前进程）">
          <span className="s5-unit">{diagnostics.cacheSummary}</span>
          {diagnostics.error ? <p className="s5-hint">{diagnostics.error}</p> : null}
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
            void requireActionResult(
              dispatch({ type: "resetSettings", target: { kind: "settings" } }),
            )
              .then(() => setNote("已重置全部设置"))
              .catch((error) => setNote(String(error)));
          }}
        />
      ) : null}
    </>
  );
}
