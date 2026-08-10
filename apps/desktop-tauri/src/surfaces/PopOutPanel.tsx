import { Fragment, useCallback, useEffect, useMemo, useRef, useState } from "react";
import type {
  BootstrapState,
  MenuBarDisplayMode,
  ProviderUsageSnapshot,
} from "../types/bridge";
import { openFlyoutWindow, openSettingsWindow, quitApp as quitApplication, reorderProviders } from "../lib/tauri";
import { useProviders } from "../hooks/useProviders";
import { useSettings } from "../hooks/useSettings";
import { useLocale } from "../hooks/useLocale";
import { useOutputSpeedSnapshot } from "../hooks/useOutputSpeedSnapshot";
import MenuCard from "../components/MenuCard";
import PopOutTitleBar from "../components/PopOutTitleBar";
import MenuSurface, {
  MenuEmpty,
  type MenuFooterRow,
} from "../components/MenuSurface";
import ProviderGrid, { prioritizeProviders } from "../components/ProviderGrid";
import { orderProviderSnapshots } from "../lib/providerOrder";
import { quotaDisplayContext } from "../lib/quotaDisplay";
import { outputSpeedProviderId } from "../lib/outputSpeed";

/**
 * Pop-out window — dashboard and provider deep-links both keep the full card
 * stack. A provider target only scrolls/focuses the requested card so the
 * layout stays consistent with the tray/menu surface.
 */
export default function PopOutPanel({
  state,
  providerId,
}: {
  state: BootstrapState;
  providerId?: string;
}) {
  const {
    providers,
    isRefreshing,
    refresh,
    hasCachedData,
  } = useProviders();
  const { settings } = useSettings(state.settings);
  const { t } = useLocale();
  // The PopOut dashboard shares the "dashboard" component's settings with the
  // tray flyout; it never reads the floating bar's or taskbar strip's choice.
  const display = useMemo(
    () => quotaDisplayContext(settings, "dashboard"),
    [settings],
  );
  const outputSpeed = useOutputSpeedSnapshot(
    settings.outputSpeedEnabled !== false,
  );

  // Provider switches are the single source of truth for both dashboard
  // surfaces. This also prevents a stale legacy dashboard-only filter from
  // hiding an enabled provider.
  const shownProviderIds = settings.enabledProviders;
  const sorted = useMemo(() => {
    return orderProviderSnapshots(
      providers.filter((provider) => shownProviderIds.includes(provider.providerId)),
      state.providers,
      settings.enabledProviders,
      settings.providerOrder,
    );
  }, [
    providers,
    shownProviderIds,
    settings.enabledProviders,
    settings.providerOrder,
    state.providers,
  ]);
  const [selectedProviderId, setSelectedProviderId] = useState<string | null>(
    providerId ?? null,
  );
  // TASK-021 item 1: same density path as the tray panel. Overview follows
  // menuBarDisplayMode; a single-provider selection always shows full detail.
  const densityMode: MenuBarDisplayMode =
    selectedProviderId !== null ? "detailed" : settings.menuBarDisplayMode;
  const [gridExpanded, setGridExpanded] = useState(false);
  const cardRefs = useRef(new Map<string, HTMLDivElement>());
  useEffect(() => {
    setSelectedProviderId(providerId ?? null);
  }, [providerId]);

  const visibleProviders = useMemo(
    () => {
      if (selectedProviderId === null) {
        if (sorted.length + 1 > 32 && !gridExpanded) {
          return prioritizeProviders(sorted, null).slice(0, 4);
        }
        return sorted;
      }
      const match = sorted.find((p) => p.providerId === selectedProviderId);
      return match ? [match] : sorted;
    },
    [sorted, selectedProviderId, gridExpanded],
  );
  const providerOrderKey = useMemo(
    () => sorted.map((provider) => provider.providerId).join(","),
    [sorted],
  );

  const handleGridClick = useCallback((nextProviderId: string | null) => {
    setSelectedProviderId(nextProviderId);
  }, []);
  const handleReorder = useCallback((orderedIds: string[]) => {
    void reorderProviders(orderedIds).catch(() => {});
  }, []);

  useEffect(() => {
    if (!providerId || selectedProviderId !== providerId || providerOrderKey.length === 0) return;

    let cancelled = false;
    const scrollToProvider = () => {
      if (cancelled) return;
      const target = cardRefs.current.get(providerId);
      if (!target) return;

      window.scrollTo(0, 0);
      if (document.scrollingElement) {
        document.scrollingElement.scrollTop = 0;
      }
      document.documentElement.scrollTop = 0;
      document.body.scrollTop = 0;

      for (const selector of [".menu-stack", ".menu-surface__body"]) {
        const container = target.closest<HTMLElement>(selector);
        if (!container) continue;
        container.scrollTop = 0;
        const targetRect = target.getBoundingClientRect();
        const containerRect = container.getBoundingClientRect();
        container.scrollTop += targetRect.top - containerRect.top;
      }
    };

    requestAnimationFrame(() => {
      requestAnimationFrame(scrollToProvider);
    });
    const timer = window.setTimeout(scrollToProvider, 100);
    const lateTimer = window.setTimeout(scrollToProvider, 350);
    return () => {
      cancelled = true;
      window.clearTimeout(timer);
      window.clearTimeout(lateTimer);
    };
  }, [providerId, selectedProviderId, providerOrderKey]);

  const openSettings = useCallback(() => {
    openSettingsWindow("general");
  }, []);
  const goTray = useCallback(() => {
    // The "Open Tray Panel" surface is now its own dedicated OS window
    // rather than a state of the shared `main` window's surface-mode
    // machine, so "back to tray" opens it directly instead of switching
    // `main`'s mode.
    void openFlyoutWindow().catch(() => {});
  }, []);
  const openAbout = useCallback(() => {
    openSettingsWindow("about");
  }, []);
  const quitApp = useCallback(() => {
    void quitApplication();
  }, []);

  const headerActions = [
    { icon: "⊟", title: t("TooltipBackToTray"), onClick: goTray },
  ];

  const footerRows: MenuFooterRow[] = [
    { icon: "⚙", label: t("TooltipSettings"), shortcut: "Ctrl+,", onClick: openSettings },
    { icon: "ℹ", label: t("MenuAbout"), onClick: openAbout },
    { icon: "✕", label: t("MenuQuit"), shortcut: "Ctrl+Q", onClick: quitApp },
  ];

  // Keyboard shortcuts
  useEffect(() => {
    const handler = (e: KeyboardEvent) => {
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

  const surface = sorted.length === 0 ? (
    <MenuSurface
      variant="popout"
      titleBar={<PopOutTitleBar />}
      onRefresh={refresh}
      isRefreshing={isRefreshing}
      actions={headerActions}
      footerRows={footerRows}
    >
      <MenuEmpty
        isLoading={isRefreshing && !hasCachedData}
        onSettings={openSettings}
      />
    </MenuSurface>
  ) : (
    <MenuSurface
      variant="popout"
      titleBar={<PopOutTitleBar />}
      onRefresh={refresh}
      isRefreshing={isRefreshing}
      actions={headerActions}
      footerRows={footerRows}
    >
      <ProviderGrid
        providers={sorted}
        selectedProviderId={selectedProviderId}
        display={display}
        showProviderIcons={settings.switcherShowsIcons}
        expanded={gridExpanded}
        onExpandedChange={setGridExpanded}
        onSelect={handleGridClick}
        onReorder={handleReorder}
      />
      <div className="provider-grid__divider" />
      <div className="menu-stack">
        {visibleProviders.map((p, idx) => (
          <Fragment key={p.providerId}>
            {idx > 0 && <div className="menu-stack__sep" />}
            <div
              className={`menu-stack__item${selectedProviderId === p.providerId ? " menu-stack__item--selected" : ""}`}
              ref={(node) => {
                if (node) {
                  cardRefs.current.set(p.providerId, node);
                } else {
                  cardRefs.current.delete(p.providerId);
                }
              }}
            >
              <MenuCard
                provider={p}
                display={display}
                localUsagePeriod={settings.localUsagePeriod}
                showProviderIcon={settings.switcherShowsIcons}
                densityMode={densityMode}
                outputSpeed={
                  (() => {
                    const speedId = outputSpeedProviderId(p.providerId);
                    return speedId ? outputSpeed?.[speedId] : null;
                  })()
                }
              />
            </div>
          </Fragment>
        ))}
      </div>
    </MenuSurface>
  );

  return (
    <div className="popout-scale-shell">
      {surface}
    </div>
  );
}
