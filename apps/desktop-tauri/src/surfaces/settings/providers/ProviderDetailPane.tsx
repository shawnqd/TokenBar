import { useCallback, useEffect, useState, type ReactNode } from "react";
import type {
  CookieSourceOption,
  CredentialStorageStatus,
  ProviderDetail,
  ProviderUsageSnapshot,
  LocalUsagePeriod,
  RegionOption,
  SettingsSnapshot,
  SettingsUpdate,
} from "../../../types/bridge";
import { useLocale } from "../../../hooks/useLocale";
import {
  getCredentialStorageStatus,
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
 *   4. Recent data (stats / charts)
 *   5. Auth & credentials (primary expanded, secondary collapsed)
 *   6. Display settings
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
  }, []);

  useEffect(() => {
    if (!providerId) {
      setDetail(null);
      setCookieOptions([]);
      setRegionOptions([]);
      setCredentialStatus(null);
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

      {/* 4. Auth sources — Cookie / browser login / CLI / API / token plan in one zone */}
      {detail && (
        <AuthWorkspace
          providerId={detail.id}
          cookieDomain={cookieDomain}
          credentialRevision={credentialRevision}
          hasTokenAccounts={hasTokenAccounts}
          credentialStatus={credentialStatus}
          busy={busy}
          primary={resolvePrimaryAuth(detail.id, cookieDomain)}
          cookieSource={detail.cookieSource}
          cookieOptions={cookieOptions}
          onCookieSourceChanged={() => void load(detail.id)}
          onRevoke={handleRevokeCredentials}
          t={t}
        />
      )}

      {/* 5. Display settings (tray metric + region only; auth sources live above) */}
      {detail && (
        <div className="provider-detail-display-zone">
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

type PrimaryAuthKind = "bespoke" | "cookie" | "apiKey";

/**
 * Pick the auth surface that should stay expanded for this provider family.
 * Remaining methods collapse under "Other authentication methods".
 */
function resolvePrimaryAuth(
  providerId: string,
  cookieDomain: string | null,
): PrimaryAuthKind {
  if (hasBespokeCredentials(providerId)) return "bespoke";
  if (cookieDomain) return "cookie";
  return "apiKey";
}

function hasBespokeCredentials(providerId: string): boolean {
  switch (providerId) {
    case "gemini":
    case "vertexai":
    case "jetbrains":
    case "kiro":
    case "claude":
    case "codex":
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
  cookieDomain,
  credentialRevision,
  hasTokenAccounts,
  credentialStatus,
  busy,
  primary,
  cookieSource,
  cookieOptions,
  onCookieSourceChanged,
  onRevoke,
  t,
}: {
  providerId: string;
  cookieDomain: string | null;
  credentialRevision: number;
  hasTokenAccounts: boolean;
  credentialStatus: CredentialStorageStatus | null;
  busy: boolean;
  primary: PrimaryAuthKind;
  cookieSource: string | null | undefined;
  cookieOptions: CookieSourceOption[];
  onCookieSourceChanged: () => void;
  onRevoke: () => void;
  t: ReturnType<typeof useLocale>["t"];
}) {
  const showCookie = cookieDomain !== null && providerId !== "codex";
  const showCookieSource = providerId !== "codex" && cookieOptions.length > 0;
  const bespoke = (
    <CredentialsDispatcher key={`creds-${providerId}`} providerId={providerId} t={t} />
  );
  const cookie = showCookie ? (
    <CookieSection
      key={`cookie-${providerId}-${credentialRevision}`}
      providerId={providerId}
      cookieDomain={cookieDomain}
    />
  ) : null;
  const apiKey = (
    <ApiKeySection
      key={`api-${providerId}-${credentialRevision}`}
      providerId={providerId}
    />
  );

  const primaryNode: ReactNode =
    primary === "bespoke"
      ? bespoke
      : primary === "cookie"
        ? cookie
        : apiKey;

  const secondaryNodes: ReactNode[] = [];
  if (primary !== "bespoke") secondaryNodes.push(bespoke);
  if (primary !== "cookie" && cookie) secondaryNodes.push(cookie);
  if (primary !== "apiKey") secondaryNodes.push(apiKey);
  if (hasTokenAccounts) {
    secondaryNodes.push(
      <TokenAccountsPanel
        key={`token-${providerId}-${credentialRevision}`}
        providerId={providerId}
        compact
      />,
    );
  }
  secondaryNodes.push(
    <CredentialStorageSection
      key={`storage-${providerId}`}
      status={credentialStatus}
      busy={busy}
      onRevoke={onRevoke}
      t={t}
    />,
  );

  return (
    <div className="provider-detail-auth-zone" data-auth-sources="true">
      <div className="provider-detail-section__header provider-detail-auth-zone__header">
        <h3 className="provider-detail-auth-zone__title">
          {t("ProviderAuthSourcesTitle")}
        </h3>
        <p className="provider-detail-auth-zone__helper">
          {t("ProviderAuthSourcesHelper")}
        </p>
      </div>
      {/* Preferred source mode (auto / web / cli / …) stays with credentials,
          not under Display — TASK-021 item 2. */}
      {showCookieSource && (
        <CookieSourceSection
          providerId={providerId}
          currentValue={cookieSource ?? null}
          options={cookieOptions}
          t={t}
          onChanged={onCookieSourceChanged}
        />
      )}
      <div className="provider-detail-auth-primary">{primaryNode}</div>
      <details className="provider-detail-section provider-detail-auth-more">
        <summary className="provider-detail-auth-more__summary">
          {t("ProviderAuthOtherMethods")}
        </summary>
        <div className="provider-detail-auth-more__body">{secondaryNodes}</div>
      </details>
    </div>
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
    case "codex":
      return <OpenAiExtras providerId={providerId} t={t} />;
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
