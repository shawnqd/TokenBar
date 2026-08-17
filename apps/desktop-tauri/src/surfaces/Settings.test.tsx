import { fireEvent, render, screen } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";
import type { BootstrapState, SettingsSnapshot } from "../types/bridge";
import { SETTINGS_NAV_ORDER } from "./settings/SettingsNav";

const settingsMocks = vi.hoisted(() => ({
  update: vi.fn(),
  settings: {} as SettingsSnapshot,
}));

vi.mock("../hooks/useLocale", () => ({
  useLocale: () => ({ t: (key: string) => key }),
}));

vi.mock("../hooks/useSettings", () => ({
  useSettings: () => ({
    settings: settingsMocks.settings,
    saving: false,
    error: null,
    update: settingsMocks.update,
  }),
}));

vi.mock("../hooks/useSurfaceMode", () => ({
  useSurfaceTarget: () => null,
}));

vi.mock("@tauri-apps/api/window", () => ({
  getCurrentWindow: () => ({ minimize: vi.fn() }),
}));

vi.mock("@tauri-apps/api/webviewWindow", () => ({
  getCurrentWebviewWindow: () => ({ label: "settings" }),
}));

vi.mock("@tauri-apps/api/core", () => ({
  invoke: vi.fn().mockResolvedValue([]),
}));

vi.mock("../lib/tauri", () => ({
  closeSettingsWindow: vi.fn(),
  setSurfaceMode: vi.fn(),
  playNotificationSound: vi.fn(),
  registerGlobalShortcut: vi.fn(),
  unregisterGlobalShortcut: vi.fn(),
  getAppInfo: vi.fn().mockResolvedValue({
    name: "TokenBar",
    version: "0.1.0",
    buildNumber: "dev",
    tagline: "",
  }),
  openExternalUrl: vi.fn(),
  getTaskbarFontFamilies: vi.fn().mockResolvedValue([]),
  getTaskbarPreviewLines: vi.fn().mockResolvedValue([]),
  getTaskbarWindowAvailability: vi.fn().mockResolvedValue({}),
  getProviderDetail: vi.fn().mockResolvedValue({
    id: "claude",
    dashboardUrl: null,
    statusPageUrl: null,
    buyCreditsUrl: null,
    lastError: null,
  }),
  refreshProviders: vi.fn(),
  openProviderDashboard: vi.fn(),
  openProviderStatusPage: vi.fn(),
  triggerProviderLogin: vi.fn(),
  revokeProviderCredentials: vi.fn(),
  getProviderChartData: vi.fn().mockResolvedValue(null),
  getSettingsSnapshot: vi.fn().mockResolvedValue({ enableAnimations: false }),
}));

vi.mock("../hooks/useProviders", () => ({
  useProviders: () => ({
    providers: [],
    isRefreshing: false,
    refresh: vi.fn(),
    lastRefresh: null,
    hasCachedData: false,
    hasLoadedCache: true,
  }),
}));

vi.mock("../hooks/useOutputSpeedSnapshot", () => ({
  useOutputSpeedSnapshot: () => null,
}));

import Settings from "./Settings";

const snapshot = {
  enabledProviders: [],
  refreshIntervalSecs: 300,
  refreshAllProvidersOnMenuOpen: false,
  startAtLogin: false,
  startMinimized: false,
  showNotifications: true,
  soundEnabled: true,
  soundVolume: 100,
  highUsageThreshold: 70,
  criticalUsageThreshold: 90,
  trayIconMode: "single",
  switcherShowsIcons: true,
  menuBarShowsHighestUsage: true,
  showAsUsed: false,
  showAllTokenAccountsInMenu: true,
  enableAnimations: true,
  resetTimeRelative: true,
  menuBarDisplayMode: "compact",
  trayScalePercent: 100,
  hidePersonalInfo: false,
  autoDownloadUpdates: false,
  installUpdatesOnQuit: false,
  globalShortcut: "",
  codexCustomSessionsDirs: [],
  updateChannel: "stable",
  uiLanguage: "english",
  theme: "dark",
  claudeAvoidKeychainPrompts: true,
  disableKeychainAccess: false,
  providerMetrics: {},
  floatBarEnabled: false,
  floatBarOpacity: 0.9,
  floatBarScale: 100,
  floatBarOrientation: "horizontal",
  floatBarStyle: "floating",
  floatBarClickThrough: false,
  floatBarProviderIds: [],
  floatBarDarkText: false,
  floatBarShowResetInline: false,
  floatBarResetWindows: ["primary"],
  taskbarWidgetEnabled: false,
  menuFontWeight: 300,
  menuFontFamily: "Microsoft YaHei UI",
  menuFontSize: 12,
  taskbarWidgetPosition: "notification",
  taskbarWidgetFontWeight: 400,
  taskbarWidgetContent: "usage",
  taskbarWidgetEntries: [],
  taskbarWidgetFontFamily: "Microsoft YaHei UI",
  taskbarWidgetFontSize: 12,
  taskbarWidgetWidth: 132,
  taskbarWidgetTextAlign: "left",
  floatBarShowAsUsed: true,
  floatBarResetTimeRelative: true,
  dashboardShowAsUsed: true,
  dashboardResetTimeRelative: true,
  dashboardProviderIds: [],
  dashboardQuotaWindows: [],
  taskbarShowAsUsed: true,
  taskbarResetTimeRelative: true,
  taskbarTooltipEntries: [],
} as unknown as SettingsSnapshot;

const state = {
  settings: snapshot,
  providers: [],
} as unknown as BootstrapState;

describe("Settings V5 shell", () => {
  beforeEach(() => {
    settingsMocks.settings = snapshot;
    settingsMocks.update.mockReset();
  });

  it("renders ten nav items in the locked order and never says Dashboard", () => {
    render(<Settings state={state} />);
    const nav = screen.getByRole("navigation");
    const items = nav.querySelectorAll(".settings-v5-nav__item");
    expect([...items].map((el) => el.textContent)).toEqual([
      "TabGeneral",
      "TabProviders",
      "TabTrayPanel",
      "TabFloatBar",
      "TabTaskbarStatus",
      "SectionNotifications",
      "TabAppearance",
      "TabPrivacy",
      "TabAdvanced",
      "TabAbout",
    ]);
    expect(SETTINGS_NAV_ORDER).toEqual([
      "general",
      "providers",
      "trayPanel",
      "floatBar",
      "taskbarStatus",
      "notifications",
      "appearance",
      "privacy",
      "advanced",
      "about",
    ]);
    expect(nav.textContent).not.toMatch(/Dashboard|menu bar|TabDashboard/i);
  });

  it("maps dashboard deep-link to the tray panel page", () => {
    render(<Settings state={state} initialTab="dashboard" />);
    expect(screen.getByText("显示密度")).toBeInTheDocument();
    expect(screen.getByText("实时预览")).toBeInTheDocument();
    expect(screen.queryByText("imported-tray-panel")).not.toBeInTheDocument();
    expect(
      screen.getByRole("button", { name: /TabTrayPanel/ }).getAttribute("aria-current"),
    ).toBe("page");
    expect(screen.queryByText("TabDashboard")).not.toBeInTheDocument();
  });

  it("maps menuBar deep-link to the mini status bar page", () => {
    render(<Settings state={state} initialTab="menuBar" />);
    expect(screen.getByText("显示小型状态栏")).toBeInTheDocument();
    expect(screen.getByText("图标渲染样式")).toBeInTheDocument();
    expect(
      screen.getByRole("button", { name: /TabTaskbarStatus/ }).getAttribute("aria-current"),
    ).toBe("page");
  });

  it("maps menu deep-link to appearance", () => {
    render(<Settings state={state} initialTab="menu" />);
    expect(screen.getByText("ThemeLabel")).toBeInTheDocument();
    expect(
      screen.getByRole("button", { name: /TabAppearance/ }).getAttribute("aria-current"),
    ).toBe("page");
  });

  it("accepts the new tab ids directly", () => {
    render(<Settings state={state} initialTab="privacy" />);
    expect(screen.getByText("HidePersonalInfo")).toBeInTheDocument();
  });

  it("switches pages from the left nav without a horizontal slide attribute", () => {
    render(<Settings state={state} />);
    fireEvent.click(screen.getByRole("button", { name: /TabAdvanced/ }));
    expect(screen.getByText("UpdatesTitle")).toBeInTheDocument();
    expect(document.querySelector("[data-slide]")).toBeNull();
    expect(screen.queryByText("GlobalShortcutFieldLabel")).not.toBeInTheDocument();
    expect(screen.queryByText("HidePersonalInfo")).not.toBeInTheDocument();
  });
});
