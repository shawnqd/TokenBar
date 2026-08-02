import { useCallback, useEffect, useState } from "react";
import {
  captureProviderLogin,
  closeProviderLogin,
  getManualCookies,
  openProviderLogin,
  removeManualCookie,
  setManualCookie,
} from "../../../lib/tauri";
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
  const [saved, setSaved] = useState<CookieInfoBridge | null>(null);
  const [loaded, setLoaded] = useState(false);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const [pasteValue, setPasteValue] = useState("");
  // Whether a sign-in window is open for this provider. The capture step is a
  // deliberate button rather than a poll: only the user knows when the
  // provider's own flow — SSO, a second factor, an org picker — is finished,
  // and guessing would store a half-authenticated session.
  const [loginOpen, setLoginOpen] = useState(false);
  const [notice, setNotice] = useState<string | null>(null);

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
    setNotice(null);
    setPasteValue("");
    setSaved(null);
    // Switching providers must not leave a window open that is signed in to
    // the provider the user just navigated away from.
    setLoginOpen(false);
    void closeProviderLogin().catch(() => {});
    void reload(signal);
    return () => {
      signal.stale = true;
    };
  }, [reload, cookieDomain, providerId]);

  if (cookieDomain === null) return null;
  if (!loaded) return null;

  const handleRemove = async () => {
    setBusy(true);
    setError(null);
    try {
      const next = await removeManualCookie(providerId);
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
    setNotice(null);
    try {
      const next = await setManualCookie(providerId, pasteValue.trim());
      setSaved(next.find((c) => c.providerId === providerId) ?? null);
      setPasteValue("");
    } catch (err: unknown) {
      setError(err instanceof Error ? err.message : String(err));
    } finally {
      setBusy(false);
    }
  };

  const handleOpenLogin = async () => {
    setBusy(true);
    setError(null);
    setNotice(null);
    try {
      await openProviderLogin(providerId);
      setLoginOpen(true);
    } catch (err: unknown) {
      setError(err instanceof Error ? err.message : String(err));
    } finally {
      setBusy(false);
    }
  };

  const handleCaptureLogin = async () => {
    setBusy(true);
    setError(null);
    setNotice(null);
    try {
      const next = await captureProviderLogin(providerId);
      setSaved(next.find((c) => c.providerId === providerId) ?? null);
      setLoginOpen(false);
      setNotice(t("ProviderLoginCaptured"));
    } catch (err: unknown) {
      // The window stays open on failure — the usual cause is that sign-in
      // has not finished yet, and closing it would throw away the progress.
      setError(err instanceof Error ? err.message : String(err));
    } finally {
      setBusy(false);
    }
  };

  const handleCloseLogin = async () => {
    setBusy(true);
    try {
      await closeProviderLogin();
      setLoginOpen(false);
    } catch (err: unknown) {
      setError(err instanceof Error ? err.message : String(err));
    } finally {
      setBusy(false);
    }
  };

  return (
    <ProviderSection title={t("BrowserCookiesSectionTitle")}>
      {error && <ProviderStatusLine tone="error">{error}</ProviderStatusLine>}
      {notice && <ProviderStatusLine tone="ok">{notice}</ProviderStatusLine>}

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
        <p className="provider-detail-helper">{t("ProviderLoginHint")}</p>
        <div className="provider-login__actions">
          <button
            className="credential-btn credential-btn--primary"
            disabled={busy}
            onClick={() => void handleOpenLogin()}
          >
            {t("ProviderLoginOpen")}
          </button>
          {loginOpen && (
            <>
              <button
                className="credential-btn credential-btn--primary"
                disabled={busy}
                onClick={() => void handleCaptureLogin()}
              >
                {t("ProviderLoginCapture")}
              </button>
              <button
                className="credential-btn"
                disabled={busy}
                onClick={() => void handleCloseLogin()}
              >
                {t("ProviderLoginClose")}
              </button>
            </>
          )}
        </div>

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
      </ProviderAuthMethod>
    </ProviderSection>
  );
}
