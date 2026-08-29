import { fireEvent, render, screen } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";

vi.mock("../../../hooks/useLocale", () => ({
  useLocale: () => ({ t: (key: string) => key }),
}));

// Mock Tauri invoke for get_available_languages
vi.mock("@tauri-apps/api/core", () => ({
  invoke: vi.fn().mockResolvedValue([
    { value: "english", display: "English" },
    { value: "chinese", display: "中文" },
    { value: "chinesetraditional", display: "繁體中文（臺灣）" },
    { value: "japanese", display: "日本語" },
    { value: "korean", display: "한국어" },
    { value: "spanish", display: "Español" },
  ]),
}));

vi.mock("../../../lib/tauri", () => ({
  invokeSurfaceAction: vi.fn().mockResolvedValue("ok"),
}));

import GeneralTab from "./GeneralTab";
import type { SettingsSnapshot } from "../../../types/bridge";

const settings: SettingsSnapshot = {
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
  taskbarWidgetEntries: [
    { providerId: "auto", window: "session" },
    { providerId: "auto", window: "weekly" },
  ],
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
};

describe("GeneralTab language picker", () => {
  it("renders 6 language options when Traditional Chinese is wired", () => {
    render(<GeneralTab settings={settings} set={vi.fn()} saving={false} />);

    const trigger = screen.getByRole("button", { name: "English" });
    expect(trigger).toBeInTheDocument();
    fireEvent.click(trigger);

    expect(screen.getAllByRole("option")).toHaveLength(6);
  });

  it("includes spanish as a selectable option", () => {
    render(<GeneralTab settings={settings} set={vi.fn()} saving={false} />);
    fireEvent.click(screen.getByRole("button", { name: "English" }));

    expect(
      screen.getByText("Español"),
    ).toBeInTheDocument();
  });

  it("includes korean as a selectable option", () => {
    render(<GeneralTab settings={settings} set={vi.fn()} saving={false} />);
    fireEvent.click(screen.getByRole("button", { name: "English" }));

    expect(
      screen.getByText("한국어"),
    ).toBeInTheDocument();
  });

  it("includes Traditional Chinese as a selectable option", () => {
    render(<GeneralTab settings={settings} set={vi.fn()} saving={false} />);
    fireEvent.click(screen.getByRole("button", { name: "English" }));

    expect(screen.getByText("繁體中文（臺灣）")).toBeInTheDocument();
  });

  it("hosts the tray shortcut and not notification controls", () => {
    render(<GeneralTab settings={settings} set={vi.fn()} saving={false} />);
    expect(screen.getByText("GlobalShortcutFieldLabel")).toBeInTheDocument();
    expect(screen.queryByText("ShowNotifications")).not.toBeInTheDocument();
  });
});
