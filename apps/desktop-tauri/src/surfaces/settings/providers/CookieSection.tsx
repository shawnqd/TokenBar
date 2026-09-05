import { useCallback, useEffect, useState } from "react";
import {
  getManualCookies,
} from "../../../lib/tauri";
import { useDispatchAction } from "../../../core/useCoreBridge";
import { requireActionResult } from "../../../core/actionDispatcher";
import { useLocale } from "../../../hooks/useLocale";
import type { CookieInfoBridge } from "../../../types/bridge";
import {
  ProviderAuthMethod,
  ProviderSection,
  ProviderStatusLine,
} from "./shell/ProviderWorkspace";

interface Props {
  providerId: string;
  cookieDomain: string | null;
}

function cookiePlaceholder(
  providerId: string,
  t: ReturnType<typeof useLocale>["t"],
): string {
  if (providerId === "ollama") {
    return t("BrowserCookiePlaceholderOllama");
  }
  if (providerId === "t3chat") {
    return t("BrowserCookiePlaceholderCurl");
  }
  return t("BrowserCookiePlaceholderDefault");
}

/**
 * Per-provider browser cookie management. Renders nothing for providers
 * that do not have a cookieDomain (i.e. don't authenticate via web cookies).
 */
export function CookieSection({ providerId, cookieDomain }: Props) {
  const { t } = useLocale();
  const dispatch = useDispatchAction();
  const [saved, setSaved] = useState<CookieInfoBridge | null>(null);
  const [loaded, setLoaded] = useState(false);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const [pasteValue, setPasteValue] = useState("");

  const reload = useCallback(async (signal: { stale: boolean }) => {
    try {
      const cookies = await getManualCookies();
      if (signal.stale) return;
      setSaved(cookies.find((c) => c.providerId === providerId) ?? null);
    } catch (err: unknown) {
      if (signal.stale) return;
      setError(err instanceof Error ? err.message : String(err));
    } finally {
      if (!signal.stale) setLoaded(true);
    }
  }, [providerId]);

  useEffect(() => {
    if (cookieDomain === null) return;
    const signal = { stale: false };
    setLoaded(false);
    setError(null);
    setPasteValue("");
    setSaved(null);
    void reload(signal);
    return () => {
      signal.stale = true;
    };
  }, [reload, cookieDomain, providerId, dispatch]);

  if (cookieDomain === null) return null;
  if (!loaded) return null;

  const handleRemove = async () => {
    setBusy(true);
    setError(null);
    try {
      await requireActionResult(
        dispatch({
          type: "removeManualCookie",
          target: { kind: "provider", providerId },
        }),
      );
      const next = await getManualCookies();
      setSaved(next.find((c) => c.providerId === providerId) ?? null);
    } catch (err: unknown) {
      setError(err instanceof Error ? err.message : String(err));
    } finally {
      setBusy(false);
    }
  };

  const handlePaste = async () => {
    if (!pasteValue.trim()) return;
    setBusy(true);
    setError(null);
    try {
      await requireActionResult(
        dispatch({
          type: "setManualCookie",
          target: { kind: "provider", providerId },
          cookieHeader: pasteValue.trim(),
        }),
      );
      const next = await getManualCookies();
      setSaved(next.find((c) => c.providerId === providerId) ?? null);
      setPasteValue("");
    } catch (err: unknown) {
      setError(err instanceof Error ? err.message : String(err));
    } finally {
      setBusy(false);
    }
  };

  return (
    <ProviderSection title={t("BrowserCookiesSectionTitle")}>
      {error && <ProviderStatusLine tone="error">{error}</ProviderStatusLine>}

      <ProviderAuthMethod
        className="provider-auth-method--cookie"
        title={t("ProviderLoginSectionTitle")}
        badge={
          saved ? t("BrowserCookieSavedBadge") : t("BrowserCookieNoneSaved")
        }
        badgeTone={saved ? "ok" : "unset"}
        meta={saved ? <span>{saved.savedAt}</span> : null}
        actions={
          saved ? (
            <button
              className="credential-btn credential-btn--danger"
              disabled={busy}
              onClick={() => void handleRemove()}
            >
              {t("BrowserCookieRemove")}
            </button>
          ) : null
        }
      >
        <p className="provider-detail-helper">{t("BrowserCookieImportHint")}</p>
        <details className="provider-detail-auth-advanced">
          <summary>{t("TabAdvanced")}</summary>
          <div className="credential-add-form">
          <textarea
            className="text-input credential-textarea"
            placeholder={cookiePlaceholder(providerId, t)}
            rows={3}
            value={pasteValue}
            onChange={(e) => setPasteValue(e.target.value)}
            disabled={busy}
          />
          <button
            className="credential-btn credential-btn--primary"
            disabled={busy || !pasteValue.trim()}
            onClick={() => void handlePaste()}
          >
            {t("BrowserCookieSave")}
          </button>
          </div>
        </details>
      </ProviderAuthMethod>
    </ProviderSection>
  );
}
