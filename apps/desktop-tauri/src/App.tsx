import { Suspense, lazy, useCallback, useEffect, useRef, useState } from "react";
import { listen } from "@tauri-apps/api/event";
import { getCurrentWebviewWindow } from "@tauri-apps/api/webviewWindow";
import {
  checkForUpdates,
  closeSettingsWindow,
  downloadUpdate,
  getBootstrapState,
  getSettingsSnapshot,
  revealSettingsWindow,
  setSurfaceMode,
} from "./lib/tauri";
import { useSurfaceSnapshot } from "./hooks/useSurfaceSnapshot";
import { useTheme } from "./hooks/useTheme";
import TrayPanel from "./surfaces/TrayPanel";
import { FLOATBAR_WINDOW_LABEL } from "./floatbar/api";
import { LocaleProvider } from "./i18n/LocaleProvider";
import type { BootstrapState, ThemePreference } from "./types/bridge";
import type { SurfaceSnapshot } from "./hooks/useSurfaceSnapshot";

/** Matches `shell::settings_window::SETTINGS_REVEALED_EVENT`. */
const SETTINGS_REVEALED_EVENT = "settings-window-revealed";

const Settings = lazy(() => import("./surfaces/Settings"));
const PopOutPanel = lazy(() => import("./surfaces/PopOutPanel"));
const FloatBar = lazy(() => import("./floatbar/FloatBar"));

function SurfaceFallback() {
  return null;
}

/** True when running inside the detached Settings window. */
function isSettingsWindow(): boolean {
  return getCurrentWebviewWindow().label === "settings";
}

/** True when running inside the detached FloatBar window. */
function isFloatBarWindow(): boolean {
  return getCurrentWebviewWindow().label === FLOATBAR_WINDOW_LABEL;
}

/** True when running inside the detached "Open Tray Panel" window. */
function isFlyoutWindow(): boolean {
  return getCurrentWebviewWindow().label === "flyout";
}

/** Parse the initial Settings tab from the URL query string. */
function initialSettingsTab(): string {
  const params = new URLSearchParams(window.location.search);
  return params.get("tab") || "general";
}

export default function App() {
  return (
    <LocaleProvider>
      <AppInner />
    </LocaleProvider>
  );
}

function AppInner() {
  const surface = useSurfaceSnapshot();
  const [state, setState] = useState<BootstrapState | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [themePreference, setThemePreference] = useState<ThemePreference>("dark");

  useTheme(themePreference);

  const reloadBootstrapState = useCallback(
    () => getBootstrapState(),
    [],
  );

  useEffect(() => {
    let cancelled = false;

    reloadBootstrapState()
      .then((bootstrap) => {
        if (cancelled) {
          return;
        }
        setState(bootstrap);
        setThemePreference(bootstrap.settings.theme);
        setError(null);
      })
      .catch((cause: unknown) => {
        if (!cancelled) {
          setError(cause instanceof Error ? cause.message : String(cause));
        }
      });

    // Fire-and-forget update checks after the first paint so startup/tray open
    // is not competing with network work.
    const updateTimer = window.setTimeout(() => {
      Promise.all([checkForUpdates(), getSettingsSnapshot()])
        .then(([update, settings]) => {
          if (settings.autoDownloadUpdates && update.canDownload) {
            void downloadUpdate().catch(() => {});
          }
        })
        .catch(() => {});
    }, 2_000);

    // Listen for user-registered global shortcut events from the
    // `register_global_shortcut` command. The persistent shortcut (bound via
    // shortcut_bridge::plugin) already opens the PopOut dashboard natively;
    // this listener is the fallback for ad-hoc capture-mode registrations.
    const unlistenPromise = listen<string>("global-shortcut-triggered", () => {
      void setSurfaceMode("popOut", { kind: "dashboard" }).catch(() => {});
    });

    const unlistenSettingsChangePromise = isSettingsWindow()
      ? listen<string>("settings-change-tab", () => {
          void reloadBootstrapState()
            .then((bootstrap) => {
              setState(bootstrap);
              setThemePreference(bootstrap.settings.theme);
              setError(null);
            })
            .catch(() => {});
        })
      : Promise.resolve(null);

    // Every Tauri surface is its own WebView. A DOM CustomEvent only reaches
    // the window that changed the setting, while Rust broadcasts this event to
    // every open surface after persistence. Subscribe here, at the app root,
    // so the tray panel, pop-out dashboard and detached flyout immediately
    // apply the selected light/dark/auto theme as well.
    const unlistenThemeSyncPromise = listen("settings-changed", () => {
      void getSettingsSnapshot()
        .then((fresh) => setThemePreference(fresh.theme))
        .catch(() => {});
    });

    // Keep the theme in sync when mutations happen inside other surfaces
    // (e.g., Settings → Appearance). `useSettings` dispatches this event
    // after every successful `updateSettings` call.
    const onSettingsUpdated = (evt: Event) => {
      const detail = (evt as CustomEvent<BootstrapState["settings"]>).detail;
      if (detail) {
        setThemePreference(detail.theme);
      } else {
        getSettingsSnapshot()
          .then((fresh) => setThemePreference(fresh.theme))
          .catch(() => {});
      }
    };
    window.addEventListener("codexbar:settings-updated", onSettingsUpdated);

    return () => {
      cancelled = true;
      void unlistenPromise.then((unlisten) => unlisten()).catch(() => {});
      void unlistenSettingsChangePromise
        .then((unlisten) => unlisten?.())
        .catch(() => {});
      void unlistenThemeSyncPromise.then((unlisten) => unlisten()).catch(() => {});
      window.clearTimeout(updateTimer);
      window.removeEventListener("codexbar:settings-updated", onSettingsUpdated);
    };
  }, [reloadBootstrapState]);

  if (error) {
    return (
      <main className="shell">
        <section className="panel error">
          <h2>Bootstrap failed</h2>
          <p>{error}</p>
        </section>
      </main>
    );
  }

  if (!state) {
    return (
      <main className="shell">
        <section className="panel">
          <h2>Loading shell contract…</h2>
          <p>Waiting for the Rust bridge to describe providers, surfaces, and settings.</p>
        </section>
      </main>
    );
  }

  // Detached settings window — render Settings directly, skip SurfaceRouter.
  if (isSettingsWindow()) {
    return <DetachedSettingsApp state={state} />;
  }

  // Detached floating-bar window — render the FloatBar surface directly.
  if (isFloatBarWindow()) {
    return (
      <Suspense fallback={<SurfaceFallback />}>
        <FloatBar state={state} />
      </Suspense>
    );
  }

  // Detached "Open Tray Panel" window — render TrayPanel directly.
  // TrayPanel is statically imported (not lazy), so no Suspense boundary is
  // needed here, unlike the other detached-window branches above.
  if (isFlyoutWindow()) {
    return <TrayPanel state={state} />;
  }

  return <SurfaceRouter surface={surface} state={state} />;
}

function SurfaceRouter({
  surface,
  state,
}: {
  surface: SurfaceSnapshot;
  state: BootstrapState;
}) {
  switch (surface.mode) {
    case "hidden":
      return null;
    // "trayPanel" was removed from this switch: `main`'s surface-mode
    // machine can no longer enter `SurfaceMode::TrayPanel` at all — the tray
    // panel is its own dedicated `flyout` window now (see
    // `isFlyoutWindow()` above), `commands::set_surface_mode` rejects a
    // `trayPanel` request outright, and proof mode routes it to the flyout
    // window too. This case was therefore unreachable dead code.
    case "popOut": {
      const providerId =
        surface.target.kind === "provider"
          ? surface.target.providerId
          : undefined;
      return (
        <Suspense fallback={<SurfaceFallback />}>
          <PopOutPanel state={state} providerId={providerId} />
        </Suspense>
      );
    }
    // Settings is the detached `settings` WebView only. Putting it on `main`
    // produced the blank title-bar window (`CodexBar 设置` with no body).
    case "settings":
    default:
      // Unknown or future modes must fail closed. Falling back to TrayPanel
      // here would silently recreate a second tray UI inside the main window.
      return null;
  }
}

function DetachedSettingsApp({ state }: { state: BootstrapState }) {
  const [tab, setTab] = useState(initialSettingsTab);

  useEffect(() => {
    // Listen for tab-change events from Rust (when the window is re-focused
    // with a different tab request).
    const unlisten = listen<string>("settings-change-tab", (event) => {
      setTab(event.payload);
    });
    return () => {
      unlisten.then((fn) => fn());
    };
  }, []);

  return (
    <Suspense fallback={<SurfaceFallback />}>
      <DetachedSettingsReadyContent state={state} tab={tab} />
    </Suspense>
  );
}

function DetachedSettingsReadyContent({
  state,
  tab,
}: {
  state: BootstrapState;
  tab: string;
}) {
  const frameRef = useRef<HTMLElement>(null);
  const closingRef = useRef(false);
  const closeTimerRef = useRef<ReturnType<typeof setTimeout> | null>(null);
  const [windowMotion, setWindowMotion] = useState<
    "idle" | "visible" | "closing"
  >("idle");

  const cancelCloseAnimation = useCallback(() => {
    if (closeTimerRef.current) {
      clearTimeout(closeTimerRef.current);
      closeTimerRef.current = null;
    }
    closingRef.current = false;
    setWindowMotion("visible");
  }, []);

  const requestClose = useCallback(() => {
    if (closingRef.current) return;
    closingRef.current = true;
    if (window.matchMedia("(prefers-reduced-motion: reduce)").matches) {
      closingRef.current = false;
      setWindowMotion("idle");
      void closeSettingsWindow();
      return;
    }

    setWindowMotion("closing");
    closeTimerRef.current = setTimeout(() => {
      closeTimerRef.current = null;
      void closeSettingsWindow()
        .catch(() => undefined)
        .finally(() => {
          closingRef.current = false;
          setWindowMotion("idle");
        });
    }, 140);
  }, []);

  useEffect(() => {
    // The native Settings window is prewarmed hidden. Keep this outer frame
    // permanently painted instead of using its opacity as a second visibility
    // state. The old approach parked the frame at opacity 0 and then raced the
    // native show/event handshake; a lost event or HMR remount made the window
    // appear blank or flash. Native hide/show is the single source of truth.
    let disposed = false;
    const unlistens: Array<() => void> = [];

    void (async () => {
      // The window is prewarmed and re-shown rather than recreated. Native
      // hide/show remains authoritative; the CSS class below is only a small
      // visual transition and never gates the first paint.
      unlistens.push(
        await listen(SETTINGS_REVEALED_EVENT, () => {
          cancelCloseAnimation();
        }),
      );
      // Re-opening while the close fade is still running must cancel the
      // pending native hide. `open_or_focus` emits this event before it checks
      // visibility, so this also covers the short close/reopen race.
      unlistens.push(
        await listen("settings-change-tab", cancelCloseAnimation),
      );

      if (disposed) return;

      // Strictly after listeners are live, complete the native prewarm
      // handshake. If the window is already visible (HMR/remount), the Rust
      // command returns true and the same visible state is restored.
      const nativeRevealCompleted = await revealSettingsWindow().catch(() => false);
      if (!disposed && nativeRevealCompleted) {
        cancelCloseAnimation();
      }
    })();

    return () => {
      disposed = true;
      if (closeTimerRef.current) clearTimeout(closeTimerRef.current);
      closeTimerRef.current = null;
      closingRef.current = false;
      for (const unlisten of unlistens) unlisten();
    };
  }, [cancelCloseAnimation]);

  return (
    <main
      ref={frameRef}
      className="settings-surface settings-surface--full settings-window-frame"
      style={{ opacity: 1, transform: "none" }}
    >
      <Settings
        state={state}
        initialTab={tab}
        onRequestClose={requestClose}
        windowMotion={windowMotion}
      />
    </main>
  );
}
