import { useCallback, useEffect, useState } from "react";
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
import {
  mapBespokeToUserKind,
  resolveAuthEntries,
  userFacingAuthMethods,
  type UserAuthKind,
} from "./authEntries";
import { useLocale } from "../../../hooks/useLocale";
import {
  getCredentialStorageStatus,
  getProviderAuthCapabilities,
  getProviderCookieSourceOptions,
  getProviderDetail,
  getProviderRegionOptions,
  getTokenAccountProviders,
} from "../../../lib/tauri";
import { useDispatchAction } from "../../../core/useCoreBridge";
import { requireActionResult } from "../../../core/actionDispatcher";
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
import type { ProviderSnapshot } from "../../../core/snapshot";
import { projectSurface } from "../../../core/projection";
import type { ProviderChartData } from "../../../types/bridge";
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
  /** Unified core snapshot. When present, the overview card and stats read
   *  from this projection; chart/usage data is fetched via the injected
   *  enrichment loader without fabricating values. */
  coreSnapshot?: ProviderSnapshot | null;
  /** Declarative chart loader for enrichment. Falls back to Tauri invoke when
   *  not supplied (legacy path). */
  chartLoader?: (
    providerId: string,
    accountEmail?: string,
  ) => Promise<ProviderChartData | null>;
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
  coreSnapshot = null,
  chartLoader,
  cookieDomain = null,
  display,
  localUsagePeriod,
  outputSpeedEnabled,
  providerMetrics,
  settingsDisabled,
  onSettingsChange,
}: Props) {
  const { t } = useLocale();
  const dispatch = useDispatchAction();
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
  const [authCapabilitiesState, setAuthCapabilitiesState] = useState<
    "loading" | "ready" | "error"
  >("loading");
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
      if (!signal?.stale) {
        setAuthCapabilities(capabilities);
        setAuthCapabilitiesState("ready");
      }
    } catch {
      if (!signal?.stale) {
        setAuthCapabilities(null);
        setAuthCapabilitiesState("error");
      }
    }
  }, []);

  useEffect(() => {
    if (!providerId) {
      setDetail(null);
      setCookieOptions([]);
      setRegionOptions([]);
      setCredentialStatus(null);
      setAuthCapabilities(null);
      setAuthCapabilitiesState("loading");
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
    setAuthCapabilitiesState("loading");
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
      await dispatch({ type: "refresh" });
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
      await dispatch({
        type: "triggerLogin",
        target: { kind: "provider", providerId: detail.id },
      });
      setCredentialRevision((value) => value + 1);
      await dispatch({ type: "refresh" });
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
      await requireActionResult(
        dispatch({
          type: "revokeCredentials",
          target: { kind: "provider", providerId: detail.id },
        }),
      );
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
    void dispatch({
      type: "openExternalUsage",
      target: { kind: "provider", providerId: detail.id },
    }).catch((e) => setError(String(e)));
  };

  const handleOpenStatusPage = () => {
    if (!detail) return;
    void dispatch({
      type: "openExternalStatus",
      target: { kind: "provider", providerId: detail.id },
    }).catch((e) => setError(String(e)));
  };

  const handleCopyError = () => {
    if (detail?.lastError && navigator.clipboard) {
      void navigator.clipboard.writeText(detail.lastError);
    }
  };

  const handleBuyCredits = () => {
    if (detail?.buyCreditsUrl) {
      void dispatch({
        type: "openExternalUsage",
        target: { kind: "provider", providerId: detail.id },
      }).catch((e) => setError(String(e)));
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
        <IdentitySection
          provider={detail}
          subtitle={subtitle}
          t={t}
          actions={
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
          }
        />
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

      {/* 2. Status & quota overview — unified core projection path.
          When a core snapshot is present the overview reads the same store as
          tray/floatbar/taskbar via `projectSurface`; chart/usage data is
          fetched via the injected `chartLoader` (enrichment scheduler) with
          loading/empty states, never a fabricated 100%. */}
      {providerSnapshot || coreSnapshot ? (
        <div className="provider-detail-live-card provider-detail-overview">
          {/* Live card keeps MenuCard metrics; its own header is hidden so the
              workspace IdentitySection is the single identity shell. */}
          <MenuCard
            provider={
              providerSnapshot ??
              // Synthetic bridge fallback so MenuCard header still has a name
              // when only the core snapshot is available; quota windows are
              // driven by `coreSnapshot` via projection so the synthetic's
              // primary is not rendered.
              ({
                providerId: coreSnapshot!.providerId,
                displayName: coreSnapshot!.displayName,
                primary: {
                  usedPercent: 0,
                  remainingPercent: 100,
                  kind: null,
                  windowMinutes: null,
                  resetsAt: null,
                  resetDescription: null,
                  isExhausted: false,
                  reservePercent: null,
                  reserveDescription: null,
                },
                primaryLabel: null,
                secondary: null,
                modelSpecific: null,
                tertiary: null,
                extraRateWindows: [],
                cost: coreSnapshot!.cost,
                planName: coreSnapshot!.planName,
                accountEmail: coreSnapshot!.accountEmail,
                sourceLabel: coreSnapshot!.sourceLabel,
                updatedAt: coreSnapshot!.updatedAt ?? new Date().toISOString(),
                error: coreSnapshot!.error,
                pace: coreSnapshot!.pace,
                accountOrganization: coreSnapshot!.accountOrganization,
                trayStatusLabel: coreSnapshot!.trayStatusLabel,
              } as unknown as ProviderUsageSnapshot)
            }
            coreSnapshot={coreSnapshot}
            chartLoader={chartLoader}
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

      {/* 3. Recent data — chart/local usage/output speed via enrichment. */}
      {detail && (
        <StatsSection
          providerId={detail.id}
          accountEmail={detail.email}
          speed={providerOutputSpeed}
          cost={detail.cost}
          localUsagePeriod={localUsagePeriod}
          chartLoader={chartLoader}
          coreSnapshot={coreSnapshot}
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
          capabilitiesState={authCapabilitiesState}
          credentialRevision={credentialRevision}
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

      {detail?.id === "claude" && <ClaudeCreds t={t} />}

      {detail && hasTokenAccounts && (
        <section className="provider-detail-section">
          <div className="provider-detail-section__header">
            <h4>{t("SectionSavedAccounts")}</h4>
          </div>
          <p className="provider-detail-helper">
            {/* TODO(lane-s-i18n): 账号卡与登录方式分开 */}
            工作号、个人号可以都存着。上面是进门方式，这里是进门之后用哪一条。
          </p>
          <TokenAccountsPanel
            key={`token-${detail.id}-${credentialRevision}`}
            providerId={detail.id}
            compact
          />
        </section>
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
  capabilitiesState,
  credentialRevision,
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
  capabilitiesState: "loading" | "ready" | "error";
  credentialRevision: number;
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
  const apiKeyNode = showApiKey ? (
    <ApiKeySection
      key={`api-${providerId}-${credentialRevision}`}
      providerId={providerId}
    />
  ) : null;

  // One method at a time. A provider is authenticated *one* way — cookies or a
  // browser sign-in or a CLI or an API key — so these are mutually exclusive
  // choices, and a segmented control is what a mutually exclusive choice looks
  // like. The zone used to render a lead card plus a collapsed "其他认证方式"
  // drawer, which implied a hierarchy that does not exist and hid a provider's
  // only real option behind a disclosure triangle.
  const userMethods = userFacingAuthMethods(methods, providerId);
  const userPrimary: UserAuthKind =
    primary === "bespoke" ? mapBespokeToUserKind(providerId) : primary;
  const userLabel: Record<UserAuthKind, string> = {
    // TODO(lane-s-i18n): CLI / OAuth 登录 / 会话 Cookie / API 密钥
    signIn: "CLI / OAuth 登录",
    cookie: "会话 Cookie",
    apiKey: "API 密钥",
  };

  const [chosen, setChosen] = useState<UserAuthKind>(userPrimary);
  useEffect(() => setChosen(userPrimary), [userPrimary, providerId]);
  const method = userMethods.includes(chosen) ? chosen : userPrimary;
  const mappedBespoke = isBespoke ? mapBespokeToUserKind(providerId) : null;

  return (
    <div className="provider-detail-auth-zone" data-auth-sources="true">
      <div className="provider-detail-section__header provider-detail-auth-zone__header">
        <h3 className="provider-detail-auth-zone__title">
          {t("ProviderAuthSourcesTitle")}
        </h3>
        {userMethods.length > 1 ? (
          <p className="provider-detail-auth-zone__helper">
            {t("ProviderAuthPickMethod")}
          </p>
        ) : null}
      </div>
      {capabilities === null ? (
        <ProviderStatusLine
          tone={capabilitiesState === "error" ? "error" : "loading"}
        >
          {capabilitiesState === "error"
            ? "暂时无法读取认证能力，未显示可能失效的登录入口。"
            : "正在读取真实认证能力…"}
        </ProviderStatusLine>
      ) : userMethods.length === 0 ? (
        <ProviderStatusLine tone="neutral">
          此服务商当前没有可用的认证方式或 API 密钥入口。
        </ProviderStatusLine>
      ) : null}
      {capabilities !== null && userMethods.length > 1 && (
        <SegmentedControl
          value={method}
          options={userMethods.map((kind) => ({
            value: kind,
            label: userLabel[kind],
          }))}
          onChange={(value) => setChosen(value as UserAuthKind)}
        />
      )}
      {capabilities !== null && userMethods.length > 0 && (
        <div className="provider-detail-auth-primary">
        {mappedBespoke === method
          ? bespokeNode
          : method === "cookie"
            ? cookieNode
            : method === "signIn"
              ? signInNode
              : apiKeyNode}
        </div>
      )}
      {capabilities !== null && userMethods.length > 0 && showCookieSource && method === "cookie" && (
        <details className="provider-detail-auth-advanced">
          <summary>{t("TabAdvanced")}</summary>
          <CookieSourceSection
            providerId={providerId}
            currentValue={cookieSource ?? null}
            options={cookieOptions}
            t={t}
            onChanged={onCookieSourceChanged}
          />
        </details>
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
  // Say *why*, not *where*. This used to print the failed URL and nothing
  // else, which was the least useful half of the error and — on Claude — put
  // the account's organization id on screen. The reason now survives from Rust
  // (see `describe_network_error`), and a timeout or a refused connection is
  // what actually tells the user whether to retry or to check the network.
  const network = message.match(/^network error:\s*(.+)$/is);
  if (network) {
    const detail = network[1].trim();
    if (/^timeout:/i.test(detail)) return t("ProviderIssueNetworkTimeout");
    if (/^connect:/i.test(detail)) return t("ProviderIssueNetworkConnectionFailed");
    // Anything else: name it as a request failure and append the cause chain
    // with the URL stripped, so the message stays diagnostic without carrying
    // an account identifier. The unedited original is still on the Copy button.
    const withoutUrl = detail.replace(/\s*\(https?:\/\/[^\s)]+\)/g, "").trim();
    return withoutUrl
      ? t("ProviderIssueNetworkRequestFailed") + "：" + withoutUrl
      : t("ProviderIssueNetworkRequestFailed");
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

  // One line, not a three-row table.
  //
  // The table restated the credential *kinds* the picker above already names,
  // and then printed the same backend string against each of them — the only
  // fact it carried, three times over. What is worth saying is what protects
  // the secrets and how to get rid of them; which kinds exist is the picker's
  // job.
  //
  // The backends are read from whichever slots are in use rather than assumed
  // to agree: they normally do, but a settings file carried across machines can
  // hold one slot encrypted by a backend the current machine cannot read, and
  // silently claiming otherwise would be worse than saying nothing.
  const backends = [status.apiKeys, status.manualCookies, status.tokenAccounts]
    .filter((value) => value.startsWith("protected:"))
    .map((value) => value.slice("protected:".length));
  const unique = [...new Set(backends)];

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
      <p className="provider-detail-credential-storage__summary">
        {unique.length > 0
          ? `${t("CredentialProtectedPrefix")} (${unique.join(", ")})`
          : t("CredentialStorageNothingStored")}
      </p>
    </section>
  );
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
