import { Fragment, useCallback, useEffect, useMemo, useState } from "react";
import type {
  BootstrapState,
  MenuBarDisplayMode,
  ProviderUsageSnapshot,
} from "../types/bridge";
import {
  beginFlyoutGesture,
  dismissTrayPanel,
  endFlyoutGesture,
  openSettingsWindow,
  quitApp as quitApplication,
  reorderProviders,
  setSurfaceMode,
} from "../lib/tauri";
import { useProviders } from "../hooks/useProviders";
import { useSettings } from "../hooks/useSettings";
import { useLocale } from "../hooks/useLocale";
import { useSurfaceTarget } from "../hooks/useSurfaceMode";
import { useTrayPanelLayout } from "../hooks/useTrayPanelLayout";
import { useOutputSpeedSnapshot } from "../hooks/useOutputSpeedSnapshot";
import MenuCard from "../components/MenuCard";
import MenuSurface, {
  MenuEmpty,
  type MenuFooterRow,
} from "../components/MenuSurface";
import ProviderGrid, { prioritizeProviders } from "../components/ProviderGrid";
import { openProviderDashboard, openProviderStatusPage } from "../lib/tauri";
import { orderProviderSnapshots } from "../lib/providerOrder";
import { resolveDashboardProviderIds } from "../lib/dashboardProviders";
import { quotaDisplayContext } from "../lib/quotaDisplay";
import { outputSpeedProviderId } from "../lib/outputSpeed";
import {
  hydrateProviderSlots,
  orderedEnabledProviderSlots,
} from "../lib/trayProviders";

/** Provider IDs that have a dashboard URL in the backend */
const HAS_DASHBOARD = new Set([
  "abacus", "alibaba", "alibabatokenplan", "amp", "augment",
  "azureopenai", "bedrock", "claude", "codex", "codebuff",
  "commandcode", "copilot", "crof", "crossmodel", "cursor", "deepgram", "deepseek",
  "doubao", "arkcodingplan", "arkagentplan", "elevenlabs", "factory", "gemini", "grok", "groq",
  "infini", "jetbrains", "kilo", "kimi", "kimik2", "kiro", "manus",
  "mimo", "mimoapi", "minimax", "mistral", "nanogpt", "ollama", "openaiapi",
  "opencode", "opencodego", "openrouter", "perplexity", "qoder", "sakana", "stepfun",
  "t3chat", "venice", "vertexai", "warp", "windsurf",
  "zai",
]);
/** Provider IDs that have a status page URL in the backend */
const HAS_STATUS_PAGE = new Set([
  "alibabatokenplan", "amp", "augment", "azureopenai", "bedrock",
  "claude", "codex", "copilot", "deepgram", "deepseek", "elevenlabs",
  "gemini", "grok", "groq", "kiro", "mistral", "openaiapi",
  "openrouter", "vertexai", "windsurf",
]);

const TRAY_INITIAL_REFRESH_DELAY_MS = 250;
const DENSE_OVERVIEW_THRESHOLD = 32;

/**
 * Tray popover surface — two modes like macOS CodexBar:
 * 1. Overview (default): provider grid + all cards stacked
 * 2. Detail: click a provider in grid → show only that provider's card
 */
export default function TrayPanel({ state }: { state: BootstrapState }) {
  const { settings } = useSettings(state.settings);
  const {
    providers,
    isRefreshing,
    refresh,
    hasCachedData,
  } = useProviders({
    initialRefreshDelayMs: TRAY_INITIAL_REFRESH_DELAY_MS,
    forceRefreshOnMount: settings.refreshAllProvidersOnMenuOpen,
  });

  const { t } = useLocale();
  const outputSpeed = useOutputSpeedSnapshot(
    settings.outputSpeedEnabled !== false,
  );
  const surfaceTarget = useSurfaceTarget("trayPanel");
  // The tray flyout and the PopOut dashboard share the "dashboard" component's
  // settings — they render the same cards from the same snapshot. Neither reads
  // the floating bar's or the taskbar strip's preference.
  const display = useMemo(
    () => quotaDisplayContext(settings, "dashboard"),
    [settings],
  );
  // The dashboard's own provider filter (item H). Everything below reads this
  // rather than `settings.enabledProviders`, so the dense-overview threshold
  // and the slot list count the providers actually on screen.
  const shownProviderIds = useMemo(
    () =>
      resolveDashboardProviderIds(
        settings.enabledProviders,
        settings.dashboardProviderIds,
      ),
    [settings.dashboardProviderIds, settings.enabledProviders],
  );
  // The cache is deliberately retained when a provider is disabled so the
  // Settings page can still describe its last result.  A tray flyout is a
  // live view, though: it must only render currently enabled providers.
  const enabledSnapshots = useMemo(
    () => providers.filter((provider) => shownProviderIds.includes(provider.providerId)),
    [providers, shownProviderIds],
  );

  const sorted = useMemo(
    () => {
      const ordered = orderProviderSnapshots(
        enabledSnapshots,
        state.providers,
        shownProviderIds,
        settings.providerOrder,
      );
      if (!settings.menuBarShowsHighestUsage) return ordered;
      return [...ordered].sort(
        (left, right) => right.primary.usedPercent - left.primary.usedPercent,
      );
    },
    [
      enabledSnapshots,
      shownProviderIds,
      settings.menuBarShowsHighestUsage,
      settings.providerOrder,
      state.providers,
    ],
  );
  const denseProviderSlots = useMemo(
    () =>
      orderedEnabledProviderSlots(
        state.providers,
        shownProviderIds,
        sorted,
        settings.providerOrder,
      ),
    [shownProviderIds, settings.providerOrder, sorted, state.providers],
  );
  const providersById = useMemo(
    () => new Map(sorted.map((provider) => [provider.providerId, provider])),
    [sorted],
  );
  const initialProviderId =
    surfaceTarget?.kind === "provider" ? surfaceTarget.providerId : null;

  // null = overview (all providers), string = single provider detail
  const [selectedProviderId, setSelectedProviderId] = useState<string | null>(
    initialProviderId,
  );
  const [gridExpanded, setGridExpanded] = useState(false);
  const expectsDenseOverview =
    selectedProviderId === null &&
    !gridExpanded &&
    shownProviderIds.length + 1 > DENSE_OVERVIEW_THRESHOLD;
  const denseTrayProviders = useMemo(() => {
    if (!expectsDenseOverview) return sorted;
    return hydrateProviderSlots(denseProviderSlots, providersById);
  }, [denseProviderSlots, expectsDenseOverview, providersById, sorted]);

  useEffect(() => {
    setSelectedProviderId(initialProviderId);
  }, [initialProviderId]);

  // Cards to display based on mode
  // Overview: all providers in the grid — non-error first, then errors
  // Detail: only the selected provider's card (macOS shows single provider)
  const visibleProviders = useMemo(() => {
    if (selectedProviderId === null) {
      // Overview: show providers in the same Settings/catalog order as the grid.
      if (sorted.length + 1 > DENSE_OVERVIEW_THRESHOLD && !gridExpanded) {
        return prioritizeProviders(denseTrayProviders, null).slice(0, 4);
      }
      return sorted;
    }
    // Detail: show ONLY the selected provider (macOS behavior — no appended errors)
    const match = sorted.find((p) => p.providerId === selectedProviderId);
    if (!match) {
      return sorted;
    }
    return [match];
  }, [denseTrayProviders, sorted, selectedProviderId, gridExpanded]);

  // The tray panel is hosted by the dedicated `flyout` window. It opens at the
  // 328×776 logical reference size, then lets the native window own edge
  // resizing and remembered dimensions; provider data must never write size.
  // The display-mode setting only governs the "all providers" overview list.
  // Opening a single provider is an explicit "show me everything" action, so
  // its detail card always renders full content regardless of the mode —
  // hardcoded "detailed" here rather than reading `menuBarDisplayMode`.
  const densityMode: MenuBarDisplayMode =
    selectedProviderId !== null ? "detailed" : settings.menuBarDisplayMode;
  const { layoutReady } = useTrayPanelLayout({
    // The native flyout starts hidden and must be revealed as soon as the
    // React shell mounts. Provider/cache data is allowed to arrive later;
    // using it as the reveal gate makes proof-mode automation observe
    // IsWindowVisible=false during a slow first refresh and misclassify that
    // as blur-dismiss.
    canMeasure: true,
  });

  const openSettings = useCallback(() => {
    // Keep the flyout visible as a live preview while Settings is open.
    // The native blur handler recognizes the Settings window as an allowed
    // companion surface, so this does not turn ordinary click-outside into a
    // permanently pinned flyout.
    void openSettingsWindow("general").catch(() => {});
  }, []);
  const openDashboard = useCallback(() => {
    void (async () => {
      try {
        await setSurfaceMode("popOut", { kind: "dashboard" });
      } finally {
        void dismissTrayPanel().catch(() => {});
      }
    })();
  }, []);
  const quitApp = useCallback(() => {
    void quitApplication();
  }, []);

  const footerRows: MenuFooterRow[] = [
    // Glyphs chosen for what the row *does*. `⧉` (overlapping squares — the
    // duplicate/clone mark) said nothing about a dashboard, and `⌧` is a
    // cancel box, not quitting; `⊞` reads as a panel of tiles and `⏻` is the
    // standard power mark.
    { icon: "⊞", label: t("TrayOpenDashboard"), onClick: openDashboard },
    {
      icon: "↻",
      label: t("ActionRefresh"),
      shortcut: "Ctrl+R",
      onClick: refresh,
      // Clicking refresh used to give no sign it had registered; the numbers
      // simply changed a second or two later, or did not.
      spinning: isRefreshing,
    },
    { icon: "⚙", label: t("MenuSettings"), shortcut: "Ctrl+,", onClick: openSettings },
    { icon: "⏻", label: t("MenuQuit"), shortcut: "Ctrl+Q", onClick: quitApp },
  ];

  // Keyboard shortcuts
  useEffect(() => {
    const handler = (e: KeyboardEvent) => {
      if (
        e.key === "Escape" &&
        !e.ctrlKey &&
        !e.shiftKey &&
        !e.altKey &&
        !e.metaKey
      ) {
        e.preventDefault();
        void dismissTrayPanel().catch(() => {});
        return;
      }
      if (!e.ctrlKey || e.shiftKey || e.altKey || e.metaKey) return;
      switch (e.key.toLowerCase()) {
        case "r":
          e.preventDefault();
          refresh();
          break;
        case ",":
          e.preventDefault();
          openSettings();
          break;
        case "q":
          e.preventDefault();
          quitApp();
          break;
      }
    };
    window.addEventListener("keydown", handler);
    return () => window.removeEventListener("keydown", handler);
  }, [refresh, openSettings, quitApp]);

  const handleGridClick = useCallback(
    (providerId: string | null) => {
      setSelectedProviderId(providerId);
    },
    [],
  );
  const handleReorder = useCallback((orderedIds: string[]) => {
    void reorderProviders(orderedIds).catch(() => {});
  }, []);
  const handleGestureStart = useCallback(() => {
    void beginFlyoutGesture().catch(() => {});
  }, []);
  const handleGestureEnd = useCallback(() => {
    void endFlyoutGesture().catch(() => {});
  }, []);
  const revealClassName = `tray-panel-reveal tray-panel-reveal--native-size${layoutReady ? " tray-panel-reveal--ready" : ""}${expectsDenseOverview ? " tray-panel-reveal--dense" : ""}${selectedProviderId !== null ? " tray-panel-reveal--detail" : ""}`;
  const isDetailView = selectedProviderId !== null;
  const renderProviderCard = (p: ProviderUsageSnapshot) => {
    const isSelected =
      selectedProviderId !== null && p.providerId === selectedProviderId;
    const speedProviderId = outputSpeedProviderId(p.providerId);
    return (
      <div
        className={`menu-stack__item${isSelected ? " menu-stack__item--selected" : ""}`}
        id={`card-${p.providerId}`}
        key={p.providerId}
      >
        <MenuCard
          provider={p}
          display={display}
          outputSpeed={speedProviderId ? outputSpeed?.[speedProviderId] : null}
          localUsagePeriod={settings.localUsagePeriod}
          showProviderIcon={settings.switcherShowsIcons}
          densityMode={densityMode}
          quotaWindows={settings.dashboardQuotaWindows}
        />
      </div>
    );
  };

  useEffect(() => {
    // A density-mode switch (in the overview) must always reveal the top of
    // the card stack, rather than preserving a scroll offset that belonged
    // to the previous tier's (taller or shorter) content.
    if (isDetailView) return;
    const body = document.querySelector<HTMLElement>(
      ".menu-surface--tray .menu-surface__body",
    );
    if (body) body.scrollTop = 0;
  }, [densityMode, isDetailView]);

  if (sorted.length === 0) {
    return (
      <div className={revealClassName}>
        <MenuSurface
          variant="tray"
          onRefresh={refresh}
          isRefreshing={isRefreshing}
          actions={[]}
          footerRows={footerRows}
        >
          <MenuEmpty
            isLoading={isRefreshing && !hasCachedData}
            onSettings={openSettings}
          />
        </MenuSurface>
      </div>
    );
  }

  return (
    <div className={revealClassName}>
      <MenuSurface
        variant="tray"
        onRefresh={refresh}
        isRefreshing={isRefreshing}
        actions={[]}
        footerRows={footerRows}
        fixedHeader={
          <ProviderGrid
            providers={expectsDenseOverview ? denseTrayProviders : sorted}
            selectedProviderId={selectedProviderId}
            display={display}
            showProviderIcons={settings.switcherShowsIcons}
            expanded={gridExpanded}
            onExpandedChange={setGridExpanded}
            onSelect={handleGridClick}
            onReorder={handleReorder}
            onGestureStart={handleGestureStart}
            onGestureEnd={handleGestureEnd}
          />
        }
      >
        <div className="menu-stack">
          {visibleProviders.map((p, idx) => (
            <Fragment key={p.providerId}>
              {idx > 0 && <div className="menu-stack__sep" />}
              {renderProviderCard(p)}
            </Fragment>
          ))}
        </div>
        {/* Context actions — detail mode only, matches macOS actionsSection */}
        {selectedProviderId && (HAS_DASHBOARD.has(selectedProviderId) || HAS_STATUS_PAGE.has(selectedProviderId)) && (
          <div className="context-actions">
            {HAS_DASHBOARD.has(selectedProviderId) && (
              <button
                type="button"
                className="context-actions__btn"
                onClick={() => void openProviderDashboard(selectedProviderId)}
              >
                <span className="context-actions__icon" aria-hidden>
                  <svg width="13" height="13" viewBox="0 0 16 16" fill="none" xmlns="http://www.w3.org/2000/svg">
                    <rect x="2" y="9" width="2.5" height="5" rx="0.6" fill="currentColor" />
                    <rect x="6.75" y="6" width="2.5" height="8" rx="0.6" fill="currentColor" />
                    <rect x="11.5" y="3" width="2.5" height="11" rx="0.6" fill="currentColor" />
                  </svg>
                </span>
                {t("ActionUsageDashboard")}
              </button>
            )}
            {HAS_STATUS_PAGE.has(selectedProviderId) && (
              <button
                type="button"
                className="context-actions__btn"
                onClick={() => void openProviderStatusPage(selectedProviderId)}
              >
                <span className="context-actions__icon" aria-hidden>
                  <svg width="14" height="13" viewBox="0 0 18 14" fill="none" xmlns="http://www.w3.org/2000/svg">
                    <path d="M1 7H4L5.5 3L8 11L10.5 5L12 7H17" stroke="currentColor" strokeWidth="1.4" strokeLinecap="round" strokeLinejoin="round" fill="none" />
                  </svg>
                </span>
                {t("ActionStatusPage")}
              </button>
            )}
          </div>
        )}
      </MenuSurface>
    </div>
  );
}
