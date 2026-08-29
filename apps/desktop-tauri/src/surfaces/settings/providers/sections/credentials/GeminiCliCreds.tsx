import { useEffect, useState } from "react";
import type { LocaleKey } from "../../../../../i18n/keys";
import type { GeminiCliStatus } from "../../../../../types/bridge";
import {
  getGeminiCliSignedIn,
} from "../../../../../lib/tauri";
import { useDispatchAction } from "../../../../../core/useCoreBridge";
import { requireActionResult } from "../../../../../core/actionDispatcher";
import {
  ProviderAuthMethod,
  ProviderSection,
} from "../../shell/ProviderWorkspace";

interface Props {
  providerId: string;
  t: (key: LocaleKey) => string;
}

/**
 * Gemini CLI credentials row.
 *
 * Port of the `ProviderId::Gemini` branch in
 * `rust/src/native_ui/preferences.rs::render_provider_detail_panel` (~5570).
 * Shows OAuth-credential presence + path + a button that opens the
 * credentials folder (when signed in) or a hint to install the CLI.
 */
export function GeminiCliCreds({ providerId, t }: Props) {
  const dispatch = useDispatchAction();
  const [status, setStatus] = useState<GeminiCliStatus | null>(null);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    let cancelled = false;
    getGeminiCliSignedIn()
      .then((s) => !cancelled && setStatus(s))
      .catch((e) => !cancelled && setError(String(e)));
    return () => {
      cancelled = true;
    };
  }, []);

  if (!status) return null;

  const statusLabel = status.signedIn
    ? t("CredsStatusAuthenticated")
    : t("CredsStatusNotSignedIn");

  const handleOpenFolder = () => {
    if (!status.credentialsPath) return;
    void requireActionResult(
      dispatch({
        type: "openPath",
        target: { kind: "app" },
        path: status.credentialsPath,
      }),
    ).catch((e) => setError(String(e)));
  };

  const handleSetup = () => {
    void dispatch({
      type: "openExternalUsage",
      target: { kind: "provider", providerId },
    }).catch((e) => setError(String(e)));
  };

  return (
    <ProviderSection title={t("CredentialsSectionTitle")}>
      <ProviderAuthMethod
        title={t("CredsGeminiCliLabel")}
        badge={statusLabel}
        badgeTone={status.signedIn ? "ok" : "unset"}
        meta={null}
        actions={
          <>
            {status.signedIn && status.credentialsPath && (
              <button
                type="button"
                className="btn btn--ghost"
                onClick={handleOpenFolder}
              >
                {t("CredsOpenFolderAction")}
              </button>
            )}
            {!status.signedIn && (
              <button
                type="button"
                className="btn btn--ghost"
                onClick={handleSetup}
              >
                {t("CredsGeminiCliSetupAction")}
              </button>
            )}
          </>
        }
      >
        {!status.signedIn && (
          <div className="provider-detail-helper">
            {t("CredsGeminiCliSetupHelp")}
          </div>
        )}
        {error && <div className="provider-detail-error">{error}</div>}
      </ProviderAuthMethod>
    </ProviderSection>
  );
}
