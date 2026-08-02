import { useEffect, useState } from "react";
import type { LocaleKey } from "../../../../../i18n/keys";
import {
  getProviderWorkspaceId,
  getProviderGatewayUrl,
  setProviderWorkspaceId,
  setProviderGatewayUrl,
} from "../../../../../lib/tauri";
import { ProviderSection } from "../../shell/ProviderWorkspace";

interface Props {
  providerId?: string;
  t: (key: LocaleKey) => string;
}

/**
 * OpenAI/Codex-specific detail help.
 *
 * Port of the help strings below the `ProviderId::Codex` toggles in
 * `rust/src/native_ui/preferences.rs::render_provider_detail_panel` (~6625).
 * The toggles themselves (`codex_historical_tracking`,
 * `codex_openai_web_extras`) are not yet persisted through
 * `update_settings` in the Tauri bridge, so this component shows the
 * upstream hint copy only. The toggles will be surfaced once they join
 * the SettingsUpdate bridge (tracked alongside Phase 6e token-accounts).
 */
export function OpenAiExtras({ providerId = "codex", t }: Props) {
  const [projectId, setProjectId] = useState("");
  const [savedProjectId, setSavedProjectId] = useState("");
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    // Config presence is keyed only by provider id (titles come later).
    if (!extraConfig(providerId, t)) return;
    let cancelled = false;
    const readValue = providerId === "wayfinder"
      ? getProviderGatewayUrl(providerId)
      : getProviderWorkspaceId(providerId);
    void readValue
      .then((value) => {
        if (!cancelled) {
          setProjectId(value ?? "");
          setSavedProjectId(value ?? "");
        }
      })
      .catch((e) => {
        if (!cancelled) setError(String(e));
      });
    return () => {
      cancelled = true;
    };
  }, [providerId]);

  const saveProjectId = async () => {
    setBusy(true);
    setError(null);
    try {
      const next = projectId.trim();
      if (providerId === "wayfinder") {
        await setProviderGatewayUrl(providerId, next);
      } else {
        await setProviderWorkspaceId(providerId, next);
      }
      setSavedProjectId(next);
    } catch (e) {
      setError(String(e));
    } finally {
      setBusy(false);
    }
  };

  const config = extraConfig(providerId, t);
  if (config) {
    return (
      <ProviderSection title={config.title}>
        <label className="provider-detail-field">
          <span className="provider-detail-field__label">
            {config.label}
          </span>
          <input
            className="provider-detail-field__input"
            value={projectId}
            placeholder={config.placeholder}
            spellCheck={false}
            onChange={(event) => setProjectId(event.target.value)}
          />
        </label>
        <div className="provider-detail-helper">
          {config.help}
        </div>
        <div className="provider-detail-actions">
          <button
            type="button"
            className="credential-btn credential-btn--primary"
            disabled={busy || projectId.trim() === savedProjectId}
            onClick={saveProjectId}
          >
            {t("Save")}
          </button>
        </div>
        {error && <div className="provider-detail-error">{error}</div>}
      </ProviderSection>
    );
  }

  return (
    <ProviderSection title={t("CredentialsSectionTitle")}>
      <div className="provider-detail-helper">
        {t("ProviderCodexHistoryHelp")}
      </div>
      <div className="provider-detail-helper">
        {t("CredsOpenAiHistoryHelp")}
      </div>
    </ProviderSection>
  );
}

function extraConfig(
  providerId: string,
  t: (key: LocaleKey) => string,
) {
  switch (providerId) {
    case "openaiapi":
      return {
        title: t("ExtrasOpenAiApiTitle"),
        label: t("ExtrasProjectIdLabel"),
        placeholder: "proj_...",
        help: t("ExtrasOpenAiApiHelp"),
      };
    case "litellm":
      return {
        title: t("ExtrasLiteLlmTitle"),
        label: t("ExtrasBaseUrlLabel"),
        placeholder: "https://litellm.example.com",
        help: t("ExtrasLiteLlmHelp"),
      };
    case "devin":
      return {
        title: t("ExtrasDevinTitle"),
        label: t("ExtrasOrganizationLabel"),
        placeholder: "org/acme",
        help: t("ExtrasDevinHelp"),
      };
    case "opencodego":
      return {
        title: t("OpenCodeGoWorkspaceTitle"),
        label: t("OpenCodeGoWorkspaceLabel"),
        placeholder: "wrk_...",
        help: t("OpenCodeGoWorkspaceHelp"),
      };
    case "zed":
      return {
        title: t("ExtrasZedTitle"),
        label: t("ExtrasApiUrlLabel"),
        placeholder: "https://cloud.zed.dev/client/users/me",
        help: t("ExtrasZedHelp"),
      };
    case "sub2api":
      return {
        title: "sub2api 连接",
        label: "基础地址",
        placeholder: "https://sub2api.example.com",
        help: "填写分组 API Key 所属的 sub2api 服务地址。远程地址必须使用 HTTPS。",
      };
    case "wayfinder":
      return {
        title: "Wayfinder 网关",
        label: "本地网关地址",
        placeholder: "http://127.0.0.1:8088",
        help: "只允许 localhost 或回环地址，TokenBar 不会把网关数据发送到外部。",
      };
    default:
      return null;
  }
}
