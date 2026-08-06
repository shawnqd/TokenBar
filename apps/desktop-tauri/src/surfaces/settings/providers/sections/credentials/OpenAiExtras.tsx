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
 * Bespoke "extra config" credential row for the OpenAI-shaped providers that
 * expose a single workspace/project id, base URL, or gateway field
 * (openaiapi, litellm, devin, opencodego, zed, sub2api, wayfinder).
 *
 * Codex used to fall through this component's no-config branch and render
 * only usage-history help text here, which made its 认证来源 (auth sources)
 * block look empty (TASK-021 item 2). Codex has its own real auth entry now
 * (see `ProviderSignInEntry` in `ProviderDetailPane.tsx`) and no longer
 * dispatches here; the no-config branch below is a defensive fallback only
 * and renders nothing.
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

  // No bespoke field is defined for this provider id — nothing to render.
  // (Historically this rendered the Codex usage-history help text; that copy
  // now lives next to `StatsSection` — see `CodexUsageHistoryNote` in
  // `ProviderDetailPane.tsx` — since it describes local history tracking,
  // not an authentication source.)
  return null;
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
