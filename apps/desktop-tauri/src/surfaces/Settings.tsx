import type { ReactNode } from "react";
import { useCallback, useEffect, useRef, useState } from "react";
import type {
  BootstrapState,
  SettingsTabId,
  SettingsUpdate,
} from "../types/bridge";
import { useSettings, SAVE_TIMEOUT_ERROR } from "../hooks/useSettings";
import { useSurfaceTarget } from "../hooks/useSurfaceMode";
import { useLocale } from "../hooks/useLocale";
import { useDispatchAction } from "../core/useCoreBridge";
import { requireActionResult } from "../core/actionDispatcher";
import type { UsageStore } from "../core/usageStore";
import type { ActionDispatcher } from "../core/actionDispatcher";
import { minimizeSettingsWindow } from "../lib/tauri";
import SettingsNav from "./settings/SettingsNav";
import SettingsPageHead from "./settings/SettingsPageHead";
import SaveToast, { type SaveToastState } from "./settings/SaveToast";
import {
  canonicalizeSettingsTab,
  isSettingsTab,
  SETTINGS_PAGE_COPY,
  type SettingsNavId,
} from "./settings/settingsTabs";
import {
  AboutPage,
  AdvancedPage,
  AppearancePage,
  FloatBarPage,
  GeneralPage,
  NotificationsPage,
  PrivacyPage,
  ProvidersPage,
  TaskbarStatusPage,
  TrayPanelPage,
} from "./settings/pages";
import "./settings/settings-v5.css";
import "./settings/settings-v5-html.css";
import "./settings/settings-v5-tray.css";
import "./settings/settings-v5-floatbar.css";
import "./settings/settings-v5-taskbar.css";

export type { SettingsNavId };

// Window geometry for the detached Settings surface is owned exclusively by
// `shell/settings_window.rs` (create size + optional user drag-resize). The
// frontend must never call setSize/setPosition here.

function prefersReducedMotion(): boolean {
  return Boolean(
    typeof window !== "undefined" &&
      window.matchMedia?.("(prefers-reduced-motion: reduce)").matches,
  );
}

export default function Settings({
  state,
  initialTab: propTab,
  onRequestClose,
  windowMotion,
  coreStore,
  dispatcher,
}: {
  state: BootstrapState;
  initialTab?: string;
  onRequestClose?: () => void;
  windowMotion?: "idle" | "visible" | "closing";
  coreStore?: UsageStore | null;
  dispatcher?: ActionDispatcher | null;
}) {
  const { settings, saving, error, update } = useSettings(state.settings);
  const { t } = useLocale();
  const dispatch = useDispatchAction(dispatcher ?? undefined);
  const shellTarget = useSurfaceTarget("settings");
  const resolvedInitial: SettingsNavId =
    propTab && isSettingsTab(propTab)
      ? canonicalizeSettingsTab(propTab)
      : shellTarget?.kind === "settings" && isSettingsTab(shellTarget.tab)
        ? canonicalizeSettingsTab(shellTarget.tab)
        : "general";
  const [activeTab, setActiveTab] = useState<SettingsNavId>(resolvedInitial);
  const initialPanelRef = useRef(true);
  const activeTabRef = useRef<SettingsNavId>(resolvedInitial);
  const panelRef = useRef<HTMLDivElement>(null);
  const savedTimerRef = useRef<number | null>(null);
  const errorTimerRef = useRef<number | null>(null);
  const fadeSourceRef = useRef<"pointer" | "keyboard" | "external">("external");
  const [toast, setToast] = useState<SaveToastState>("hidden");

  const motionEnabled = settings.enableAnimations && !prefersReducedMotion();

  const changeTab = useCallback(
    (next: SettingsTabId, source: "pointer" | "keyboard" | "external" = "external") => {
      const canonical = canonicalizeSettingsTab(next);
      if (activeTabRef.current === canonical) return;
      initialPanelRef.current = false;
      fadeSourceRef.current = source;
      activeTabRef.current = canonical;
      setActiveTab(canonical);
    },
    [],
  );

  useEffect(() => {
    const panel = panelRef.current;
    if (!panel || initialPanelRef.current) return;
    const animate = fadeSourceRef.current === "pointer" && motionEnabled;
    if (!animate) {
      panel.style.transition = "none";
      panel.style.opacity = "1";
      return;
    }
    panel.style.transition = "none";
    panel.style.opacity = "0";
    const frame = requestAnimationFrame(() => {
      panel.style.transition = "opacity 240ms var(--ease-page)";
      panel.style.opacity = "1";
    });
    return () => cancelAnimationFrame(frame);
  }, [activeTab, motionEnabled]);

  useEffect(() => {
    if (propTab && isSettingsTab(propTab)) {
      changeTab(propTab, "external");
    }
  }, [propTab, changeTab]);

  useEffect(() => {
    if (shellTarget?.kind !== "settings" || !isSettingsTab(shellTarget.tab)) {
      return;
    }
    changeTab(shellTarget.tab, "external");
  }, [shellTarget, changeTab]);

  useEffect(() => {
    if (error) {
      if (savedTimerRef.current !== null) {
        window.clearTimeout(savedTimerRef.current);
        savedTimerRef.current = null;
      }
      setToast("error");
      // An error pill must inform, not occupy: auto-dismiss like "saved",
      // just with a longer read time. The `error` state itself clears on the
      // next update attempt.
      if (errorTimerRef.current !== null) {
        window.clearTimeout(errorTimerRef.current);
      }
      errorTimerRef.current = window.setTimeout(() => {
        errorTimerRef.current = null;
        setToast((current) => (current === "error" ? "hidden" : current));
      }, 6000);
      return;
    }

    if (saving) {
      if (savedTimerRef.current !== null) {
        window.clearTimeout(savedTimerRef.current);
        savedTimerRef.current = null;
      }
      if (errorTimerRef.current !== null) {
        window.clearTimeout(errorTimerRef.current);
        errorTimerRef.current = null;
      }
      setToast("saving");
      return;
    }

    // A successful save normally emits `codexbar:settings-updated`, but a
    // cross-window settings event can supersede that response. In that case
    // `saving` is still the authoritative completion signal; never leave the
    // transient "saving" pill mounted just because the success event was
    // skipped.
    setToast((current) =>
      current === "saving" || current === "error" ? "hidden" : current,
    );
  }, [saving, error]);

  useEffect(() => {
    const onSaved = () => {
      if (errorTimerRef.current !== null) {
        window.clearTimeout(errorTimerRef.current);
        errorTimerRef.current = null;
      }
      setToast("saved");
      if (savedTimerRef.current !== null) {
        window.clearTimeout(savedTimerRef.current);
      }
      savedTimerRef.current = window.setTimeout(() => {
        savedTimerRef.current = null;
        setToast((current) => (current === "saved" ? "hidden" : current));
      }, 1400);
    };
    window.addEventListener("codexbar:settings-updated", onSaved);
    return () => {
      window.removeEventListener("codexbar:settings-updated", onSaved);
    };
  }, []);

  useEffect(() => {
    return () => {
      if (savedTimerRef.current !== null) window.clearTimeout(savedTimerRef.current);
      if (errorTimerRef.current !== null) window.clearTimeout(errorTimerRef.current);
    };
  }, []);

  useEffect(() => {
    const onKey = (event: KeyboardEvent) => {
      if (event.key !== "Escape") return;
      if (document.querySelector("[data-settings-dialog]")) return;
      event.preventDefault();
      if (onRequestClose) onRequestClose();
      else void requireActionResult(dispatch({ type: "closeSettings", target: { kind: "settings" } }));
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [onRequestClose]);

  const set = (patch: SettingsUpdate) => void update(patch);
  const handleNavSelect = useCallback(
    (tab: SettingsNavId, source: "pointer" | "keyboard") => {
      changeTab(tab, source);
    },
    [changeTab],
  );

  const copy = SETTINGS_PAGE_COPY[activeTab];
  const isProviders = activeTab === "providers";
  // 带实时预览的三个页面：预览列是独立滚动区域，页面本体占满显示高度。
  const isSurfPreview =
    activeTab === "trayPanel" ||
    activeTab === "floatBar" ||
    activeTab === "taskbarStatus";

  // 预览三页：页头放进左列顶部（预览列从页面顶端开始，右侧不再空一截）。
  const surfHead = isSurfPreview ? (
    <SettingsPageHead
      eyebrow={copy.eyebrowKey ? t(copy.eyebrowKey) : undefined}
      title={t(copy.titleKey)}
      description={
        copy.descriptionKey ? t(copy.descriptionKey) : undefined
      }
    />
  ) : null;
  const tabProps: TabProps = {
    settings,
    set,
    saving,
    coreStore,
    dispatcher,
    head: surfHead,
  };
  return (
    <div
      className={`settings settings-v5${
        windowMotion ? ` settings-window-motion--${windowMotion}` : ""
      }`}
      data-motion={motionEnabled ? "on" : "off"}
    >
      <div className="settings-titlebar settings-v5__titlebar" data-tauri-drag-region>
        <span className="settings-titlebar__title" data-tauri-drag-region>
          {t("SettingsWindowTitle")}
        </span>
        <div className="settings-titlebar__controls">
          <button
            className="settings-titlebar__control settings-titlebar__control--minimize"
            onClick={() => void minimizeSettingsWindow()}
            aria-label={t("WindowMinimize")}
            title={t("WindowMinimize")}
          />
          <button
            className="settings-titlebar__control settings-titlebar__control--close"
            onClick={() =>
              onRequestClose
                ? onRequestClose()
                : void requireActionResult(
                    dispatch({ type: "closeSettings", target: { kind: "settings" } }),
                  )
            }
            aria-label={t("WindowClose")}
            title={t("WindowClose")}
          >
            <svg aria-hidden viewBox="0 0 16 16" focusable="false">
              <path d="M4.5 4.5l7 7M11.5 4.5l-7 7" />
            </svg>
          </button>
        </div>
      </div>

      <div className="settings-v5__shell">
        <SettingsNav active={activeTab} onSelect={handleNavSelect} />

        <div
          className={`settings-v5__workspace${
            isProviders
              ? " settings-v5__workspace--providers"
              : isSurfPreview
                ? " settings-v5__workspace--surf"
                : ""
          }`}
        >
          <SaveToast
            state={toast}
            error={error === SAVE_TIMEOUT_ERROR ? t("SettingsStatusSaveTimeout") : error}
          />
          <div
            ref={panelRef}
            className={`settings-v5__panel${
              isProviders
                ? " settings-v5__panel--providers"
                : isSurfPreview
                  ? " settings-v5__panel--surf"
                  : ""
            }${initialPanelRef.current ? " settings-v5__panel--initial" : ""}`}
          >
            <div
              className={`settings-page${
                isProviders
                  ? " settings-page--providers"
                  : isSurfPreview
                    ? " settings-page--surf"
                    : ""
              }`}
              data-settings-page={activeTab}
              data-settings-tab-content={activeTab}
            >
              {!isProviders && !isSurfPreview && (
                <SettingsPageHead
                  eyebrow={copy.eyebrowKey ? t(copy.eyebrowKey) : undefined}
                  title={t(copy.titleKey)}
                  description={
                    copy.descriptionKey ? t(copy.descriptionKey) : undefined
                  }
                />
              )}
              {activeTab === "general" && <GeneralPage {...tabProps} />}
              {activeTab === "providers" && (
                <ProvidersPage {...tabProps} catalog={state.providers} />
              )}
              {activeTab === "trayPanel" && <TrayPanelPage {...tabProps} />}
              {activeTab === "floatBar" && (
                <FloatBarPage {...tabProps} catalog={state.providers} />
              )}
              {activeTab === "taskbarStatus" && (
                <TaskbarStatusPage {...tabProps} catalog={state.providers} />
              )}
              {activeTab === "notifications" && (
                <NotificationsPage {...tabProps} />
              )}
              {activeTab === "appearance" && <AppearancePage {...tabProps} />}
              {activeTab === "privacy" && <PrivacyPage {...tabProps} />}
              {activeTab === "advanced" && <AdvancedPage {...tabProps} />}
              {activeTab === "about" && <AboutPage {...tabProps} />}
            </div>
          </div>
        </div>
      </div>
    </div>
  );
}

export interface TabProps {
  /** 预览三页：由 Settings 下发的页头节点，放进左列顶部。 */
  head?: ReactNode;
  settings: BootstrapState["settings"];
  set: (p: SettingsUpdate) => void;
  saving: boolean;
  coreStore?: UsageStore | null;
  dispatcher?: ActionDispatcher | null;
}
