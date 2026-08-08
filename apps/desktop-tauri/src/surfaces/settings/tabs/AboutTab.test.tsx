import { fireEvent, render, screen, waitFor } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";

const tauriMocks = vi.hoisted(() => ({
  getAppInfo: vi.fn(),
  openExternalUrl: vi.fn(),
}));

vi.mock("../../../lib/tauri", () => tauriMocks);
vi.mock("../../../hooks/useLocale", () => ({
  useLocale: () => ({ t: (key: string) => key }),
}));
import AboutTab from "./AboutTab";
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
  hidePersonalInfo: false,
  autoDownloadUpdates: false,
  installUpdatesOnQuit: false,
  globalShortcut: "",
  codexCustomSessionsDirs: [],
  updateChannel: "stable",
  uiLanguage: "english",
  theme: "dark",
  windowScalePercent: 125,
  trayScalePercent: 100,
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
  taskbarShowAsUsed: true,
  taskbarResetTimeRelative: true,
    taskbarTooltipEntries: [],
};

describe("AboutTab", () => {
  beforeEach(() => {
    vi.clearAllMocks();
    tauriMocks.getAppInfo.mockResolvedValue({
      name: "CodexBar",
      version: "0.30.3",
      buildNumber: "dev",
      updateChannel: "stable",
      tagline: "Keep agent limits in view.",
    });
    tauriMocks.openExternalUrl.mockResolvedValue(undefined);
  });

  it("opens about links through the Tauri URL bridge", async () => {
    render(<AboutTab settings={settings} set={vi.fn()} saving={false} />);

    fireEvent.click(await screen.findByRole("button", { name: "AboutLinkGithub" }));
    fireEvent.click(screen.getByRole("button", { name: "AboutLinkWebsite" }));
    fireEvent.click(screen.getByRole("button", { name: "AboutLinkOriginalProject" }));

    expect(tauriMocks.openExternalUrl).toHaveBeenNthCalledWith(
      1,
      "https://github.com/Finesssee/Win-CodexBar",
    );
    expect(tauriMocks.openExternalUrl).toHaveBeenNthCalledWith(
      2,
      "https://codexbar.app",
    );
    expect(tauriMocks.openExternalUrl).toHaveBeenNthCalledWith(
      3,
      "https://github.com/steipete/CodexBar",
    );
  });

  it("shows a link error if the OS browser launch fails", async () => {
    tauriMocks.openExternalUrl.mockRejectedValue("no browser");

    render(<AboutTab settings={settings} set={vi.fn()} saving={false} />);

    fireEvent.click(await screen.findByRole("button", { name: "AboutLinkWebsite" }));

    await waitFor(() => {
      expect(screen.getByText("StateError: no browser")).toBeInTheDocument();
    });
  });
});
