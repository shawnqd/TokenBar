import { useCallback, useEffect, useState } from "react";
import { useLocale } from "../../../hooks/useLocale";
import {
  getApiKeyProviders,
  getApiKeys,
  removeApiKey,
  setApiKey,
} from "../../../lib/tauri";
import type {
  ApiKeyInfoBridge,
  ApiKeyProviderInfoBridge,
} from "../../../types/bridge";
import {
  ProviderAuthMethod,
  ProviderSection,
  ProviderStatusLine,
} from "./shell/ProviderWorkspace";

interface Props {
  providerId: string;
}

/**
 * Per-provider API key management, embedded inside the ProviderDetailPane.
 * Mirrors the upstream macOS layout where credential management lives next
 * to provider state instead of in a separate tab.
 */
export function ApiKeySection({ providerId }: Props) {
  const { t } = useLocale();
  const [info, setInfo] = useState<ApiKeyProviderInfoBridge | null>(null);
  const [saved, setSaved] = useState<ApiKeyInfoBridge | null>(null);
  const [loaded, setLoaded] = useState(false);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const [editing, setEditing] = useState(false);
  const [editValue, setEditValue] = useState("");
  const [editLabel, setEditLabel] = useState("");

  const reload = useCallback(async (signal: { stale: boolean }) => {
    try {
      const [providers, keys] = await Promise.all([
        getApiKeyProviders(),
        getApiKeys(),
      ]);
      if (signal.stale) return;
      setInfo(providers.find((p) => p.id === providerId) ?? null);
      setSaved(keys.find((k) => k.providerId === providerId) ?? null);
    } catch (err: unknown) {
      if (signal.stale) return;
      setError(err instanceof Error ? err.message : String(err));
    } finally {
      if (!signal.stale) setLoaded(true);
    }
  }, [providerId]);

  useEffect(() => {
    const signal = { stale: false };
    setLoaded(false);
    setEditing(false);
    setEditValue("");
    setEditLabel("");
    setError(null);
    setInfo(null);
    setSaved(null);
    void reload(signal);
    return () => {
      signal.stale = true;
    };
  }, [reload]);

  if (!loaded) return null;
  // Provider doesn't support API keys — render nothing.
  // Distinguish from "failed to load" by checking error state.
  if (!info && !error) return null;
  if (!info && error) {
    return (
      <ProviderSection title={t("ApiKeysTitle")}>
        <ProviderStatusLine tone="error">{error}</ProviderStatusLine>
      </ProviderSection>
    );
  }
  // After the guards above, info is guaranteed non-null.
  if (!info) return null;

  const handleSave = async () => {
    if (!editValue.trim()) return;
    setBusy(true);
    setError(null);
    try {
      const next = await setApiKey(
        providerId,
        editValue.trim(),
        editLabel.trim() || undefined,
      );
      setSaved(next.find((k) => k.providerId === providerId) ?? null);
      setEditing(false);
      setEditValue("");
      setEditLabel("");
    } catch (err: unknown) {
      setError(err instanceof Error ? err.message : String(err));
    } finally {
      setBusy(false);
    }
  };

  const handleRemove = async () => {
    setBusy(true);
    setError(null);
    try {
      const next = await removeApiKey(providerId);
      setSaved(next.find((k) => k.providerId === providerId) ?? null);
    } catch (err: unknown) {
      setError(err instanceof Error ? err.message : String(err));
    } finally {
      setBusy(false);
    }
  };

  return (
    <ProviderSection title={t("ApiKeysTitle")}>
      {error && <ProviderStatusLine tone="error">{error}</ProviderStatusLine>}

      <ProviderAuthMethod
        title={info.displayName || t("ApiKeysTitle")}
        badge={
          saved ? t("CredentialConfigured") : t("CredentialNotSet")
        }
        badgeTone={saved ? "ok" : "unset"}
        meta={
          saved ? (
            <>
              <span className="provider-auth-method__masked">
                {saved.maskedKey}
              </span>
              {saved.label && (
                <span className="provider-auth-method__label">
                  {saved.label}
                </span>
              )}
              <span>
                {t("CredentialSavedOnDate").replace("{}", saved.savedAt)}
              </span>
            </>
          ) : null
        }
        actions={
          <>
            {!editing && (
              <button
                className="credential-btn"
                disabled={busy}
                onClick={() => {
                  setEditing(true);
                  setEditValue("");
                  setEditLabel(saved?.label ?? "");
                }}
              >
                {saved ? t("CredentialUpdateButton") : t("CredentialAddKeyButton")}
              </button>
            )}
            {saved && !editing && (
              <button
                className="credential-btn credential-btn--danger"
                disabled={busy}
                onClick={() => void handleRemove()}
              >
                {t("Remove")}
              </button>
            )}
          </>
        }
      >
        {info.help && !editing && (
          <p className="provider-detail-helper">{info.help}</p>
        )}

        {info.dashboardUrl && !editing && (
          <a
            className="credential-card__link"
            href={info.dashboardUrl}
            target="_blank"
            rel="noopener noreferrer"
          >
            {t("CredentialOpenDashboard")}
          </a>
        )}

        {editing && (
          <div className="credential-card__edit">
            <input
              type="password"
              className="text-input credential-card__input"
              placeholder={t("PasteApiKeyHere")}
              autoComplete="off"
              value={editValue}
              onChange={(e) => setEditValue(e.target.value)}
              disabled={busy}
            />
            <input
              type="text"
              className="text-input credential-card__input credential-card__input--label"
              placeholder={t("CredentialLabelOptionalPlaceholder")}
              value={editLabel}
              onChange={(e) => setEditLabel(e.target.value)}
              disabled={busy}
            />
            <div className="credential-card__edit-actions">
              <button
                className="credential-btn credential-btn--primary"
                disabled={busy || !editValue.trim()}
                onClick={() => void handleSave()}
              >
                {t("Save")}
              </button>
              <button
                className="credential-btn"
                disabled={busy}
                onClick={() => {
                  setEditing(false);
                  setEditValue("");
                  setEditLabel("");
                }}
              >
                {t("Cancel")}
              </button>
            </div>
          </div>
        )}
      </ProviderAuthMethod>
    </ProviderSection>
  );
}
