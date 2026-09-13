import { fireEvent, render, screen } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";
import FloatBarPage from "./FloatBarPage";
import type { SettingsSnapshot } from "../../../types/bridge";

vi.mock("../../../hooks/useLocale", () => ({
  useLocale: () => ({
    t: (key: string) => key,
    language: "zh-CN",
  }),
}));

vi.mock("../../../lib/tauri", () => ({
  getTaskbarWindowAvailability: vi.fn().mockResolvedValue({}),
  getCachedProviders: vi.fn().mockResolvedValue([]),
  getSettingsSnapshot: vi.fn().mockResolvedValue({}),
  getProviderLocalUsageSummary: vi.fn().mockResolvedValue(null),
}));

vi.mock("@tauri-apps/api/event", () => ({
  listen: vi.fn().mockResolvedValue(() => {}),
}));

vi.mock("@tauri-apps/api/window", () => ({
  getCurrentWindow: vi.fn(() => ({
    startDragging: vi.fn().mockResolvedValue(undefined),
  })),
}));

function mockSettings(overrides: Partial<SettingsSnapshot> = {}): SettingsSnapshot {
  return {
    enabledProviders: ["claude", "codex"],
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
    menuBarShowsHighestUsage: false,
    showAsUsed: true,
    showAllTokenAccountsInMenu: false,
    enableAnimations: true,
    resetTimeRelative: true,
    menuBarDisplayMode: "detailed",
    hidePersonalInfo: false,
    updateChannel: "stable",
    autoDownloadUpdates: false,
    installUpdatesOnQuit: false,
    globalShortcut: "Ctrl+Shift+U",
    codexCustomSessionsDirs: [],
    uiLanguage: "english",
    theme: "dark",
    trayScalePercent: 100,
    claudeAvoidKeychainPrompts: false,
    disableKeychainAccess: false,
    providerMetrics: {},
    floatBarEnabled: true,
    floatBarOpacity: 80,
    floatBarScale: 100,
    floatBarOrientation: "horizontal",
    floatBarStyle: "floating",
    floatBarClickThrough: false,
    floatBarProviderIds: ["claude"],
    floatBarEntries: [{ providerId: "claude", window: "session" }],
    floatBarDarkText: false,
    floatBarShowCost: false,
    floatBarShowResetInline: true,
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
    ...overrides,
  };
}

describe("FloatBarPage", () => {
  beforeEach(() => {
    vi.clearAllMocks();
  });

  it("renders 4 structured sections and clean non-colloquial headers", () => {
    render(
      <FloatBarPage
        settings={mockSettings()}
        set={vi.fn()}
        saving={false}
      />,
    );

    expect(screen.getByText("窗口与交互")).toBeInTheDocument();
    expect(screen.getByText("显示内容")).toBeInTheDocument();
    expect(screen.getByText("外观与尺寸")).toBeInTheDocument();
    expect(screen.getByText("服务商与额度")).toBeInTheDocument();

    // Verify absence of outdated colloquial phrases
    expect(screen.queryByText("两态有名，不用下拉")).toBeNull();
    expect(screen.queryByText("打开后点不到条子，只能回设置关掉")).toBeNull();
  });

  it("toggles floatBarEnabled", () => {
    const set = vi.fn();
    render(
      <FloatBarPage
        settings={mockSettings({ floatBarEnabled: false })}
        set={set}
        saving={false}
      />,
    );

    const toggle = screen.getByRole("button", { name: "显示悬浮栏" });
    fireEvent.click(toggle);
    expect(set).toHaveBeenCalledWith({ floatBarEnabled: true });
  });

  it("changes orientation between horizontal and vertical", () => {
    const set = vi.fn();
    render(
      <FloatBarPage
        settings={mockSettings({ floatBarOrientation: "horizontal" })}
        set={set}
        saving={false}
      />,
    );

    const vertBtn = screen.getByRole("radio", { name: "纵向" });
    fireEvent.click(vertBtn);
    expect(set).toHaveBeenCalledWith({ floatBarOrientation: "vertical" });
  });

  it("changes window style between floating and taskbar/edge", () => {
    const set = vi.fn();
    render(
      <FloatBarPage
        settings={mockSettings({ floatBarStyle: "floating" })}
        set={set}
        saving={false}
      />,
    );

    const edgeBtn = screen.getByRole("radio", { name: "贴边" });
    fireEvent.click(edgeBtn);
    expect(set).toHaveBeenCalledWith({ floatBarStyle: "taskbar" });
  });

  it("toggles light background mode (floatBarDarkText)", () => {
    const set = vi.fn();
    render(
      <FloatBarPage
        settings={mockSettings({ floatBarDarkText: false })}
        set={set}
        saving={false}
      />,
    );

    const darkTextToggle = screen.getByRole("button", { name: "浅色桌面自适应" });
    fireEvent.click(darkTextToggle);
    expect(set).toHaveBeenCalledWith({ floatBarDarkText: true });
  });

  it("updates opacity and scale sliders", () => {
    const set = vi.fn();
    const { container } = render(
      <FloatBarPage
        settings={mockSettings({ floatBarOpacity: 80, floatBarScale: 100 })}
        set={set}
        saving={false}
      />,
    );

    const sliders = container.querySelectorAll<HTMLInputElement>("input[type='range']");
    expect(sliders.length).toBe(2);

    // Opacity
    fireEvent.change(sliders[0], { target: { value: "90" } });
    expect(set).toHaveBeenCalledWith({ floatBarOpacity: 90 });

    // Scale
    fireEvent.change(sliders[1], { target: { value: "125" } });
    expect(set).toHaveBeenCalledWith({ floatBarScale: 125 });
  });

  it("changes quota metric mode (used vs remain)", () => {
    const set = vi.fn();
    render(
      <FloatBarPage
        settings={mockSettings({ floatBarShowAsUsed: true, floatBarQuotaDisplay: "used" })}
        set={set}
        saving={false}
      />,
    );

    const remainBtn = screen.getByRole("radio", { name: "QuotaShowRemainingOption" });
    fireEvent.click(remainBtn);
    expect(set).toHaveBeenCalledWith({ floatBarQuotaDisplay: "remaining" });
  });

  it("keeps an effective-value helper when a surface overrides General", () => {
    const { container } = render(
      <FloatBarPage
        settings={mockSettings({
          floatBarQuotaDisplay: "used",
          floatBarResetDisplay: "absolute",
          floatBarShowResetInline: true,
        })}
        set={vi.fn()}
        saving={false}
      />,
    );

    expect(container.querySelectorAll('[title="QuotaOverrideHelper"]')).toHaveLength(2);
  });

  it("toggles inline reset and shows reset chips", () => {
    const set = vi.fn();
    const { container } = render(
      <FloatBarPage
        settings={mockSettings({
          floatBarShowResetInline: true,
          floatBarResetWindows: ["primary"],
        })}
        set={set}
        saving={false}
      />,
    );

    const chips = Array.from(container.querySelectorAll<HTMLButtonElement>(".s5-chips .s5-chip"));
    const sessionChip = chips.find((c) => c.textContent?.includes("TaskbarWindowSession"));
    expect(sessionChip).toBeDefined();
    fireEvent.click(sessionChip!);
    expect(set).toHaveBeenCalledWith({
      floatBarResetWindows: ["primary", "session"],
    });
  });

  it("resets section values via Section Reset button", () => {
    const set = vi.fn();
    render(
      <FloatBarPage
        settings={mockSettings({
          floatBarOpacity: 50,
          floatBarScale: 150,
        })}
        set={set}
        saving={false}
      />,
    );

    const resetButtons = screen.getAllByRole("button", { name: "ComponentResetDefaults" });
    // Section 4: 外观与尺寸 reset
    fireEvent.click(resetButtons[3]);
    expect(set).toHaveBeenCalledWith({
      floatBarOpacity: 80,
      floatBarScale: 100,
    });
  });
});
