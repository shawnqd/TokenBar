import { Fragment, useCallback, useEffect, useMemo, useState } from "react";
import { listen } from "@tauri-apps/api/event";
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
import { useTrayPanelLayout } from "../hooks/useTrayPanelLayout";
import { useOutputSpeedSnapshot } from "../hooks/useOutputSpeedSnapshot";
import TrayCard from "./tray/TrayCard";
import { MenuEmpty } from "../components/MenuSurface";
import ProviderGrid, { prioritizeProviders } from "../components/ProviderGrid";
import { orderProviderSnapshots } from "../lib/providerOrder";
import { quotaDisplayContext } from "../lib/quotaDisplay";
import { outputSpeedProviderId } from "../lib/outputSpeed";
import {
  hydrateProviderSlots,
  orderedEnabledProviderSlots,
} from "../lib/trayProviders";

const TRAY_INITIAL_REFRESH_DELAY_MS = 250;
const DENSE_OVERVIEW_THRESHOLD = 32;
/** Matches shell::flyout_window's retained-window lifecycle events. */
const TRAY_PANEL_REVEALED_EVENT = "tray-panel-revealed";
const TRAY_PANEL_CLOSING_EVENT = "tray-panel-closing";
const TRAY_PANEL_HIDDEN_EVENT = "tray-panel-hidden";

/** Stroke-width / cap / join shared by every footer action glyph. */
const footerIconProps = {
  width: 14,
  height: 14,
  viewBox: "0 0 24 24",
  fill: "none",
  stroke: "currentColor",
  strokeWidth: 1.85,
  strokeLinecap: "round" as const,
  strokeLinejoin: "round" as const,
  "aria-hidden": true,
};
const DashboardIcon = () => (
  <svg {...footerIconProps}>
    <rect x="3" y="3" width="7" height="7" rx="1.5" />
    <rect x="14" y="3" width="7" height="7" rx="1.5" />
    <rect x="14" y="14" width="7" height="7" rx="1.5" />
    <rect x="3" y="14" width="7" height="7" rx="1.5" />
  </svg>
);
const RefreshIcon = () => (
  <svg {...footerIconProps}>
    <path d="M21 12a9 9 0 0 0-9-9 9.75 9.75 0 0 0-6.74 2.74L3 8" />
    <path d="M3 3v5h5" />
    <path d="M3 12a9 9 0 0 0 9 9 9.75 9.75 0 0 0 6.74-2.74L21 16" />
    <path d="M21 21v-5h-5" />
  </svg>
);
const GearIcon = () => (
  <svg {...footerIconProps}>
    <circle cx="12" cy="12" r="3" />
    <path d="M19.4 15a1.65 1.65 0 0 0 .33 1.82l.06.06a2 2 0 0 1 0 2.83 2 2 0 0 1-2.83 0l-.06-.06a1.65 1.65 0 0 0-1.82-.33 1.65 1.65 0 0 0-1 1.51V21a2 2 0 0 1-2 2 2 2 0 0 1-2-2v-.09A1.65 1.65 0 0 0 9 19.4a1.65 1.65 0 0 0-1.82.33l-.06.06a2 2 0 0 1-2.83 0 2 2 0 0 1 0-2.83l.06-.06a1.65 1.65 0 0 0 .33-1.82 1.65 1.65 0 0 0-1.51-1H3a2 2 0 0 1-2-2 2 2 0 0 1 2-2h.09A1.65 1.65 0 0 0 4.6 9a1.65 1.65 0 0 0-.33-1.82l-.06-.06a2 2 0 0 1 0-2.83 2 2 0 0 1 2.83 0l.06.06a1.65 1.65 0 0 0 1.82.33H9a1.65 1.65 0 0 0 1-1.51V3a2 2 0 0 1 2-2 2 2 0 0 1 2 2v.09a1.65 1.65 0 0 0 1 1.51 1.65 1.65 0 0 0 1.82-.33l.06-.06a2 2 0 0 1 2.83 0 2 2 0 0 1 0 2.83l-.06.06a1.65 1.65 0 0 0-.33 1.82V9a1.65 1.65 0 0 0 1.51 1H21a2 2 0 0 1 2 2 2 2 0 0 1-2 2h-.09a1.65 1.65 0 0 0-1.51 1z" />
  </svg>
);
const PowerIcon = () => (
  <svg {...footerIconProps}>
    <path d="M18.36 6.64a9 9 0 1 1-12.73 0" />
    <line x1="12" y1="2" x2="12" y2="12" />
  </svg>
);

/**
 * Tray popover surface — native 328 DIP flyout with three density tiers
 * (overview obeys the setting; a single provider's detail view is always
 * detailed). The card stack is driven by TrayCard (the unified modular port
 * of design/density-preview.html); the footer holds the four actions as
 * stroke-icon rows.
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
  // The tray flyout and the PopOut dashboard share the "dashboard" component's
  // settings — they render the same cards from the same snapshot.
  const display = useMemo(
    () => quotaDisplayContext(settings, "dashboard"),
    [settings],
  );
  const shownProviderIds = settings.enabledProviders;
  // A tray flyout is a live view: render only currently enabled providers.
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
  // Selection is local to the retained flyout WebView.
  const [selectedProviderId, setSelectedProviderId] = useState<string | null>(null);
  const [gridExpanded, setGridExpanded] = useState(false);
  const [revealPhase, setRevealPhase] = useState<
    "parked" | "opening" | "closing"
  >("parked");
  const expectsDenseOverview =
    selectedProviderId === null &&
    !gridExpanded &&
    shownProviderIds.length + 1 > DENSE_OVERVIEW_THRESHOLD;
  const denseTrayProviders = useMemo(() => {
    if (!expectsDenseOverview) return sorted;
    return hydrateProviderSlots(denseProviderSlots, providersById);
  }, [denseProviderSlots, expectsDenseOverview, providersById, sorted]);

  const visibleProviders = useMemo(() => {
    if (selectedProviderId === null) {
      if (sorted.length + 1 > DENSE_OVERVIEW_THRESHOLD && !gridExpanded) {
        return prioritizeProviders(denseTrayProviders, null).slice(0, 4);
      }
      return sorted;
    }
    const match = sorted.find((p) => p.providerId === selectedProviderId);
    if (!match) return sorted;
    return [match];
  }, [denseTrayProviders, sorted, selectedProviderId, gridExpanded]);

  // Detail is an explicit "show me everything" action → always detailed.
  const densityMode: MenuBarDisplayMode =
    selectedProviderId !== null ? "detailed" : settings.menuBarDisplayMode;
  useTrayPanelLayout({ canMeasure: true });

  useEffect(() => {
    let disposed = false;
    const unlisteners: Array<() => void> = [];
    const replayReveal = () => setRevealPhase("opening");
    const replayClose = () => setRevealPhase("closing");
    const parkReveal = () => setRevealPhase("parked");

    void (async () => {
      unlisteners.push(await listen(TRAY_PANEL_REVEALED_EVENT, replayReveal));
      unlisteners.push(await listen(TRAY_PANEL_CLOSING_EVENT, replayClose));
      unlisteners.push(await listen(TRAY_PANEL_HIDDEN_EVENT, parkReveal));
      if (disposed) {
        for (const unlisten of unlisteners) unlisten();
      }
    })();

    return () => {
      disposed = true;
      for (const unlisten of unlisteners) unlisten();
    };
  }, []);

  const openSettings = useCallback(() => {
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

  // Keyboard shortcuts — Esc dismiss, Ctrl+R refresh, Ctrl+, settings, Ctrl+Q quit.
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

  const handleGridClick = useCallback((providerId: string | null) => {
    setSelectedProviderId(providerId);
  }, []);
  const handleReorder = useCallback((orderedIds: string[]) => {
    void reorderProviders(orderedIds).catch(() => {});
  }, []);
  const handleGestureStart = useCallback(() => {
    void beginFlyoutGesture().catch(() => {});
  }, []);
  const handleGestureEnd = useCallback(() => {
    void endFlyoutGesture().catch(() => {});
  }, []);

  const revealClassName = `tray-panel-reveal tray-panel-reveal--native-size tray-panel-reveal--${revealPhase}${expectsDenseOverview ? " tray-panel-reveal--dense" : ""}${selectedProviderId !== null ? " tray-panel-reveal--detail" : ""}`;
  const isDetailView = selectedProviderId !== null;

  const renderProviderCard = (p: ProviderUsageSnapshot) => {
    const speedId = outputSpeedProviderId(p.providerId);
    return (
      <TrayCard
        key={p.providerId}
        provider={p}
        densityMode={densityMode}
        display={display}
        outputSpeed={speedId ? outputSpeed?.[speedId] : null}
        localUsagePeriod={settings.localUsagePeriod}
        showProviderIcon={settings.switcherShowsIcons}
        detail={isDetailView}
      />
    );
  };

  useEffect(() => {
    // A density-mode switch (in the overview) must always reveal the top of
    // the card stack, rather than preserving a scroll offset that belonged to
    // the previous tier's content.
    if (isDetailView) return;
    const body = document.querySelector<HTMLElement>(
      ".tray-panel-reveal .flyout-body",
    );
    if (body) body.scrollTop = 0;
  }, [densityMode, isDetailView]);

  const grid = (
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
  );

  const footer = (
    <footer className="flyout-footer" aria-label={t("PanelMenu")}>
      <button type="button" className="footer-row" onClick={openDashboard}>
        <span className="footer-row__left">
          <span className="footer-row__icon"><DashboardIcon /></span>
          <span className="footer-row__label">{t("TrayOpenDashboard")}</span>
        </span>
      </button>
      <button type="button" className="footer-row" onClick={() => refresh()}>
        <span className="footer-row__left">
          <span className={`footer-row__icon${isRefreshing ? " is-spinning" : ""}`}><RefreshIcon /></span>
          <span className="footer-row__label">{t("ActionRefresh")}</span>
        </span>
        <span className="footer-row__shortcut">Ctrl+R</span>
      </button>
      <button type="button" className="footer-row" onClick={openSettings}>
        <span className="footer-row__left">
          <span className="footer-row__icon"><GearIcon /></span>
          <span className="footer-row__label">{t("MenuSettings")}</span>
        </span>
        <span className="footer-row__shortcut">Ctrl+,</span>
      </button>
      <button type="button" className="footer-row" onClick={quitApp}>
        <span className="footer-row__left">
          <span className="footer-row__icon"><PowerIcon /></span>
          <span className="footer-row__label">{t("MenuQuit")}</span>
        </span>
        <span className="footer-row__shortcut">Ctrl+Q</span>
      </button>
    </footer>
  );

  if (sorted.length === 0) {
    return (
      <div className={revealClassName}>
        <div className="tray-panel">
          {grid}
          <div className="flyout-body">
            <MenuEmpty
              isLoading={isRefreshing && !hasCachedData}
              onSettings={openSettings}
            />
          </div>
          {footer}
        </div>
      </div>
    );
  }

  return (
    <div className={revealClassName}>
      <div className="tray-panel">
        {grid}
        <div className="flyout-body">
          {visibleProviders.map((p, idx) => (
            <Fragment key={p.providerId}>
              {idx > 0 && <div className="provider-stack-divider" />}
              {renderProviderCard(p)}
            </Fragment>
          ))}
        </div>
        {footer}
      </div>
    </div>
  );
}
