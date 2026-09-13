import { useCallback, useEffect, useState } from "react";
import type { ProviderTokenAccountsBridge } from "../../../types/bridge";
import { useLocale } from "../../../hooks/useLocale";
import {
  getTokenAccounts,
} from "../../../lib/tauri";
import { useDispatchAction } from "../../../core/useCoreBridge";
import { requireActionResult } from "../../../core/actionDispatcher";

interface Props {
  providerId: string;
  /**
   * When `true`, renders a tight collapsible variant suitable for embedding
   * inside the Providers → detail pane (Phase 6e inline surface).
   * When `false` (default), renders the full standalone-tab layout.
   */
  compact?: boolean;
  /** 移除账号前的确认回调（V5 卡片传入；独立页签不传则直接删除）。 */
  onConfirm?: (text: string, action: () => void) => void;
}

/**
 * Shared Token Accounts surface.
 *
 * Port of the token-account blocks in
 * `rust/src/native_ui/preferences.rs::render_provider_detail_panel`.
 * Wires the existing Phase-4 IPC (`get_token_accounts`,
 * `add_token_account`, `remove_token_account`, `set_active_token_account`).
 *
 * Used by:
 *   - `Settings.tsx::TokenAccountsTab` (compact=false, standalone tab)
 *   - `ProviderDetailPane.tsx` (compact=true, inline in detail pane)
 */
export function TokenAccountsPanel({
  providerId,
  compact = false,
  onConfirm,
}: Props) {
  const { t } = useLocale();
  const dispatch = useDispatchAction();
  const [data, setData] = useState<ProviderTokenAccountsBridge | null>(null);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [addLabel, setAddLabel] = useState("");
  const [addToken, setAddToken] = useState("");

  const load = useCallback(async () => {
    if (!providerId) {
      setData(null);
      return;
    }
    setBusy(true);
    setError(null);
    try {
      const next = await getTokenAccounts(providerId);
      setData(next);
    } catch (err: unknown) {
      setError(err instanceof Error ? err.message : String(err));
      setData(null);
    } finally {
      setBusy(false);
    }
  }, [providerId]);

  useEffect(() => {
    setAddLabel("");
    setAddToken("");
    setError(null);
    void load();
  }, [load]);

  const handleAdd = async () => {
    if (!providerId || !addLabel.trim() || !addToken.trim()) return;
    setBusy(true);
    setError(null);
    try {
      await requireActionResult(
        dispatch({
          type: "addTokenAccount",
          target: { kind: "provider", providerId },
          label: addLabel.trim(),
          token: addToken.trim(),
        }),
      );
      const next = await getTokenAccounts(providerId);
      setData(next);
      setAddLabel("");
      setAddToken("");
    } catch (err: unknown) {
      setError(err instanceof Error ? err.message : String(err));
    } finally {
      setBusy(false);
    }
  };

  const handleRemove = async (accountId: string) => {
    if (!providerId) return;
    setBusy(true);
    setError(null);
    try {
      await requireActionResult(
        dispatch({
          type: "removeTokenAccount",
          target: { kind: "provider", providerId },
          accountId,
        }),
      );
      const next = await getTokenAccounts(providerId);
      setData(next);
    } catch (err: unknown) {
      setError(err instanceof Error ? err.message : String(err));
    } finally {
      setBusy(false);
    }
  };

  const handleSetActive = async (accountId: string) => {
    if (!providerId) return;
    setBusy(true);
    setError(null);
    try {
      await requireActionResult(
        dispatch({
          type: "setActiveTokenAccount",
          target: { kind: "provider", providerId },
          accountId,
        }),
      );
      const next = await getTokenAccounts(providerId);
      setData(next);
    } catch (err: unknown) {
      setError(err instanceof Error ? err.message : String(err));
    } finally {
      setBusy(false);
    }
  };

  const handleProviderLogin = async () => {
    if (!providerId) return;
    setBusy(true);
    setError(null);
    try {
      await dispatch({
        type: "triggerLogin",
        target: { kind: "provider", providerId },
      });
      await load();
    } catch (err: unknown) {
      setError(err instanceof Error ? err.message : String(err));
    } finally {
      setBusy(false);
    }
  };

  if (!providerId) return null;

  const placeholder = data?.support.placeholder ?? t("TokenAccountPastePlaceholder");
  const subtitle = data?.support.subtitle ?? "";

  // V5 卡片内的中文占位：按服务商说明要粘贴的令牌种类（纯展示文案）。
  const tokenPlaceholderZh: Record<string, string> = {
    claude: "粘贴 sessionKey（浏览器 Cookie 里的会话令牌）",
    cursor: "粘贴 Cursor 的 Cookie 请求头",
    zai: "粘贴智谱 API 令牌",
  };
  const tokenPh =
    compact && tokenPlaceholderZh[providerId]
      ? tokenPlaceholderZh[providerId]
      : placeholder;

  const body = (
    <>
      {subtitle && !compact && (
        <p className="settings-section__hint">{subtitle}</p>
      )}

      {error && (
        <div className="settings-status settings-status--error">{error}</div>
      )}

      {!compact && (
        <h3 className="settings-section__title">
          {t("SectionSavedAccounts")}
        </h3>
      )}

      {data && data.accounts.length > 0 ? (
        <div className="s5-token-list">
          {data.accounts.map((acct) => (
            <div key={acct.id} className="s5-pd-row">
              <div>
                <div className="s5-field-label">
                  {acct.label}
                  {acct.isActive ? (
                    <span className="s5-login-ok">正在使用</span>
                  ) : null}
                </div>
                <div className="s5-field-help">
                  {t("TokenAccountAddedPrefix")} {acct.addedAt}
                  {acct.lastUsed
                    ? ` · ${t("TokenAccountUsedPrefix")} ${acct.lastUsed}`
                    : ""}
                </div>
              </div>
              <div className="s5-token-actions">
                {!acct.isActive && (
                  <button
                    type="button"
                    className="s5-ghost"
                    disabled={busy}
                    onClick={() => void handleSetActive(acct.id)}
                  >
                    {t("TokenAccountSetActive")}
                  </button>
                )}
                <button
                  type="button"
                  className="s5-ghost danger"
                  disabled={busy}
                  onClick={() => {
                    const action = () => void handleRemove(acct.id);
                    if (onConfirm) {
                      onConfirm(`删除账号「${acct.label}」？不会影响服务商账号本身。`, action);
                    } else {
                      action();
                    }
                  }}
                >
                  {t("TokenAccountRemove")}
                </button>
              </div>
            </div>
          ))}
        </div>
      ) : (
        <p className="s5-pd-lead">{t("TokenAccountEmpty")}</p>
      )}

      {!compact && (
        <h3 className="settings-section__title">{t("SectionAddAccount")}</h3>
      )}
      {providerId === "copilot" && (
        <button
          type="button"
          className="s5-primary"
          disabled={busy}
          onClick={() => void handleProviderLogin()}
        >
          {t("TokenAccountGithubLoginButton")}
        </button>
      )}
      <div className="s5-pd-field">
        <div className="s5-pd-field-head">
          <div className="s5-field-label">{t("SectionAddAccount")}</div>
        </div>
        <input
          className="s5-textin"
          type="text"
          placeholder={t("TokenAccountLabelPlaceholder")}
          value={addLabel}
          onChange={(e) => setAddLabel(e.target.value)}
          disabled={busy}
        />
        <textarea
          className="s5-area"
          placeholder={tokenPh}
          rows={compact ? 3 : 3}
          value={addToken}
          onChange={(e) => setAddToken(e.target.value)}
          disabled={busy}
        />
      </div>
      <div className="s5-actions">
        <button
          type="button"
          className="s5-primary"
          disabled={busy || !addLabel.trim() || !addToken.trim()}
          onClick={() => void handleAdd()}
        >
          {t("TokenAccountAddButton")}
        </button>
      </div>
    </>
  );

  if (compact) {
    const activeCount = data?.accounts.filter((a) => a.isActive).length ?? 0;
    const total = data?.accounts.length ?? 0;
    return (
      <details className="s5-token-disclosure">
        <summary className="s5-token-disclosure__summary">
          <span>{t("TokenAccountInlineSummary")}</span>
          <span className="s5-token-disclosure__count">
            {activeCount > 0 ? `正在使用 ${activeCount}/${total}` : `${total}`}
          </span>
          <span className="s5-token-disclosure__chevron" aria-hidden>
            ▾
          </span>
        </summary>
        <div className="s5-token-disclosure__body">{body}</div>
      </details>
    );
  }

  return (
    <section className="settings-section token-accounts-standalone">
      <h3 className="settings-section__title">{t("SectionTokenAccounts")}</h3>
      {body}
    </section>
  );
}
