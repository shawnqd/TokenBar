import { useCallback, useEffect, useRef, useState } from "react";
import type {
  BootstrapState,
  SettingsTabId,
  SettingsUpdate,
} from "../types/bridge";
import { useSettings } from "../hooks/useSettings";
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
      setToast("error");
      return;
    }
    if (saving) setToast("saving");
  }, [saving, error]);

  useEffect(() => {
    const onSaved = () => {
      setToast("saved");
      if (savedTimerRef.current) window.clearTimeout(savedTimerRef.current);
      savedTimerRef.current = window.setTimeout(() => {
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
      if (savedTimerRef.current) window.clearTimeout(savedTimerRef.current);
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
  const tabProps: TabProps = { settings, set, saving, coreStore, dispatcher };
  const isProviders = activeTab === "providers";

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
            isProviders ? " settings-v5__workspace--providers" : ""
          }`}
        >
          <SaveToast state={toast} error={error} />
          <div
            ref={panelRef}
            className={`settings-v5__panel${
              isProviders ? " settings-v5__panel--providers" : ""
            }${initialPanelRef.current ? " settings-v5__panel--initial" : ""}`}
          >
            <div
              className={`settings-page${
                isProviders ? " settings-page--providers" : ""
              }`}
              data-settings-page={activeTab}
              data-settings-tab-content={activeTab}
            >
              {!isProviders && (
                <SettingsPageHead
                  eyebrow={t(copy.eyebrowKey)}
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
              {activeTab === "floatBar" && <FloatBarPage {...tabProps} />}
              {activeTab === "taskbarStatus" && (
                <TaskbarStatusPage {...tabProps} />
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
  settings: BootstrapState["settings"];
  set: (p: SettingsUpdate) => void;
  saving: boolean;
  coreStore?: UsageStore | null;
  dispatcher?: ActionDispatcher | null;
}
