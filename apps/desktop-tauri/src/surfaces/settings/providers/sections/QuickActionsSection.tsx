import type { ProviderDetail } from "../../../../types/bridge";
import type { LocaleKey } from "../../../../i18n/keys";

interface Props {
  provider: ProviderDetail;
  busy: boolean;
  onRefresh: () => void;
  onSwitchAccount: () => void;
  onOpenDashboard: () => void;
  onOpenStatusPage: () => void;
  onCopyError: () => void;
  onBuyCredits: () => void;
  t: (key: LocaleKey) => string;
}

/**
 * Quick-action toolbar. Mirrors the six buttons in
 * `rust/src/native_ui/preferences.rs::render_provider_detail_panel`.
 * Buttons that have no backing URL on the provider are omitted entirely
 * (egui parity: the button only renders when the action is meaningful).
 */
export function QuickActionsSection({
  provider,
  busy,
  onRefresh,
  onSwitchAccount,
  onOpenDashboard,
  onOpenStatusPage,
  onCopyError,
  onBuyCredits,
  t,
}: Props) {
  return (
    <section
      className="provider-detail-section provider-detail-quick-actions"
      aria-label={t("QuickActions")}
    >
      <div className="provider-detail-quick-actions__header">
        <h4>{t("QuickActions")}</h4>
      </div>
      <div className="provider-detail-actions provider-detail-actions--toolbar">
        <button
          type="button"
          className="credential-btn credential-btn--primary"
          onClick={onRefresh}
          disabled={busy}
        >
          {/* Clicking this used to give no sign it had registered: the button
              greys out, but a disabled button is also what a broken one looks
              like. The glyph turns while the fetch is in flight. */}
          <span className={busy ? "spin" : undefined} aria-hidden>
            ↻
          </span>
          {t("ActionRefresh")}
        </button>
        {provider.dashboardUrl && (
          <button
            type="button"
            className="btn btn--ghost"
            onClick={onSwitchAccount}
            disabled={busy}
          >
            {t("ActionSwitchAccount")}
          </button>
        )}
        {provider.dashboardUrl && (
          <button
            type="button"
            className="btn btn--ghost"
            onClick={onOpenDashboard}
          >
            {t("ActionUsageDashboard")}
          </button>
        )}
        {provider.statusPageUrl && (
          <button
            type="button"
            className="btn btn--ghost"
            onClick={onOpenStatusPage}
          >
            {t("ActionStatusPage")}
          </button>
        )}
        {provider.lastError && (
          <button
            type="button"
            className="btn btn--ghost"
            onClick={onCopyError}
          >
            {t("ActionCopyError")}
          </button>
        )}
        {provider.buyCreditsUrl && (
          <button
            type="button"
            className="btn btn--ghost"
            onClick={onBuyCredits}
          >
            {t("ActionBuyCredits")}
          </button>
        )}
      </div>
    </section>
  );
}
