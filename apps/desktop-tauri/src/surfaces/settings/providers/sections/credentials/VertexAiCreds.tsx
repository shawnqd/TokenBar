import { useEffect, useState } from "react";
import type { LocaleKey } from "../../../../../i18n/keys";
import type { VertexAiStatus } from "../../../../../types/bridge";
import {
  getVertexAiStatus,
  openPath,
  openProviderDashboard,
} from "../../../../../lib/tauri";
import {
  ProviderAuthMethod,
  ProviderSection,
} from "../../shell/ProviderWorkspace";

interface Props {
  providerId: string;
  t: (key: LocaleKey) => string;
}

/**
 * Vertex AI / Google Cloud credentials row.
 *
 * Port of the `ProviderId::VertexAI` branch in
 * `rust/src/native_ui/preferences.rs::render_provider_detail_panel` (~5611).
 */
export function VertexAiCreds({ providerId, t }: Props) {
  const [status, setStatus] = useState<VertexAiStatus | null>(null);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    let cancelled = false;
    getVertexAiStatus()
      .then((s) => !cancelled && setStatus(s))
      .catch((e) => !cancelled && setError(String(e)));
    return () => {
      cancelled = true;
    };
  }, []);

  if (!status) return null;

  const statusLabel = status.hasCredentials
    ? t("CredsStatusAuthenticated")
    : t("CredsStatusNotSignedIn");

  const handleOpenFolder = () => {
    if (!status.credentialsPath) return;
    void openPath(status.credentialsPath).catch((e) => setError(String(e)));
  };

  const handleSetup = () => {
    void openProviderDashboard(providerId).catch((e) => setError(String(e)));
  };

  return (
    <ProviderSection title={t("CredentialsSectionTitle")}>
      <ProviderAuthMethod
        title={t("CredsVertexAiLabel")}
        badge={statusLabel}
        badgeTone={status.hasCredentials ? "ok" : "unset"}
        meta={
          status.credentialsPath ? (
            <span className="provider-detail-grid__mono">
              {status.credentialsPath}
            </span>
          ) : null
        }
        actions={
          <>
            {status.hasCredentials && status.credentialsPath && (
              <button
                type="button"
                className="btn btn--ghost"
                onClick={handleOpenFolder}
              >
                {t("CredsOpenFolderAction")}
              </button>
            )}
            {!status.hasCredentials && (
              <button
                type="button"
                className="btn btn--ghost"
                onClick={handleSetup}
              >
                {t("CredsVertexAiSetupAction")}
              </button>
            )}
          </>
        }
      >
        {!status.hasCredentials && (
          <div className="provider-detail-helper">
            {t("CredsVertexAiSetupHelp")}
          </div>
        )}
        {error && <div className="provider-detail-error">{error}</div>}
      </ProviderAuthMethod>
    </ProviderSection>
  );
}
