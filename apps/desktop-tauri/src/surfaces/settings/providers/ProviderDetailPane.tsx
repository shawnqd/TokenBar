import { useCallback, useEffect, useState, type ReactNode } from "react";
import type { LocaleKey } from "../../../i18n/keys";
import type {
  CookieSourceOption,
  CredentialStorageStatus,
  ProviderAuthCapabilitiesBridge,
  ProviderDetail,
  ProviderUsageSnapshot,
  LocalUsagePeriod,
  RegionOption,
  SettingsSnapshot,
  SettingsUpdate,
} from "../../../types/bridge";
import { SegmentedControl } from "../../../components/FormControls";
import { resolveAuthEntries, type PrimaryAuthKind } from "./authEntries";
import { useLocale } from "../../../hooks/useLocale";
import {
  getCredentialStorageStatus,
  getProviderAuthCapabilities,
  getProviderCookieSourceOptions,
  getProviderDetail,
  getProviderRegionOptions,
  getTokenAccountProviders,
  openProviderDashboard,
  openProviderStatusPage,
  refreshProviders,
  revokeProviderCredentials,
  triggerProviderLogin,
} from "../../../lib/tauri";
import { listen } from "@tauri-apps/api/event";

import { IdentitySection } from "./sections/IdentitySection";
import { UsageSection } from "./sections/UsageSection";
import { PaceSection } from "./sections/PaceSection";
import { StatsSection } from "./sections/StatsSection";
import { QuickActionsSection } from "./sections/QuickActionsSection";
import { CookieSourceSection } from "./sections/CookieSourceSection";
import { RegionSection } from "./sections/RegionSection";
import {
  classifyProviderFetchIssue,
  providerFetchIssueLocaleKey,
} from "../../../lib/providerFetchIssue";
import { GeminiCliCreds } from "./sections/credentials/GeminiCliCreds";
import { VertexAiCreds } from "./sections/credentials/VertexAiCreds";
import { JetBrainsCreds } from "./sections/credentials/JetBrainsCreds";
import { KiroCreds } from "./sections/credentials/KiroCreds";
import { ClaudeCreds } from "./sections/credentials/ClaudeCreds";
import { OpenAiExtras } from "./sections/credentials/OpenAiExtras";
import { TokenAccountsPanel } from "../tokens/TokenAccountsPanel";
import { ApiKeySection } from "./ApiKeySection";
import { CookieSection } from "./CookieSection";
import { MenuBarMetricSection } from "./sections/MenuBarMetricSection";
import MenuCard from "../../../components/MenuCard";
import { useOutputSpeedSnapshot } from "../../../hooks/useOutputSpeedSnapshot";
import { outputSpeedProviderId } from "../../../lib/outputSpeed";
import type { QuotaDisplayContext } from "../../../lib/quotaDisplay";
import {
  ProviderAuthMethod,
  ProviderHeaderSkeleton,
  ProviderSection,
  ProviderSectionSkeleton,
  ProviderStatusLine,
} from "./shell/ProviderWorkspace";

interface Props {
  providerId: string | null;
  providerSnapshot?: ProviderUsageSnapshot | null;
  cookieDomain?: string | null;
  /**
   * The detail pane is a data preview, so per TASK-018 item B it follows the
   * display context of the component it previews — the dashboard cards.
   */
  display: QuotaDisplayContext;
  localUsagePeriod: LocalUsagePeriod;
  outputSpeedEnabled: boolean;
  providerMetrics: SettingsSnapshot["providerMetrics"];
  settingsDisabled: boolean;
  onSettingsChange: (patch: SettingsUpdate) => void;
}

/**
 * Orchestrates the Settings → Providers right-hand detail pane.
 *
 * Workspace order (empty blocks omit themselves):
 *   1. Provider header (fixed shell)
 *   2. Quick actions (sticky toolbar)
 *   3. Status / quota overview (subscription or balance variant)
 *   4. Recent data (stats / charts), plus Codex's local usage-history note
 *   5. Auth sources (primary expanded, secondary collapsed) — every entry is
 *      a real, capability-driven auth method; see `AuthWorkspace` below
 *   6. Display settings (own "Display" heading, TASK-021 item 2)
 */
export function ProviderDetailPane({
  providerId,
  providerSnapshot = null,
  cookieDomain = null,
  display,
  localUsagePeriod,
  outputSpeedEnabled,
  providerMetrics,
  settingsDisabled,
  onSettingsChange,
}: Props) {
  const { t } = useLocale();
  const [detail, setDetail] = useState<ProviderDetail | null>(null);
  const [cookieOptions, setCookieOptions] = useState<CookieSourceOption[]>([]);
  const [regionOptions, setRegionOptions] = useState<RegionOption[]>([]);
  const [credentialStatus, setCredentialStatus] =
    useState<CredentialStorageStatus | null>(null);
  // `null` means "unknown" (including: the backend command isn't registered
  // yet), which the entry list treats the same as "no OAuth/CLI sign-in" —
  // never as a reason to error the whole pane. See `getProviderAuthCapabilities`.
  const [authCapabilities, setAuthCapabilities] =
    useState<ProviderAuthCapabilitiesBridge | null>(null);
  const [credentialRevision, setCredentialRevision] = useState(0);
  const [tokenProviderIds, setTokenProviderIds] = useState<Set<string>>(
    () => new Set(),
  );
  // A selected provider always needs an async detail request on mount. Start
  // in the loading state so the first paint contains the fixed workspace
  // skeleton instead of an empty right pane that grows one frame later.
  const [loading, setLoading] = useState(() => Boolean(providerId));
  const [error, setError] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);
  const outputSpeed = useOutputSpeedSnapshot(outputSpeedEnabled);

  // Load the set of providers that support token accounts once.
  useEffect(() => {
    let cancelled = false;
    void getTokenAccountProviders()
      .then((list) => {
        if (!cancelled) {
          setTokenProviderIds(new Set(list.map((p) => p.providerId)));
        }
      })
      .catch(() => {
        // Non-fatal: inline token section will simply not render.
      });
    return () => {
      cancelled = true;
    };
  }, []);

  const load = useCallback(async (id: string, signal?: { stale: boolean }) => {
    setLoading(true);
    setError(null);
    try {
      const [next, cookieOpts, regionOpts, storageStatus] = await Promise.all([
        getProviderDetail(id),
        getProviderCookieSourceOptions(id),
        getProviderRegionOptions(id),
        getCredentialStorageStatus(),
      ]);
      if (signal?.stale) return;
      setDetail(next);
      setCookieOptions(cookieOpts);
      setRegionOptions(regionOpts);
      setCredentialStatus(storageStatus);
    } catch (e) {
      if (signal?.stale) return;
      setError(String(e));
      setDetail(null);
      setCookieOptions([]);
      setRegionOptions([]);
      setCredentialStatus(null);
    } finally {
      if (!signal?.stale) setLoading(false);
    }

    // Independent of the payload above: this only refines which auth entry
    // is expanded by default, so a rejection (e.g. the command is not yet
    // registered) must never surface as a pane-wide error.
    try {
      const capabilities = await getProviderAuthCapabilities(id);
      if (!signal?.stale) setAuthCapabilities(capabilities);
    } catch {
      if (!signal?.stale) setAuthCapabilities(null);
    }
  }, []);

  useEffect(() => {
    if (!providerId) {
      setDetail(null);
      setCookieOptions([]);
      setRegionOptions([]);
      setCredentialStatus(null);
      setAuthCapabilities(null);
      setError(null);
      setLoading(false);
      return;
    }
    // Keep the workspace skeleton; clear provider-specific payloads so the
    // previous provider never flashes under the new selection.
    setDetail(null);
    setCookieOptions([]);
    setRegionOptions([]);
    setCredentialStatus(null);
    setAuthCapabilities(null);
    setError(null);
    const signal = { stale: false };
    void load(providerId, signal);
    return () => {
      signal.stale = true;
    };
  }, [providerId, load]);

  // Live-refresh when a new snapshot lands for this provider.
  useEffect(() => {
    if (!providerId) return;
    const signal = { stale: false };
    const unlistenPromise = listen<{ providerId?: string }>(
      "provider-updated",
      (event) => {
        const pid = event.payload?.providerId;
        if (!pid || pid === providerId) {
          void load(providerId, signal);
        }
      },
    );
    return () => {
      signal.stale = true;
      void unlistenPromise.then((fn) => fn());
    };
  }, [providerId, load]);

  if (!providerId) {
    return (
      <div className="provider-detail">
        <div className="provider-detail-empty">
          {t("StateNoProviderSelected")}
        </div>
      </div>
    );
  }

  // A selected provider must never render an empty workspace while its first
  // detail request is in flight. The provider key remounts this pane on
  // selection changes, and this guard keeps the skeleton visible even if an
  // async callback briefly leaves `loading` false between state updates.
  const showSkeleton = Boolean(providerId) && !detail && !error;
  const dataLoading = loading || showSkeleton;
  const subtitle = detail ? buildSubtitle(detail, t) : "";
  const speedProviderId = detail ? outputSpeedProviderId(detail.id) : null;
  const providerOutputSpeed = speedProviderId
    ? outputSpeed?.[speedProviderId] ?? null
    : null;
  // The Settings → Providers preview shows a single, explicitly-selected
  // provider, so it always renders the full quota content regardless of the
  // menu-bar display mode (which only governs the "all providers" overview).
  const compactMetrics = false;

  const handleRefresh = async () => {
    if (!detail) return;
    setBusy(true);
    try {
      await refreshProviders();
    } catch (e) {
      setError(String(e));
    } finally {
      setBusy(false);
    }
  };

  const handleSwitchAccount = async () => {
    if (!detail) return;
    setBusy(true);
    try {
      await triggerProviderLogin(detail.id);
      setCredentialRevision((value) => value + 1);
      await refreshProviders();
      await load(detail.id);
    } catch (e) {
      setError(String(e));
    } finally {
      setBusy(false);
    }
  };

  const handleRevokeCredentials = async () => {
    if (!detail) return;
    setBusy(true);
    setError(null);
    try {
      await revokeProviderCredentials(detail.id);
      setCredentialRevision((value) => value + 1);
      await load(detail.id);
    } catch (e) {
      setError(String(e));
    } finally {
      setBusy(false);
    }
  };

  const handleOpenDashboard = () => {
    if (!detail) return;
    void openProviderDashboard(detail.id).catch((e) => setError(String(e)));
  };

  const handleOpenStatusPage = () => {
    if (!detail) return;
    void openProviderStatusPage(detail.id).catch((e) => setError(String(e)));
  };

  const handleCopyError = () => {
    if (detail?.lastError && navigator.clipboard) {
      void navigator.clipboard.writeText(detail.lastError);
    }
  };

  const handleBuyCredits = () => {
    if (detail?.buyCreditsUrl) {
      void openProviderDashboard(detail.id).catch((e) => setError(String(e)));
    }
  };

  const hasTokenAccounts = detail
    ? tokenProviderIds.has(detail.id)
    : false;

  return (
    <div
      className="provider-detail"
      data-loading={dataLoading ? "true" : "false"}
      data-provider-id={providerId}
    >
      {/* 1. Provider header — same shell whether or not a live snapshot exists */}
      {detail ? (
        <IdentitySection provider={detail} subtitle={subtitle} t={t} />
      ) : showSkeleton ? (
        <ProviderHeaderSkeleton />
      ) : error ? (
        <ProviderSection title={providerId}>
          <ProviderStatusLine tone="error">
            {t("StateError")}: {error}
          </ProviderStatusLine>
        </ProviderSection>
      ) : null}

      {detail?.lastError && (
        <ProviderIssueNotice
          detail={detail}
          message={detail.lastError}
          onCopy={handleCopyError}
          t={t}
        />
      )}

      {/* Keep provider actions visible before the long quota/credential stack. */}
      {detail && (
        <QuickActionsSection
          provider={detail}
          busy={busy}
          onRefresh={handleRefresh}
          onSwitchAccount={handleSwitchAccount}
          onOpenDashboard={handleOpenDashboard}
          onOpenStatusPage={handleOpenStatusPage}
          onCopyError={handleCopyError}
          onBuyCredits={handleBuyCredits}
          t={t}
        />
      )}

      {/* 2. Status & quota overview */}
      {providerSnapshot ? (
        <div className="provider-detail-live-card provider-detail-overview">
          {/* Live card keeps MenuCard metrics; its own header is hidden so the
              workspace IdentitySection is the single identity shell. */}
          <MenuCard
            provider={providerSnapshot}
            display={display}
            compactMetrics={compactMetrics}
            localUsagePeriod={localUsagePeriod}
            hideLocalUsage
          />
        </div>
      ) : detail ? (
        <>
          <UsageSection provider={detail} display={display} t={t} />
          <PaceSection pace={detail.pace} t={t} />
        </>
      ) : showSkeleton ? (
        <ProviderSectionSkeleton title={t("ProviderUsage")} />
      ) : null}

      {/* 3. Recent data */}
      {detail && (
        <StatsSection
          providerId={detail.id}
          accountEmail={detail.email}
          speed={providerOutputSpeed}
          cost={detail.cost}
          localUsagePeriod={localUsagePeriod}
        />
      )}

      {/* Codex-only local usage-history note. This used to live inside the
          auth-sources zone (it is not an auth source), rendered next to
          StatsSection where usage history belongs — TASK-021 item 2. */}
      {detail?.id === "codex" && <CodexUsageHistoryNote t={t} />}

      {/* 4. Auth sources — Cookie / browser login / CLI / API / token plan in one zone */}
      {detail && (
        <AuthWorkspace
          providerId={detail.id}
          dashboardUrl={detail.dashboardUrl}
          cookieDomain={cookieDomain}
          capabilities={authCapabilities}
          credentialRevision={credentialRevision}
          hasTokenAccounts={hasTokenAccounts}
          credentialStatus={credentialStatus}
          busy={busy}
          cookieSource={detail.cookieSource}
          cookieOptions={cookieOptions}
          onCookieSourceChanged={() => void load(detail.id)}
          onRevoke={handleRevokeCredentials}
          onSignIn={handleSwitchAccount}
          t={t}
        />
      )}

      {/* 5. The provider's remaining per-provider settings: which quota metric
          its tray icon shows, and its region. Titled 其他相关设置 rather than
          显示 — "显示" is a *tab* name, and a section sharing it read as though
          the whole Display page had been embedded here. Neither control has a
          home on the per-component pages: both are per-provider. */}
      {detail && (
        <div className="provider-detail-display-zone settings-section">
          <h3 className="settings-section__title">{t("ProviderOtherSettings")}</h3>
          <MenuBarMetricSection
            provider={detail}
            providerMetrics={providerMetrics}
            disabled={settingsDisabled}
            t={t}
            onChange={onSettingsChange}
          />
          <RegionSection
            providerId={detail.id}
            currentValue={detail.region}
            options={regionOptions}
            t={t}
            onChanged={() => void load(detail.id)}
          />
        </div>
      )}

    </div>
  );
}

/**
 * Provider ids with a hand-built credentials component (see
 * `CredentialsDispatcher`). This is a frontend routing table, not a
 * capability — Rust has no notion of which React component renders a
 * provider's controls — so it stays a small explicit list.
 */
function hasBespokeCredentials(providerId: string): boolean {
  switch (providerId) {
    case "gemini":
    case "vertexai":
    case "jetbrains":
    case "kiro":
    case "claude":
    case "openaiapi":
    case "litellm":
    case "devin":
    case "opencodego":
    case "zed":
    case "sub2api":
    case "wayfinder":
      return true;
    default:
      return false;
  }
}

function AuthWorkspace({
  providerId,
  dashboardUrl,
  cookieDomain,
  capabilities,
  credentialRevision,
  hasTokenAccounts,
  credentialStatus,
  busy,
  cookieSource,
  cookieOptions,
  onCookieSourceChanged,
  onRevoke,
  onSignIn,
  t,
}: {
  providerId: string;
  dashboardUrl: string | null;
  cookieDomain: string | null;
  capabilities: ProviderAuthCapabilitiesBridge | null;
  credentialRevision: number;
  hasTokenAccounts: boolean;
  credentialStatus: CredentialStorageStatus | null;
  busy: boolean;
  cookieSource: string | null | undefined;
  cookieOptions: CookieSourceOption[];
  onCookieSourceChanged: () => void;
  onRevoke: () => void;
  onSignIn: () => void;
  t: ReturnType<typeof useLocale>["t"];
}) {
  // Every branch of this decision lives in `authEntries.ts` so it can be
  // tested — this zone is the whole of item 2 and had no coverage at all.
  const isBespoke = hasBespokeCredentials(providerId);
  const supportsOAuth = capabilities?.supportsOAuth ?? false;
  const supportsCli = capabilities?.supportsCli ?? false;
  const {
    availability,
    primary,
    showApiKey,
    methods,
    showCookieSource: codexAllowsSource,
  } = resolveAuthEntries({
      providerId,
      cookieDomain,
      dashboardUrl,
      capabilities,
      isBespoke,
    });
  const showCookie = availability.cookie;
  const showSignIn = availability.signIn;
  // The options list is a runtime fact rather than a capability, so it stays
  // here rather than moving into the pure decision.
  const showCookieSource = codexAllowsSource && cookieOptions.length > 0;

  const bespokeNode = isBespoke ? (
    <CredentialsDispatcher key={`creds-${providerId}`} providerId={providerId} t={t} />
  ) : null;
  const cookieNode = showCookie ? (
    <CookieSection
      key={`cookie-${providerId}-${credentialRevision}`}
      providerId={providerId}
      cookieDomain={cookieDomain}
    />
  ) : null;
  const signInNode = showSignIn ? (
    <ProviderSignInEntry
      key={`signin-${providerId}`}
      supportsOAuth={supportsOAuth}
      supportsCli={supportsCli}
      busy={busy}
      onSignIn={onSignIn}
      t={t}
    />
  ) : null;
  const apiKeyNode = (
    <ApiKeySection
      key={`api-${providerId}-${credentialRevision}`}
      providerId={providerId}
    />
  );

  // One method at a time. A provider is authenticated *one* way — cookies or a
  // browser sign-in or a CLI or an API key — so these are mutually exclusive
  // choices, and a segmented control is what a mutually exclusive choice looks
  // like. The zone used to render a lead card plus a collapsed "其他认证方式"
  // drawer, which implied a hierarchy that does not exist and hid a provider's
  // only real option behind a disclosure triangle.
  const nodeFor: Record<PrimaryAuthKind, ReactNode> = {
    bespoke: bespokeNode,
    cookie: cookieNode,
    signIn: signInNode,
    apiKey: apiKeyNode,
  };
  const labelFor: Record<PrimaryAuthKind, LocaleKey> = {
    bespoke: "CredentialsSectionTitle",
    cookie: "CredentialManualCookies",
    // Not the bare "OAuth": the entry covers a browser sign-in *and* a CLI
    // login, and "OAuth" names a protocol rather than the thing the user does.
    signIn: "ProviderAuthMethodSignIn",
    apiKey: "CredentialApiKeys",
  };

  // Reset to the provider's own default when the pane switches providers —
  // "cookie" selected on one provider means nothing on the next, and may not
  // even be offered there.
  const [chosen, setChosen] = useState<PrimaryAuthKind>(primary);
  useEffect(() => setChosen(primary), [primary, providerId]);
  const method = methods.includes(chosen) ? chosen : primary;

  return (
    <div className="provider-detail-auth-zone" data-auth-sources="true">
      <div className="provider-detail-section__header provider-detail-auth-zone__header">
        <h3 className="provider-detail-auth-zone__title">
          {t("ProviderAuthSourcesTitle")}
        </h3>
        {/* Says what is true of *this* provider rather than listing every
            method the app knows about. The old helper — "supports Cookie,
            browser login, CLI, API Key and Token Plan; only supported ones are
            shown" — named five things without saying which applied here, so it
            answered nothing and still had to be read. */}
        <p className="provider-detail-auth-zone__helper">
          {methods.length > 1
            ? t("ProviderAuthPickMethod")
            : t("ProviderAuthUsingMethod").replace("{}", t(labelFor[method]))}
        </p>
      </div>
      {/* One option is not a choice. A single-method provider — Codex, whose
          only route is a browser sign-in — gets its controls directly, with no
          control that can only be set to what it already is; the line above
          has already named the method. */}
      {methods.length > 1 && (
        <SegmentedControl
          value={method}
          options={methods.map((kind) => ({
            value: kind,
            label: t(labelFor[kind]),
          }))}
          onChange={(value) => setChosen(value as PrimaryAuthKind)}
        />
      )}
      <div className="provider-detail-auth-primary">{nodeFor[method]}</div>
      {/* Preferred source mode (auto / web / cli / …) stays with credentials,
          not under Display — TASK-021 item 2. Shown under the cookie method
          because that is the only one it governs. */}
      {showCookieSource && method === "cookie" && (
        <CookieSourceSection
          providerId={providerId}
          currentValue={cookieSource ?? null}
          options={cookieOptions}
          t={t}
          onChanged={onCookieSourceChanged}
        />
      )}
      {/* Below the picker, not inside it: neither is an authentication
          *method*. Token accounts are a roster the chosen method fills, and
          credential storage is a status readout plus the revoke action. */}
      {hasTokenAccounts && (
        <TokenAccountsPanel
          key={`token-${providerId}-${credentialRevision}`}
          providerId={providerId}
          compact
        />
      )}
      <CredentialStorageSection
        status={credentialStatus}
        busy={busy}
        onRevoke={onRevoke}
        t={t}
      />
    </div>
  );
}

/**
 * Generic OAuth/CLI sign-in entry. Reuses the exact same
 * `trigger_provider_login` command the "切换账号…" quick action already
 * calls (`handleSwitchAccount` in `ProviderDetailPane`) — this only gives it
 * a second, labelled home inside the auth-sources zone so providers whose
 * only real auth method is "open the browser and sign in" (Codex, GitHub
 * Copilot, …) are not left with an empty or misleading primary card.
 */
function ProviderSignInEntry({
  supportsOAuth,
  supportsCli,
  busy,
  onSignIn,
  t,
}: {
  supportsOAuth: boolean;
  supportsCli: boolean;
  busy: boolean;
  onSignIn: () => void;
  t: ReturnType<typeof useLocale>["t"];
}) {
  const methodLabels = [
    supportsOAuth ? t("OAuth") : null,
    supportsCli ? t("ProviderSourceCliShort") : null,
  ].filter((label): label is string => Boolean(label));

  return (
    <ProviderSection title={t("CredentialsSectionTitle")}>
      <ProviderAuthMethod
        title={methodLabels.length > 0 ? methodLabels.join(" / ") : t("CredentialsSectionTitle")}
        actions={
          <button
            type="button"
            className="credential-btn credential-btn--primary"
            disabled={busy}
            onClick={onSignIn}
          >
            {t("ActionSwitchAccount")}
          </button>
        }
      />
    </ProviderSection>
  );
}

/**
 * Codex-only local usage-history note (moved out of the auth-sources zone —
 * TASK-021 item 2). Both strings are unchanged upstream copy; only their
 * location changed, since local history tracking is not an auth source.
 */
function CodexUsageHistoryNote({
  t,
}: {
  t: ReturnType<typeof useLocale>["t"];
}) {
  return (
    <ProviderSection title={t("ProviderHistoricalTracking")}>
      <div className="provider-detail-helper">{t("ProviderCodexHistoryHelp")}</div>
      <div className="provider-detail-helper">{t("CredsOpenAiHistoryHelp")}</div>
    </ProviderSection>
  );
}

function ProviderIssueNotice({
  detail,
  message,
  onCopy,
  t,
}: {
  detail: ProviderDetail;
  message: string;
  onCopy: () => void;
  t: ReturnType<typeof useLocale>["t"];
}) {
  const cleaned = message.replace(/^last fetch failed:\s*/i, "").trim();
  const issue = classifyProviderFetchIssue(cleaned);
  const needsLogin = issue.category === "auth";
  const title = needsLogin
    ? `${detail.displayName} ${t("ProviderIssueNeedsSignIn")}`
    : t("ProviderIssueFetchNeedsAttention");
  const displayMessage = localizeProviderIssue(cleaned, t);
  const categoryLabel = t(providerFetchIssueLocaleKey(issue.category));

  return (
    <div
      className="provider-detail-error"
      role="status"
      data-issue-category={issue.category}
      data-issue-transient={issue.mayBeTransient ? "true" : "false"}
    >
      <div className="provider-detail-error__header">
        <strong>{title}</strong>
        <button
          type="button"
          className="provider-detail-error__copy"
          onClick={onCopy}
        >
          {t("ProviderIssueCopy")}
        </button>
      </div>
      <p className="provider-detail-error__category">{categoryLabel}</p>
      <p>{displayMessage}</p>
    </div>
  );
}

function localizeProviderIssue(
  message: string,
  t: ReturnType<typeof useLocale>["t"],
): string {
  const lower = message.toLowerCase();
  const requestUrl = message.match(
    /^network error:\s*error sending request for url\s*\((https?:\/\/[^\s)]+)\)\s*$/i,
  );
  if (requestUrl) {
    return t("ProviderIssueNetworkRequestFailed") + "：" + requestUrl[1];
  }
  if (lower.startsWith("network error:")) {
    // The raw diagnostic remains available through the existing Copy action.
    // The visible message stays in the selected UI language.
    return t("ProviderIssueNetworkConnectionFailed");
  }
  if (
    lower.includes("oauth credentials not found") ||
    lower.includes("sign-in was not found") ||
    lower.includes("sign-in expired") ||
    lower === "authentication required"
  ) {
    return t("ProviderIssueSignInRequired");
  }
  const unsupported = message.match(
    /^Source mode `?([^`']+)`? not supported for this provider$/i,
  );
  if (unsupported) {
    return `${t("ProviderIssueUnsupportedSourceModePrefix")} (${unsupported[1]})`;
  }
  return message;
}

function CredentialStorageSection({
  status,
  busy,
  onRevoke,
  t,
}: {
  status: CredentialStorageStatus | null;
  busy: boolean;
  onRevoke: () => void;
  t: ReturnType<typeof useLocale>["t"];
}) {
  if (!status) return null;

  return (
    <section className="provider-detail-section provider-detail-credential-storage provider-detail-section--nested">
      <div className="provider-detail-section__header">
        <h4>{t("CredentialStorageTitle")}</h4>
        <button
          className="credential-btn credential-btn--danger"
          disabled={busy}
          onClick={onRevoke}
        >
          {t("CredentialRevokeStored")}
        </button>
      </div>
      <dl className="provider-detail-grid provider-detail-grid--storage">
        <dt>{t("CredentialApiKeys")}</dt>
        <dd>{storageLabel(status.apiKeys, t)}</dd>
        <dt>{t("CredentialManualCookies")}</dt>
        <dd>{storageLabel(status.manualCookies, t)}</dd>
        <dt>{t("CredentialTokenAccounts")}</dt>
        <dd>{storageLabel(status.tokenAccounts, t)}</dd>
      </dl>
    </section>
  );
}

function storageLabel(
  value: string,
  t: ReturnType<typeof useLocale>["t"],
): string {
  if (value.startsWith("protected:")) {
    return `${t("CredentialProtectedPrefix")} (${value.slice("protected:".length)})`;
  }
  switch (value) {
    case "missing":
      return t("CredentialStatusNotCreated");
    case "plaintext":
      return t("CredentialStatusPlaintext");
    case "unavailable":
      return t("CredentialStatusUnavailable");
    case "unreadable":
      return t("CredentialStatusUnreadable");
    default:
      return value;
  }
}

function buildSubtitle(
  detail: ProviderDetail,
  t: (k: Parameters<ReturnType<typeof useLocale>["t"]>[0]) => string,
): string {
  const parts: string[] = [];
  if (detail.sourceLabel) parts.push(detail.sourceLabel);
  if (detail.lastUpdated) {
    const ago = relativeAgo(detail.lastUpdated);
    if (ago) parts.push(`${t("DetailUpdatedPrefix")} ${ago}`);
  } else if (!detail.hasSnapshot) {
    parts.push(t("ProviderUsageNotFetchedYet"));
  }
  return parts.join(" · ");
}

function relativeAgo(iso: string): string | null {
  const t = new Date(iso).getTime();
  if (!Number.isFinite(t)) return null;
  const diff = Date.now() - t;
  const secs = Math.round(Math.abs(diff) / 1000);
  if (secs < 60) return `${secs}s`;
  const mins = Math.round(secs / 60);
  if (mins < 60) return `${mins}m`;
  const hrs = Math.round(mins / 60);
  if (hrs < 24) return `${hrs}h`;
  return `${Math.round(hrs / 24)}d`;
}

/**
 * Dispatch the appropriate Phase-6d credential component based on the
 * current provider. Providers without a bespoke credentials UI render
 * nothing. Mirrors the `provider_id == ProviderId::*` chain in
 * `rust/src/native_ui/preferences.rs::render_provider_detail_panel`.
 */
function CredentialsDispatcher({
  providerId,
  t,
}: {
  providerId: string;
  t: ReturnType<typeof useLocale>["t"];
}) {
  switch (providerId) {
    case "gemini":
      return <GeminiCliCreds providerId={providerId} t={t} />;
    case "vertexai":
      return <VertexAiCreds providerId={providerId} t={t} />;
    case "jetbrains":
      return <JetBrainsCreds t={t} />;
    case "kiro":
      return <KiroCreds t={t} />;
    case "claude":
      return <ClaudeCreds t={t} />;
    case "openaiapi":
      return <OpenAiExtras providerId={providerId} t={t} />;
    case "litellm":
    case "devin":
    case "opencodego":
    case "zed":
    case "sub2api":
    case "wayfinder":
      return <OpenAiExtras providerId={providerId} t={t} />;
    default:
      return null;
  }
}
