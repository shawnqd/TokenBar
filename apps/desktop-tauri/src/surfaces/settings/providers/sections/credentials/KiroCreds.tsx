import { useEffect, useState } from "react";
import type { LocaleKey } from "../../../../../i18n/keys";
import type { KiroStatus } from "../../../../../types/bridge";
import { getKiroStatus, openPath } from "../../../../../lib/tauri";
import {
  ProviderAuthMethod,
  ProviderSection,
} from "../../shell/ProviderWorkspace";

interface Props {
  t: (key: LocaleKey) => string;
}

/**
 * Kiro CLI availability row.
 *
 * Backed by the Phase-4 `get_kiro_status` IPC which returns availability
 * plus a hint string (either the detected CLI path or a not-found error).
 */
export function KiroCreds({ t }: Props) {
  const [status, setStatus] = useState<KiroStatus | null>(null);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    let cancelled = false;
    getKiroStatus()
      .then((s) => !cancelled && setStatus(s))
      .catch((e) => !cancelled && setError(String(e)));
    return () => {
      cancelled = true;
    };
  }, []);

  if (!status) return null;

  const statusLabel = status.available
    ? t("CredsStatusAvailable")
    : t("CredsStatusUnavailable");

  const handleOpenFolder = () => {
    if (!status.hint) return;
    void openPath(status.hint).catch((e) => setError(String(e)));
  };

  return (
    <ProviderSection title={t("CredentialsSectionTitle")}>
      <ProviderAuthMethod
        title={t("CredsKiroLabel")}
        badge={statusLabel}
        badgeTone={status.available ? "ok" : "unset"}
        meta={
          status.available && status.hint ? (
            <span className="provider-detail-grid__mono">{status.hint}</span>
          ) : null
        }
        actions={
          status.available && status.hint ? (
            <button
              type="button"
              className="btn btn--ghost"
              onClick={handleOpenFolder}
            >
              {t("CredsOpenFolderAction")}
            </button>
          ) : null
        }
      >
        {!status.available && (
          <div className="provider-detail-helper">
            {t("CredsKiroHelperMissing")}
          </div>
        )}
        {error && <div className="provider-detail-error">{error}</div>}
      </ProviderAuthMethod>
    </ProviderSection>
  );
}
