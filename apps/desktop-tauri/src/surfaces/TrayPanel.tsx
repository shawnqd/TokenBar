import { Fragment, useCallback, useEffect, useMemo, useRef, useState } from "react";
import { getCurrentWindow } from "@tauri-apps/api/window";
import type { BootstrapState, ProviderUsageSnapshot } from "../types/bridge";
import {
  beginFlyoutGesture,
  dismissTrayPanel,
  endFlyoutGesture,
  flyoutStoredSize,
  openSettingsWindow,
  quitApp as quitApplication,
  reorderProviders,
  setFlyoutSize,
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
import { getProviderBalance } from "../lib/providerBalance";
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

function getProviderStatus(
  p: ProviderUsageSnapshot,
): "ok" | "warning" | "exhausted" | "error" {
  if (p.error) return "error";
  if (p.primary.isExhausted) return "exhausted";
  if (p.primary.usedPercent > 80) return "warning";
  return "ok";
}

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
    hasLoadedCache,
  } = useProviders({
    initialRefreshDelayMs: TRAY_INITIAL_REFRESH_DELAY_MS,
    forceRefreshOnMount: settings.refreshAllProvidersOnMenuOpen,
  });

  const { t } = useLocale();
  const outputSpeed = useOutputSpeedSnapshot(
    settings.outputSpeedEnabled !== false,
  );
  const surfaceTarget = useSurfaceTarget("trayPanel");
  // The cache is deliberately retained when a provider is disabled so the
  // Settings page can still describe its last result.  A tray flyout is a
  // live view, though: it must only render currently enabled providers.
  const enabledSnapshots = useMemo(
    () => providers.filter((provider) => settings.enabledProviders.includes(provider.providerId)),
    [providers, settings.enabledProviders],
  );

  const sorted = useMemo(
    () => {
      const ordered = orderProviderSnapshots(
        enabledSnapshots,
        state.providers,
        settings.enabledProviders,
        settings.providerOrder,
      );
      if (!settings.menuBarShowsHighestUsage) return ordered;
      return [...ordered].sort(
        (left, right) => right.primary.usedPercent - left.primary.usedPercent,
      );
    },
    [
      enabledSnapshots,
      settings.enabledProviders,
      settings.menuBarShowsHighestUsage,
      settings.providerOrder,
      state.providers,
    ],
  );
  const denseProviderSlots = useMemo(
    () =>
      orderedEnabledProviderSlots(
        state.providers,
        settings.enabledProviders,
        sorted,
        settings.providerOrder,
      ),
    [settings.enabledProviders, settings.providerOrder, sorted, state.providers],
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
    settings.enabledProviders.length + 1 > DENSE_OVERVIEW_THRESHOLD;
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

  const layoutKey = useMemo(
    () =>
      [
        selectedProviderId ?? "overview",
        gridExpanded ? "expanded" : "collapsed",
        expectsDenseOverview ? "dense" : "normal",
        hasLoadedCache ? "cache-ready" : "cache-pending",
        settings.menuBarDisplayMode,
        settings.menuBarShowsPercent ? "percent" : "no-percent",
        settings.trayScalePercent,
        visibleProviders.map((provider) => provider.providerId).join(","),
      ].join("|"),
    [
      selectedProviderId,
      gridExpanded,
      expectsDenseOverview,
      hasLoadedCache,
      settings.menuBarDisplayMode,
      settings.menuBarShowsPercent,
      settings.trayScalePercent,
      visibleProviders,
    ],
  );

  // The tray's height never auto-fits to provider card content (that caused
  // resize-on-switch flicker); it only changes when the user drags the top /
  // top-left resize grips. This is just the default until they do.
  const TRAY_DEFAULT_LOGICAL_HEIGHT = 540;
  const [flyoutSize, setFlyoutSizeState] = useState<
    [number, number] | null | undefined
  >(undefined);
  useEffect(() => {
    let active = true;
    void flyoutStoredSize()
      .then((size) => {
        if (active) setFlyoutSizeState(size);
      })
      .catch(() => {
        if (active) setFlyoutSizeState(null);
      });
    return () => {
      active = false;
    };
  }, []);

  const saveSizeTimerRef = useRef<number | undefined>(undefined);
  const handleUserResize = useCallback((width: number, height: number) => {
    // Persist both dimensions from a genuine drag on the left/top/top-left
    // grips; the layout hook re-applies this exact size on every future open.
    if (saveSizeTimerRef.current !== undefined) {
      window.clearTimeout(saveSizeTimerRef.current);
    }
    saveSizeTimerRef.current = window.setTimeout(() => {
      setFlyoutSizeState([width, height]);
      void setFlyoutSize(width, height).catch(() => {});
    }, 300);
  }, []);
  useEffect(
    () => () => {
      if (saveSizeTimerRef.current !== undefined) {
        window.clearTimeout(saveSizeTimerRef.current);
      }
    },
    [],
  );

  // TrayPanel now renders exclusively inside its own dedicated "flyout" OS
  // window (see App.tsx's isFlyoutWindow() routing) — it is no longer a
  // state of the shared `main` window's surface-mode machine. The old
  // `useSurfaceMode() === "trayPanel"` check would be permanently false
  // here (that machine now only tracks Hidden/PopOut/Settings on `main`),
  // which would silently gate off the fixed-size restore + reveal below
  // (useTrayPanelLayout's `isOpen` gate) — a user-resized flyout would never
  // reveal itself. Hardcoded true: being mounted IS "the flyout is open".
  const isFlyoutOpen = true;
  const fixedFlyoutSize = Array.isArray(flyoutSize) ? flyoutSize : null;
  const isMinimalMode = settings.menuBarDisplayMode === "minimal";
  const useWideColumns =
    selectedProviderId === null &&
    fixedFlyoutSize !== null &&
    fixedFlyoutSize[0] >= 640;
  const wideColumns = useMemo(() => {
    const columns: ProviderUsageSnapshot[][] = [[], []];
    visibleProviders.forEach((provider, index) => {
      columns[index % 2].push(provider);
    });
    return columns;
  }, [visibleProviders]);
  const { layoutReady, requestLayout } = useTrayPanelLayout({
    canMeasure: hasLoadedCache || sorted.length > 0,
    denseOverview: expectsDenseOverview,
    detailMode: selectedProviderId !== null,
    minimalOverview: isMinimalMode && selectedProviderId === null,
    layoutKey,
    autoFit: false,
    fixedSize: fixedFlyoutSize,
    fixedLogicalHeight: TRAY_DEFAULT_LOGICAL_HEIGHT,
    isOpen: isFlyoutOpen,
    onUserResize: handleUserResize,
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
    { icon: "⧉", label: t("TrayShowWindow"), onClick: openDashboard },
    { icon: "↻", label: t("ActionRefresh"), shortcut: "Ctrl+R", onClick: refresh },
    { icon: "⚙", label: t("MenuSettings"), shortcut: "Ctrl+,", onClick: openSettings },
    { icon: "⌧", label: t("MenuQuit"), shortcut: "Ctrl+Q", onClick: quitApp },
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
  const revealClassName = `tray-panel-reveal tray-panel-reveal--fixed-height${layoutReady ? " tray-panel-reveal--ready" : ""}${expectsDenseOverview ? " tray-panel-reveal--dense" : ""}${selectedProviderId !== null ? " tray-panel-reveal--detail" : ""}`;
  const trayScale = Math.min(
    2,
    Math.max(
      1,
      Number.isFinite(settings.trayScalePercent)
        ? settings.trayScalePercent / 100
        : 1,
    ),
  );
  const compactMetrics = settings.menuBarDisplayMode !== "detailed";
  // Minimal deliberately has its own render path. Reusing full MenuCards here
  // made its result indistinguishable from compact mode and, in a fixed-height
  // flyout, could leave users looking at an apparently empty scroll body.
  const showMinimalSummary = isMinimalMode && selectedProviderId === null;
  const renderProviderCard = (p: ProviderUsageSnapshot) => {
    const isSelected =
      selectedProviderId !== null && p.providerId === selectedProviderId;
    const speedProviderId = p.providerId === "codex" || p.providerId === "claude"
      ? p.providerId
      : null;
    return (
      <div
        className={`menu-stack__item${isSelected ? " menu-stack__item--selected" : ""}`}
        id={`card-${p.providerId}`}
        key={p.providerId}
      >
        <MenuCard
          provider={p}
          resetTimeRelative={settings.resetTimeRelative}
          showAsUsed={settings.showAsUsed}
          compactMetrics={compactMetrics}
          onLayoutChange={requestLayout}
          outputSpeed={speedProviderId ? outputSpeed?.[speedProviderId] : null}
          localUsagePeriod={settings.localUsagePeriod}
          hideLocalUsage={settings.menuBarDisplayMode !== "detailed"}
        />
      </div>
    );
  };

  useEffect(() => {
    if (!showMinimalSummary) return;
    // A mode switch must always reveal the first minimal rows, rather than
    // preserving a scroll offset from the preceding full-card overview.
    const body = document.querySelector<HTMLElement>(
      ".menu-surface--tray .menu-surface__body",
    );
    if (body) body.scrollTop = 0;
  }, [showMinimalSummary]);

  if (sorted.length === 0) {
    return (
      <div className={revealClassName}>
        <MenuSurface
          variant="tray"
          onRefresh={refresh}
          isRefreshing={isRefreshing}
          actions={[]}
          footerRows={footerRows}
          style={{ zoom: trayScale }}
        >
          <MenuEmpty
            isLoading={isRefreshing && !hasCachedData}
            onSettings={openSettings}
          />
        </MenuSurface>
        <TrayResizeHandles />
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
          style={{ zoom: trayScale }}
          fixedHeader={
            <ProviderGrid
              providers={expectsDenseOverview ? denseTrayProviders : sorted}
              selectedProviderId={selectedProviderId}
              showAsUsed={settings.showAsUsed}
              showProviderIcons={settings.switcherShowsIcons}
              showPercent={settings.menuBarShowsPercent}
              expanded={gridExpanded}
              onExpandedChange={setGridExpanded}
              onSelect={handleGridClick}
              onReorder={handleReorder}
              onGestureStart={handleGestureStart}
              onGestureEnd={handleGestureEnd}
            />
          }
        >
        <div className="provider-grid__divider" />
        {showMinimalSummary ? (
          <div className="tray-minimal-summary" aria-label={t("DisplayModeMinimal")}>
            {visibleProviders.map((provider) => {
              const status = getProviderStatus(provider);
              const usedPercent = Math.max(0, Math.min(100, provider.primary.usedPercent));
              const displayPercent = settings.showAsUsed
                ? usedPercent
                : 100 - usedPercent;
              // Balance-type providers (DeepSeek, MiMo without an active
              // token plan) synthesize a meaningless 0% primary window —
              // showing that bar/percent here just reads as "empty". Swap
              // in the parsed balance amount instead, same as MenuCard.
              const { balance, excludeWindows } = getProviderBalance(provider);
              const showBalance = excludeWindows.has("primary") && !!balance;
              return (
                <button
                  type="button"
                  className="tray-minimal-summary__row"
                  data-status={status}
                  key={provider.providerId}
                  onClick={() => handleGridClick(provider.providerId)}
                  title={provider.displayName}
                >
                  <span className="tray-minimal-summary__status" aria-hidden />
                  <span className="tray-minimal-summary__name">{provider.displayName}</span>
                  {showBalance ? (
                    <span
                      className="tray-minimal-summary__balance"
                      data-unavailable={balance.unavailable ? "true" : undefined}
                    >
                      {balance.amount}
                    </span>
                  ) : (
                    <>
                      <span className="tray-minimal-summary__bar" aria-hidden>
                        <span style={{ width: `${displayPercent}%` }} />
                      </span>
                      <span className="tray-minimal-summary__percent">
                        {Math.round(displayPercent)}%
                      </span>
                    </>
                  )}
                </button>
              );
            })}
          </div>
        ) : (
          <div className="menu-stack">
            {useWideColumns
              ? wideColumns.map((column, index) => (
                  <div className="menu-stack__column" key={index}>
                    {column.map(renderProviderCard)}
                  </div>
                ))
              : visibleProviders.map((p, idx) => (
                  <Fragment key={p.providerId}>
                    {idx > 0 && <div className="menu-stack__sep" />}
                    {renderProviderCard(p)}
                  </Fragment>
                ))}
          </div>
        )}
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
      <TrayResizeHandles />
    </div>
  );
}

/**
 * Invisible resize grip along the flyout's in-screen left edge. The flyout is
 * anchored bottom-right above the tray, so users can widen it without changing
 * its deliberately fixed height. Native edge-resize doesn't work through the
 * borderless WebView2, so we drive it explicitly with `startResizeDragging`.
 * That call enters a Win32 modal size loop which
 * transiently steals focus from the WebView2 child for its duration — Windows
 * fires a spurious `Focused(false)` the instant the press starts even though
 * the user never left the window. We arm a gesture-scoped blur guard on the
 * backend *before* starting the loop so that transient blur doesn't
 * auto-hide the flyout; the guard clears itself once focus genuinely returns
 * (via the `Focused(true)` refocus path) or after a 15s expiry, so no
 * explicit end call is needed here — the OS loop swallows mouseup.
 */
function TrayResizeHandles() {
  const startDrag = (direction: "North" | "West" | "NorthWest") => (
    e: React.MouseEvent<HTMLDivElement>,
  ) => {
    e.preventDefault();
    void (async () => {
      await beginFlyoutGesture().catch(() => {});
      await getCurrentWindow().startResizeDragging(direction);
    })().catch((err) => console.error("[tray-resize] startResizeDragging failed:", err));
  };
  return (
    <>
      <div className="tray-resize tray-resize--top" aria-hidden onMouseDown={startDrag("North")} />
      <div className="tray-resize tray-resize--left" aria-hidden onMouseDown={startDrag("West")} />
      <div className="tray-resize tray-resize--topleft" aria-hidden onMouseDown={startDrag("NorthWest")} />
    </>
  );
}
