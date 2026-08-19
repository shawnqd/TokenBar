import { useCallback, useEffect, useState } from "react";
import {
  clearProviderLocalUsageCache,
  getSafeDiagnostics,
} from "../../../lib/tauri";
import { useLocale } from "../../../hooks/useLocale";
import type { SettingsPageProps } from "./pageTypes";
import { ConfirmDialog, V5Field, V5Section, V5Toggle } from "./v5Controls";

function formatDirs(paths: string[]): string {
  return paths.join("; ");
}

function parseDirs(value: string): string[] {
  return value
    .split(/[;\n]/)
    .map((path) => path.trim())
    .filter(Boolean);
}

export default function PrivacyPage({
  settings,
  set,
  saving,
}: SettingsPageProps) {
  const { t } = useLocale();
  const [draft, setDraft] = useState(() =>
    formatDirs(settings.codexCustomSessionsDirs),
  );
  const [confirm, setConfirm] = useState<string | null>(null);
  const [mockNote, setMockNote] = useState<string | null>(null);

  useEffect(() => {
    if (!saving) setDraft(formatDirs(settings.codexCustomSessionsDirs));
  }, [saving, settings.codexCustomSessionsDirs]);

  const commit = useCallback(() => {
    set({ codexCustomSessionsDirs: parseDirs(draft) });
  }, [draft, set]);

  return (
    <>
      <V5Section title={t("PrivacyTitle")}>
        <V5Field label={t("HidePersonalInfo")} help={t("HidePersonalInfoHelper")}>
          <V5Toggle
            on={settings.hidePersonalInfo}
            disabled={saving}
            onChange={(v) => set({ hidePersonalInfo: v })}
            label={t("HidePersonalInfo")}
          />
        </V5Field>
      </V5Section>

      <V5Section
        title={t("SectionKeychainAccess")}
        hint="关掉总开关后，「减少提示」会变灰。"
      >
        <V5Field
          label={t("DisableAllKeychainLabel")}
          help={t("DisableAllKeychainHelper")}
        >
          <V5Toggle
            on={settings.disableKeychainAccess}
            disabled={saving}
            onChange={(v) => set({ disableKeychainAccess: v })}
            label={t("DisableAllKeychainLabel")}
          />
        </V5Field>
        <V5Field
          label={t("AvoidKeychainPromptsLabel")}
          help={t("AvoidKeychainPromptsHelper")}
          off={settings.disableKeychainAccess}
        >
          <V5Toggle
            on={settings.claudeAvoidKeychainPrompts}
            disabled={saving || settings.disableKeychainAccess}
            onChange={(v) => set({ claudeAvoidKeychainPrompts: v })}
            label={t("AvoidKeychainPromptsLabel")}
          />
        </V5Field>
      </V5Section>

      <V5Section
        title={t("CodexLocalLogsTitle")}
        hint="分号或换行分隔多个目录。失焦或回车提交。"
      >
        <V5Field label={t("CodexLogPathsLabel")} help={t("CodexLogPathsHelper")}>
          <input
            className="s5-textin"
            style={{ minWidth: 260 }}
            value={draft}
            placeholder={String.raw`\\wsl.localhost\<distro>\home\<user>\.codex`}
            disabled={saving}
            onChange={(event) => setDraft(event.target.value)}
            onBlur={commit}
            onKeyDown={(event) => {
              if (event.key === "Enter") event.currentTarget.blur();
            }}
          />
        </V5Field>
      </V5Section>

      <V5Section title={t("SettingsLocalCache")}>
        <V5Field label={t("SettingsClearCache")} help="不会登出，也不会删除凭据">
          <button
            type="button"
            className="s5-ghost danger"
            disabled={saving}
            onClick={() =>
              setConfirm("清理缓存不会删除凭据或登出账号。")
            }
          >
            清理缓存
          </button>
        </V5Field>
        <V5Field
          label={t("SettingsExportDiagnostics")}
          help="只含服务商、来源、状态和耗时，不含密钥"
        >
          <button
            type="button"
            className="s5-ghost"
            disabled={saving}
            onClick={() => {
              void getSafeDiagnostics()
                .then((data) => {
                  const blob = new Blob([JSON.stringify(data, null, 2)], {
                    type: "application/json",
                  });
                  const url = URL.createObjectURL(blob);
                  const a = document.createElement("a");
                  a.href = url;
                  a.download = `codexbar-diagnostics-${new Date().toISOString().slice(0, 10)}.json`;
                  a.click();
                  URL.revokeObjectURL(url);
                  setMockNote("诊断已导出");
                })
                .catch((error) => setMockNote(String(error)));
            }}
          >
            导出
          </button>
        </V5Field>
        {mockNote ? <p className="s5-hint">{mockNote}</p> : null}
        <p className="s5-hint">{t("SettingsLocalDataSources")}</p>
      </V5Section>

      {confirm ? (
        <ConfirmDialog
          text={confirm}
          onCancel={() => setConfirm(null)}
          onConfirm={() => {
            setConfirm(null);
            void clearProviderLocalUsageCache()
                .then(() => setMockNote("缓存已清理"))
                .catch((error) => setMockNote(String(error)));
          }}
        />
      ) : null}
    </>
  );
}
