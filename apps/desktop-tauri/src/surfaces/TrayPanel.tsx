import { Fragment, useCallback, useEffect, useMemo, useRef, useState, type AnimationEvent, type MouseEvent as ReactMouseEvent } from "react";
import { listen } from "@tauri-apps/api/event";
import type {
  BootstrapState,
  MenuBarDisplayMode,
  ProviderUsageSnapshot,
} from "../types/bridge";
import type { ProviderSnapshot, UsageStoreKey } from "../core";
import {
  beginFlyoutGesture,
  beginTrayPanelResize,
  endFlyoutGesture,
} from "../lib/tauri";
import { useDispatchAction } from "../core/useCoreBridge";
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
  coreSnapshotToBridge,
  hydrateProviderSlots,
  orderedEnabledProviderSlots,
} from "../lib/trayProviders";
import {
  trayCoreStore,
  useTrayCoreRecords,
  useTrayCoreState,
  useTrayRefreshAllCommand,
} from "./tray/trayCoreStore";

const TRAY_INITIAL_REFRESH_DELAY_MS = 250;
const DENSE_OVERVIEW_THRESHOLD = 32;
/** Matches shell::flyout_window's retained-window lifecycle events. */
const TRAY_PANEL_REVEALED_EVENT = "tray-panel-revealed";
const TRAY_PANEL_CLOSING_EVENT = "tray-panel-closing";
const TRAY_PANEL_HIDDEN_EVENT = "tray-panel-hidden";
const TRAY_PANEL_FROST_EVENT = "tray-panel-frost";

/** The eight resize directions, matching shell::flyout_window::begin_resize. */
const RESIZE_DIRECTIONS = ["n", "s", "e", "w", "ne", "nw", "se", "sw"] as const;

/** Stroke-width / cap / join shared by every footer action glyph. */
const footerIconProps = {
  width: 12,
  height: 12,
  viewBox: "0 0 24 24",
  fill: "none",
  stroke: "currentColor",
  strokeWidth: 1.85,
  strokeLinecap: "round" as const,
  strokeLinejoin: "round" as const,
  "aria-hidden": true,
};
const RefreshIcon = () => (
  <svg {...footerIconProps}>
    <path d="M3 12a9 9 0 0 1 9-9 9.75 9.75 0 0 1 6.74 2.74L21 8" />
    <path d="M21 3v5h-5" />
    <path d="M21 12a9 9 0 0 1-9 9 9.75 9.75 0 0 1-6.74-2.74L3 16" />
    <path d="M3 21v-5h5" />
  </svg>
);
const GearIcon = () => (
  <svg {...footerIconProps}>
    <circle cx="12" cy="12" r="3" />
    <path d="M19.4 15a1.65 1.65 0 0 0 .33 1.82l.06.06a2 2 0 0 1 0 2.83 2 2 0 0 1-2.83 0l-.06-.06a1.65 1.65 0 0 0-1.82-.33 1.65 1.65 0 0 0-1 1.51V21a2 2 0 0 1-2 2 2 2 0 0 1-2-2v-.09A1.65 1.65 0 0 0 9 19.4a1.65 1.65 0 0 0-1.82.33l-.06.06a2 2 0 0 1-2.83 0 2 2 0 0 1 0-2.83l.06-.06a1.65 1.65 0 0 0 .33-1.82 1.65 1.65 0 0 0-1.51-1H3a2 2 0 1 1 2-2 2 2 0 0 1 2 2h.09A1.65 1.65 0 0 0 4.6 9a1.65 1.65 0 0 0-.33-1.82l-.06-.06a2 2 0 0 1 0-2.83 2 2 0 0 1 2.83 0l.06.06a1.65 1.65 0 0 0 1.82.33H9a1.65 1.65 0 0 0 1-1.51V3a2 2 0 0 1 2-2 2 2 0 0 1 2 2v.09a1.65 1.65 0 0 0 1 1.51 1.65 1.65 0 0 0 1.82-.33l.06.06a2 2 0 0 1 2.83 0 2 2 0 0 1 0 2.83l-.06.06a1.65 1.65 0 0 0-.33 1.82V9a1.65 1.65 0 0 0 1.51 1H21a2 2 0 0 1 2 2 2 2 0 0 1-2 2h-.09a1.65 1.65 0 0 0-1.51 1z" />
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
 *
 * Data comes from the unified core read model (trayCoreStore), never from
 * `useProviders`: provider snapshots are core records, refresh is a manual
 * coordinator command, and chart/speed slots render injected enrichment
 * results. The local timers below only drive UI ticking (reset countdowns);
 * panel data freshness belongs to the core coordinator.
 */
export default function TrayPanel({ state }: { state: BootstrapState }) {
  const { settings } = useSettings(state.settings);
  const { t } = useLocale();
  const trayState = useTrayCoreState();
  const records = useTrayCoreRecords();
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
    () =>
      records
        .filter(
          (record) =>
            record.snapshot != null &&
            shownProviderIds.includes(record.snapshot.providerId),
        )
        .map((record) => coreSnapshotToBridge(record.snapshot!)),
    [records, shownProviderIds],
  );
  const coreById = useMemo(() => {
    const map = new Map<string, ProviderSnapshot>();
    for (const record of records) {
      if (record.snapshot) map.set(record.snapshot.providerId, record.snapshot);
    }
    return map;
  }, [records]);
  const dispatchAction = useDispatchAction();
  const refreshReplica = useTrayRefreshAllCommand();
  const [refreshPending, setRefreshPending] = useState(false);
  const replicaRefreshing = records.some(
    (record) =>
      record.displayState === "refreshing" || record.displayState === "loading",
  );
  const isRefreshing = refreshPending || replicaRefreshing;
  const hasCachedData = records.some(
    (record) => record.snapshot != null || record.lastGood != null,
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

  /**
   * Listen for the backend's "select provider" intent (tray context menu,
   * proof harness, surface_action selectProvider). Each window only owns the
   * events it publishes, so the flyout reacts to the one it cares about.
   */
  useEffect(() => {
    let disposed = false;
    let unlisten: (() => void) | null = null;
    void listen<string | null>("flyout-select-provider", (event) => {
      if (disposed) return;
      setSelectedProviderId(event.payload || null);
    }).then((fn) => {
      unlisten = fn;
    });
    return () => {
      disposed = true;
      try { unlisten?.(); } catch { /* ignore */ }
    };
  }, []);
  const [gridExpanded, setGridExpanded] = useState(false);
  const [revealPhase, setRevealPhase] = useState<
    "parked" | "opening" | "closing"
  >("parked");
  /** True once the opening animation has played. The reveal keyframes use
   *  fill-mode `both`, which would otherwise hold a transform on the panel
   *  forever and keep the whole tree on a composited layer — the source of
   *  the blurry-while-scrolling text. --settled drops it (styles.css). */
  const [revealSettled, setRevealSettled] = useState(false);
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

  // Provider keys to refresh: real keys of enabled records first, then
  // placeholder keys for enabled providers the store has not seen yet.
  const refreshKeys = useMemo<UsageStoreKey[]>(() => {
    const keys: UsageStoreKey[] = [];
    const seen = new Set<string>();
    for (const record of records) {
      if (shownProviderIds.includes(record.key.providerId)) {
        keys.push(record.key);
        seen.add(record.key.providerId);
      }
    }
    for (const providerId of shownProviderIds) {
      if (!seen.has(providerId)) {
        keys.push({ providerId, accountKey: "default", sourceKey: "default" });
      }
    }
    return keys;
  }, [records, shownProviderIds]);

  // Initial stale-aware refresh (manual when the "refresh on open" setting is
  // on). Runs once per surface mount, delayed so the flyout can paint first.
  const initialRefreshRequested = useRef(false);
  useEffect(() => {
    if (initialRefreshRequested.current) return;
    if (refreshKeys.length === 0) return;
    initialRefreshRequested.current = true;
    const timer = window.setTimeout(() => {
      void refreshReplica(refreshKeys, {
        manual: settings.refreshAllProvidersOnMenuOpen,
      }).catch(() => {});
    }, TRAY_INITIAL_REFRESH_DELAY_MS);
    return () => window.clearTimeout(timer);
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [refreshReplica, refreshKeys, settings.refreshAllProvidersOnMenuOpen]);

  // Committed chart/local-usage enrichment per provider, read from the core
  // store so TrayCard receives injected results (never a direct tauri call).
  const chartByProviderId = useMemo(() => {
    const map = new Map<string, import("../types/bridge").ProviderChartData>();
    if (!trayCoreStore.hasRecords()) return map;
    for (const record of records) {
      const chart = trayCoreStore.getChartData(record.key);
      if (chart) map.set(record.key.providerId, chart);
    }
    return map;
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [records, trayState.version]);

  // Trigger chart enrichment for all chart/local-usage capable providers.
  // The runner caches results in memory and reads local logs from disk/cache.
  const chartCapableSignature = useMemo(
    () =>
      trayCoreStore
        .chartCapableKeys()
        .map((key) => `${key.providerId}:${key.accountKey}:${key.sourceKey}`)
        .join("|"),
    // eslint-disable-next-line react-hooks/exhaustive-deps
    [trayState.version],
  );
  useEffect(() => {
    const keys = trayCoreStore.chartCapableKeys();
    for (const key of keys) {
      void trayCoreStore
        .triggerEnrichment("chart", key, { manual: false })
        .catch(() => {});
    }
  }, [chartCapableSignature]);


  useEffect(() => {
    let disposed = false;
    const unlisteners: Array<() => void> = [];
    const replayReveal = () => {
      setRevealPhase("opening");
      const body = document.querySelector<HTMLElement>(
        ".tray-panel-reveal .flyout-body",
      );
      if (body) body.scrollTop = 0;
    };
    const replayClose = () => setRevealPhase("closing");
    const parkReveal = () => setRevealPhase("parked");

    void (async () => {
      unlisteners.push(await listen(TRAY_PANEL_REVEALED_EVENT, replayReveal));
      unlisteners.push(await listen(TRAY_PANEL_CLOSING_EVENT, replayClose));
      unlisteners.push(await listen(TRAY_PANEL_HIDDEN_EVENT, parkReveal));
      unlisteners.push(
        await listen<string>(TRAY_PANEL_FROST_EVENT, (event) => {
          if (typeof event.payload === "string" && event.payload.startsWith("url(")) {
            document.documentElement.style.setProperty("--tray-frost", event.payload);
          }
        }),
      );
      if (disposed) {
        for (const unlisten of unlisteners) unlisten();
      }
    })();

    return () => {
      disposed = true;
      for (const unlisten of unlisteners) unlisten();
    };
  }, []);

  useEffect(() => {
    if (revealPhase !== "opening") setRevealSettled(false);
  }, [revealPhase]);
  // Only the wrapper's own reveal animation counts; child SVGs animate too
  // and animationend bubbles.
  const handleRevealAnimationEnd = useCallback((event: AnimationEvent<HTMLDivElement>) => {
    if (event.target === event.currentTarget) setRevealSettled(true);
  }, []);

  const openSettings = useCallback(() => {
    void dispatchAction({
      type: "openSettings",
      target: { kind: "settings", tab: "general" },
    }).catch(() => {});
  }, [dispatchAction]);
  const requestQuit = useCallback(() => {
    void dispatchAction({ type: "quit", target: { kind: "app" } }).catch(() => {});
  }, [dispatchAction]);
  const handleRefresh = useCallback(() => {
    if (refreshPending) return;
    setRefreshPending(true);
    void refreshReplica(refreshKeys, { manual: true })
      .catch(() => {})
      .finally(() => setRefreshPending(false));
  }, [refreshReplica, refreshKeys, refreshPending]);

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
        void dispatchAction({
          type: "dismiss",
          target: { kind: "summary" },
        }).catch(() => {});
        return;
      }
      if (!e.ctrlKey || e.shiftKey || e.altKey || e.metaKey) return;
      switch (e.key.toLowerCase()) {
        case "r":
          e.preventDefault();
          void handleRefresh();
          break;
        case ",":
          e.preventDefault();
          openSettings();
          break;
        case "q":
          e.preventDefault();
          requestQuit();
          break;
      }
    };
    window.addEventListener("keydown", handler);
    return () => window.removeEventListener("keydown", handler);
  }, [handleRefresh, openSettings, requestQuit, dispatchAction]);

  const handleGridClick = useCallback((providerId: string | null) => {
    setSelectedProviderId(providerId);
  }, []);
  const handleReorder = useCallback((orderedIds: string[]) => {
    void dispatchAction({
      type: "reorderProviders",
      target: { kind: "summary" },
      providerIds: orderedIds,
    }).catch(() => {});
  }, [dispatchAction]);
  const handleGestureStart = useCallback(() => {
    void beginFlyoutGesture().catch(() => {});
  }, []);
  const handleGestureEnd = useCallback(() => {
    void endFlyoutGesture().catch(() => {});
  }, []);

  const revealClassName = `tray-panel-reveal tray-panel-reveal--native-size tray-panel-reveal--${revealPhase}${revealSettled && revealPhase === "opening" ? " tray-panel-reveal--settled" : ""}${expectsDenseOverview ? " tray-panel-reveal--dense" : ""}${selectedProviderId !== null ? " tray-panel-reveal--detail" : ""}`;
  const isDetailView = selectedProviderId !== null;
  const revealRef = useRef<HTMLDivElement>(null);

  // HTML's 6px flyout-chrome sits on the page, not inside the 328px card.
  // A leftover gutter (or :root app-bg) in this HWND reads as extra inset
  // from the window edge to the icons. Pin padding/background on the
  // elements styles.css cannot always beat in a retained WebView.
  useEffect(() => {
    const html = document.documentElement;
    const body = document.body;
    const root = document.getElementById("root");
    // The reveal element itself keeps its 6 DIP padding: that gutter is the
    // popup_chrome shadow budget (blur 3 + offset-y 3) and must survive, or
    // the shared down-only shadow falls outside the HWND and is clipped.
    const remembered: Array<[HTMLElement, string]> = [
      [html, html.style.cssText],
      [body, body.style.cssText],
    ];
    if (root) remembered.push([root, root.style.cssText]);
    const zeroBox = (el: HTMLElement | null) => {
      if (!el) return;
      el.style.setProperty("padding", "0px", "important");
      el.style.setProperty("margin", "0px", "important");
    };
    zeroBox(html);
    zeroBox(body);
    zeroBox(root);
    html.style.setProperty("background", "transparent", "important");
    body.style.setProperty("background", "transparent", "important");
    root?.style.setProperty("background", "transparent", "important");
    return () => {
      for (const [el, css] of remembered) {
        el.style.cssText = css;
      }
    };
  }, []);

  const renderProviderCard = (p: ProviderUsageSnapshot) => {
    const speedId = outputSpeedProviderId(p.providerId);
    const coreSnapshot = coreById.get(p.providerId) ?? null;
    const providerForCard: ProviderSnapshot | ProviderUsageSnapshot =
      coreSnapshot ?? p;
    return (
      <TrayCard
        key={p.providerId}
        provider={providerForCard}
        densityMode={densityMode}
        display={display}
        outputSpeed={speedId ? outputSpeed?.[speedId] : null}
        outputSpeedEnabled={settings.outputSpeedEnabled !== false}
        localUsagePeriod={settings.localUsagePeriod}
        showProviderIcon={settings.switcherShowsIcons}
        chartData={chartByProviderId.get(p.providerId) ?? null}
        detail={isDetailView}
        onOpenExternalUsage={(providerId) => {
          void dispatchAction({
            type: "openExternalUsage",
            target: { kind: "provider", providerId },
          }).catch(() => {});
        }}
        onOpenExternalStatus={(providerId) => {
          void dispatchAction({
            type: "openExternalStatus",
            target: { kind: "provider", providerId },
          }).catch(() => {});
        }}
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
      <button type="button" className="footer-row" onClick={() => void handleRefresh()}>
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
      <button type="button" className="footer-row" onClick={requestQuit}>
        <span className="footer-row__left">
          <span className="footer-row__icon"><PowerIcon /></span>
          <span className="footer-row__label">{t("MenuQuit")}</span>
        </span>
        <span className="footer-row__shortcut">Ctrl+Q</span>
      </button>
    </footer>
  );

  // Native resize affordance. WebView2 covers the whole client HWND, so the
  // card's visible edges never reach WM_NCHITTEST; these strips sit on the
  // card stroke and call startResizeDragging (ReleaseCapture + HT*). Keep
  // the blur guard armed until pointerup — PostMessage returns immediately.
  const handleResizeDown = useCallback(
    (dir: string) => (event: ReactMouseEvent<HTMLDivElement>) => {
      event.preventDefault();
      event.stopPropagation();
      void Promise.resolve(beginFlyoutGesture()).catch(() => {});
      const end = () => {
        window.removeEventListener("pointerup", end, true);
        void Promise.resolve(endFlyoutGesture()).catch(() => {});
      };
      window.addEventListener("pointerup", end, true);
      void Promise.resolve(beginTrayPanelResize(dir)).catch(() => {
        end();
      });
    },
    [],
  );
  const resizeHandles = (
    <div className="tray-resize-hits" aria-hidden="true">
      {RESIZE_DIRECTIONS.map((dir) => (
        <div
          key={dir}
          data-resize-dir={dir}
          className={`tray-resize-hit tray-resize-hit--${dir}`}
          onMouseDown={handleResizeDown(dir)}
        />
      ))}
    </div>
  );

  if (sorted.length === 0) {
    return (
      <div
        ref={revealRef}
        className={revealClassName}
        style={{ margin: 0, boxSizing: "border-box" }}
        onAnimationEnd={handleRevealAnimationEnd}
      >
        <div className="tray-panel">
          {grid}
          <div className="flyout-body">
            <MenuEmpty
              isLoading={isRefreshing && !hasCachedData}
              onSettings={openSettings}
            />
          </div>
          {footer}
          {resizeHandles}
        </div>
      </div>
    );
  }

  return (
    <div
      ref={revealRef}
      className={revealClassName}
      style={{ margin: 0, boxSizing: "border-box" }}
      onAnimationEnd={handleRevealAnimationEnd}
    >
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
        {resizeHandles}
      </div>
    </div>
  );
}
