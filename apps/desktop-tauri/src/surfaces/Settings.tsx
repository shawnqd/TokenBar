import { useCallback, useEffect, useRef, useState, type ReactElement, type ReactNode } from "react";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { getCurrentWebviewWindow } from "@tauri-apps/api/webviewWindow";
import type {
  BootstrapState,
  SettingsTabId,
  SettingsUpdate,
} from "../types/bridge";
import { useSettings } from "../hooks/useSettings";
import { useSurfaceTarget } from "../hooks/useSurfaceMode";
import { useLocale } from "../hooks/useLocale";
import type { LocaleKey } from "../i18n/keys";
import { closeSettingsWindow, setSurfaceMode } from "../lib/tauri";
import GeneralTab from "./settings/tabs/GeneralTab";
import DashboardTab from "./settings/tabs/DashboardTab";
import DisplayTab from "./settings/tabs/DisplayTab";
import FloatBarTab from "./settings/tabs/FloatBarTab";
import AdvancedTab from "./settings/tabs/AdvancedTab";
import AboutTab from "./settings/tabs/AboutTab";
import ProvidersTab from "./settings/tabs/ProvidersTab";
import TaskbarTab from "./settings/tabs/TaskbarTab";

// ── tab types ────────────────────────────────────────────────────────

type SettingsTab = SettingsTabId;

// Inline monochrome SVG icons stand in for the upstream macOS SF Symbols
// (gearshape / square.grid.2x2 / eye / slider.horizontal.3 / info.circle).
// They render in `currentColor` so they pick up the same secondary/accent
// text color as the tab label.
const ICON_SIZE = 16;

function Svg({ children }: { children: ReactNode }) {
  return (
    <svg
      width={ICON_SIZE}
      height={ICON_SIZE}
      viewBox="0 0 16 16"
      fill="none"
      stroke="currentColor"
      strokeWidth={1.4}
      strokeLinecap="round"
      strokeLinejoin="round"
      aria-hidden
    >
      {children}
    </svg>
  );
}

const TabIcons: Record<SettingsTab, ReactElement> = {
  general: (
    <Svg>
      <circle cx="8" cy="8" r="2" />
      <path d="M8 1.5v2M8 12.5v2M1.5 8h2M12.5 8h2M3.4 3.4l1.4 1.4M11.2 11.2l1.4 1.4M3.4 12.6l1.4-1.4M11.2 4.8l1.4-1.4" />
    </Svg>
  ),
  providers: (
    <Svg>
      <rect x="2" y="2" width="5" height="5" rx="1" />
      <rect x="9" y="2" width="5" height="5" rx="1" />
      <rect x="2" y="9" width="5" height="5" rx="1" />
      <rect x="9" y="9" width="5" height="5" rx="1" />
    </Svg>
  ),
  notifications: (
    <Svg>
      <path d="M3.5 11.5h9l-1.2-1.8V7a3.3 3.3 0 0 0-6.6 0v2.7Z" />
      <path d="M6.5 13a1.7 1.7 0 0 0 3 0" />
    </Svg>
  ),
  menuBar: (
    <Svg>
      <rect x="1.5" y="3" width="13" height="9.5" rx="1.6" />
      <path d="M3.5 8.5h3.2M9.2 6.5h2.8M9.2 9.5h2.8" />
    </Svg>
  ),
  dashboard: (
    <Svg>
      <rect x="1.8" y="2.5" width="12.4" height="11" rx="1.8" />
      <path d="M1.8 6h12.4M6 6v7.5" />
    </Svg>
  ),
  floatBar: (
    <Svg>
      <rect x="1.5" y="5.5" width="13" height="5" rx="2.5" />
      <path d="M4.5 8h2.5" />
    </Svg>
  ),
  menu: (
    <Svg>
      <path d="M1.5 8c1.6-3 4-4.5 6.5-4.5S13 5 14.5 8c-1.5 3-4 4.5-6.5 4.5S3.1 11 1.5 8Z" />
      <circle cx="8" cy="8" r="2" />
    </Svg>
  ),
  advanced: (
    <Svg>
      <path d="M2 4h8M2 8h5M2 12h10" />
      <circle cx="11.5" cy="4" r="1.4" />
      <circle cx="8.5" cy="8" r="1.4" />
      <circle cx="13" cy="12" r="1.4" />
    </Svg>
  ),
  about: (
    <Svg>
      <circle cx="8" cy="8" r="6.25" />
      <path d="M8 7v4" />
      <circle cx="8" cy="5" r="0.6" fill="currentColor" stroke="none" />
    </Svg>
  ),
};

// Tab order mirrors upstream PreferencesView (General, Providers, Display,
// Advanced, About). Per-provider credential management (API keys, cookies,
// token accounts) is handled inside the Providers tab.
const TAB_META: { id: SettingsTab; labelKey: LocaleKey }[] = [
  { id: "general", labelKey: "TabGeneral" },
  { id: "providers", labelKey: "TabProviders" },
  { id: "notifications", labelKey: "SectionNotifications" },
  // One page per component (item H): each writes only its own keys.
  { id: "floatBar", labelKey: "TabFloatBar" },
  { id: "dashboard", labelKey: "TabDashboard" },
  { id: "menuBar", labelKey: "TaskbarWidgetTab" },
  { id: "menu", labelKey: "TabDisplay" },
  { id: "advanced", labelKey: "TabAdvanced" },
  { id: "about", labelKey: "TabAbout" },
];

function isSettingsTab(value: string): value is SettingsTab {
  return TAB_META.some((t) => t.id === value);
}

// Window geometry for the detached Settings surface is owned exclusively by
// `shell/settings_window.rs` (create size + optional user drag-resize). The
// frontend must never call setSize/setPosition here: even "matching" values
// animate a Win32 resize, and tab-dependent layout used to amplify that into
// a whole-window stretch when entering Providers.

export default function Settings({ state, initialTab: propTab }: { state: BootstrapState; initialTab?: string }) {
  const { settings, saving, error, update } = useSettings(state.settings);
  const { t } = useLocale();
  const shellTarget = useSurfaceTarget("settings");
  const initialTab: SettingsTab =
    propTab && isSettingsTab(propTab)
      ? propTab
      : shellTarget?.kind === "settings" && isSettingsTab(shellTarget.tab)
        ? shellTarget.tab
        : "general";
  const [activeTab, setActiveTab] = useState<SettingsTab>(initialTab);
  // Tracks which side the next tab panel should slide in from: 1 when
  // moving to a tab further right in the bar, -1 when moving left. A ref
  // (rather than deriving from `activeTab` state) so the direction is
  // computed once, synchronously, before the state update that triggers
  // the remount — avoiding a stale value on the first animated frame.
  const activeTabRef = useRef<SettingsTab>(initialTab);
  const [tabSlideDirection, setTabSlideDirection] = useState<1 | -1>(1);
  const changeTab = useCallback((next: SettingsTab) => {
    if (activeTabRef.current === next) return;
    const from = TAB_META.findIndex((t) => t.id === activeTabRef.current);
    const to = TAB_META.findIndex((t) => t.id === next);
    setTabSlideDirection(to >= from ? 1 : -1);
    activeTabRef.current = next;
    setActiveTab(next);
  }, []);

  // Respond to prop-driven tab changes (detached window re-focus events).
  useEffect(() => {
    if (propTab && isSettingsTab(propTab)) {
      changeTab(propTab);
    }
  }, [propTab, changeTab]);

  useEffect(() => {
    if (shellTarget?.kind !== "settings" || !isSettingsTab(shellTarget.tab)) {
      return;
    }
    changeTab(shellTarget.tab);
  }, [shellTarget, changeTab]);

  const set = (patch: SettingsUpdate) => void update(patch);
  const handleTabClick = useCallback((tab: SettingsTab) => {
    changeTab(tab);
    // Only transition the main window if we're NOT in the detached settings window
    if (getCurrentWebviewWindow().label !== "settings") {
      void setSurfaceMode("settings", { kind: "settings", tab });
    }
  }, [changeTab]);

  return (
    <div className="settings">
      {/* custom title bar (decorations disabled for guaranteed dark theme) */}
      <div className="settings-titlebar" data-tauri-drag-region>
        <span className="settings-titlebar__title" data-tauri-drag-region>
          {t("SettingsWindowTitle")}
        </span>
        <div className="settings-titlebar__controls">
          <button
            className="settings-titlebar__control settings-titlebar__control--minimize"
            onClick={() => void getCurrentWindow().minimize()}
            aria-label={t("WindowMinimize")}
            title={t("WindowMinimize")}
          />
          <button
            className="settings-titlebar__control settings-titlebar__control--close"
            onClick={() => void closeSettingsWindow()}
            aria-label={t("WindowClose")}
            title={t("WindowClose")}
          >
            <svg aria-hidden viewBox="0 0 16 16" focusable="false">
              <path d="M4.5 4.5l7 7M11.5 4.5l-7 7" />
            </svg>
          </button>
        </div>
      </div>

      {/* tab bar */}
      <nav className="settings-tabs" role="tablist">
        {TAB_META.map((tab) => (
          <button
            key={tab.id}
            role="tab"
            aria-selected={activeTab === tab.id}
            className={`settings-tab ${activeTab === tab.id ? "settings-tab--active" : ""}`}
            onClick={() => handleTabClick(tab.id)}
          >
            <span className="settings-tab__icon">{TabIcons[tab.id]}</span>
            <span className="settings-tab__label">{t(tab.labelKey)}</span>
          </button>
        ))}
      </nav>

      {/* tab panels — body chrome is identical for every tab so switching
          into Providers cannot reflow the outer window (no display/overflow
          class toggles, no setSize, no transform animations). */}
      <div className="settings-body">
        {/* status toast — absolutely positioned within settings-body so a
            save notice never pushes the panel content down/up. */}
        {(saving || error) && (
          <div
            className={`settings-status ${error ? "settings-status--error" : ""}`}
          >
            {saving ? t("SettingsStatusSaving") : error}
          </div>
        )}
        <div
          key={activeTab}
          className={`settings-tab-panel${
            activeTab === "providers" ? " settings-tab-panel--providers" : ""
          }`}
          data-slide={tabSlideDirection > 0 ? "right" : "left"}
        >
          <div
            className={`settings-page${
              activeTab === "providers" ? " settings-page--providers" : ""
            }`}
            data-settings-page={activeTab}
            data-settings-tab-content={activeTab}
          >
            {activeTab === "general" && (
              <GeneralTab
                mode="general"
                settings={settings}
                set={set}
                saving={saving}
              />
            )}
            {activeTab === "providers" && (
              <ProvidersTab
                settings={settings}
                providers={state.providers}
                set={set}
                saving={saving}
              />
            )}
            {activeTab === "notifications" && (
              <GeneralTab
                mode="notifications"
                settings={settings}
                set={set}
                saving={saving}
              />
            )}
            {activeTab === "menuBar" && (
              <TaskbarTab settings={settings} set={set} saving={saving} />
            )}
            {activeTab === "dashboard" && (
              <DashboardTab settings={settings} set={set} saving={saving} />
            )}
            {activeTab === "floatBar" && (
              <FloatBarTab settings={settings} set={set} saving={saving} />
            )}
            {activeTab === "menu" && (
              <DisplayTab settings={settings} set={set} saving={saving} />
            )}
            {activeTab === "advanced" && (
              <AdvancedTab settings={settings} set={set} saving={saving} />
            )}
            {activeTab === "about" && (
              <AboutTab settings={settings} set={set} saving={saving} />
            )}
          </div>
        </div>
      </div>
    </div>
  );
}

// ── Tab props shared with extracted tab components ──────────────────

export interface TabProps {
  settings: BootstrapState["settings"];
  set: (p: SettingsUpdate) => void;
  saving: boolean;
}
