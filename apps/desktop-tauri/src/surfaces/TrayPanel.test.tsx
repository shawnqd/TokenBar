import { act, cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

const tauriMocks = vi.hoisted(() => ({
  getCachedProviders: vi.fn(),
  getOutputSpeedSnapshot: vi.fn().mockResolvedValue({
    codex: {
      providerId: "codex",
      status: "recent",
      tokensPerSecond: 24.5,
      outputTokens: 120,
      updatedAtMs: 1,
      approximate: true,
    },
    claude: {
      providerId: "claude",
      status: "recent",
      tokensPerSecond: 18.2,
      outputTokens: 90,
      updatedAtMs: 1,
      approximate: true,
    },
  }),
  refreshProviders: vi.fn(),
  refreshProvidersIfStale: vi.fn(),
  getSettingsSnapshot: vi.fn(),
  updateSettings: vi.fn(),
  getUpdateState: vi.fn(),
  checkForUpdates: vi.fn(),
  downloadUpdate: vi.fn(),
  applyUpdate: vi.fn(),
  dismissUpdate: vi.fn(),
  openReleasePage: vi.fn(),
  setSurfaceMode: vi.fn(),
  dismissTrayPanel: vi.fn(),
  beginFlyoutGesture: vi.fn().mockResolvedValue(undefined),
  endFlyoutGesture: vi.fn().mockResolvedValue(undefined),
  openSettingsWindow: vi.fn(),
  quitApp: vi.fn(),
  reanchorTrayPanel: vi.fn(),
  revealTrayPanelWindow: vi.fn(),
  openProviderDashboard: vi.fn(),
  openProviderStatusPage: vi.fn(),
  getProviderChartData: vi.fn(),
  getCurrentSurfaceState: vi.fn(),
  getLocaleStrings: vi.fn(),
  setUiLanguage: vi.fn(),
}));

const eventMocks = vi.hoisted(() => ({
  listen: vi.fn(),
  listeners: new Map<string, Array<(event: { payload: unknown }) => void>>(),
}));

const windowMocks = vi.hoisted(() => ({
  getCurrentWindow: vi.fn(() => ({
    setSize: vi.fn().mockResolvedValue(undefined),
    close: vi.fn().mockResolvedValue(undefined),
    scaleFactor: vi.fn().mockResolvedValue(1),
    onResized: vi.fn().mockResolvedValue(() => {}),
    innerSize: vi.fn().mockResolvedValue({ width: 328, height: 200 }),
  })),
  LogicalSize: vi.fn((width: number, height: number) => ({ width, height })),
  PhysicalSize: vi.fn((width: number, height: number) => ({ width, height })),
}));

vi.mock("../lib/tauri", () => tauriMocks);
vi.mock("@tauri-apps/api/event", () => eventMocks);
vi.mock("@tauri-apps/api/window", () => windowMocks);

import TrayPanel from "./TrayPanel";
import { LocaleProvider } from "../i18n/LocaleProvider";
import { TEST_PROVIDER_CATALOG } from "../test/providerCatalog";
import { buildBundle } from "../test/localeHarness";
import type {
  BootstrapState,
  ProviderCatalogEntry,
  ProviderUsageSnapshot,
  SettingsSnapshot,
} from "../types/bridge";

function rateWindow(used: number) {
  return {
    usedPercent: used,
    remainingPercent: 100 - used,
kind: null,
        windowMinutes: null,
    resetsAt: null,
    resetDescription: null,
    isExhausted: false,
    reservePercent: null,
    reserveDescription: null,
  };
}

function provider(id: string, displayName: string, used = 20): ProviderUsageSnapshot {
  return {
    providerId: id,
    displayName,
    primary: rateWindow(used),
    primaryLabel: "Monthly",
    secondary: null,
    modelSpecific: null,
    tertiary: null,
    extraRateWindows: [],
    cost: null,
    planName: null,
    accountEmail: null,
    sourceLabel: "auto",
    updatedAt: "2026-05-24T00:00:00Z",
    error: null,
    pace: null,
    accountOrganization: null,
    trayStatusLabel: null,
    fetchDurationMs: null,
  };
}

function settings(overrides: Partial<SettingsSnapshot> = {}): SettingsSnapshot {
  return {
    enabledProviders: ["codex", "claude"],
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
    outputSpeedEnabled: true,
    hidePersonalInfo: false,
    updateChannel: "stable",
    autoDownloadUpdates: false,
    installUpdatesOnQuit: false,
    globalShortcut: "Ctrl+Shift+U",
    codexCustomSessionsDirs: [],
    uiLanguage: "english",
    theme: "dark",
    windowScalePercent: 125,
    trayScalePercent: 100,
    claudeAvoidKeychainPrompts: false,
    disableKeychainAccess: false,
    providerMetrics: {},
    floatBarEnabled: false,
    floatBarOpacity: 80,
    floatBarScale: 100,
    floatBarOrientation: "horizontal",
    floatBarStyle: "floating",
    floatBarClickThrough: false,
    floatBarProviderIds: [],
    floatBarDarkText: false,
    floatBarShowResetInline: false,
    floatBarResetWindows: ["primary"],
    taskbarWidgetEnabled: false,
    taskbarWidgetPosition: "notification",
    taskbarWidgetFontWeight: 400,
    menuFontWeight: 300,
    menuFontFamily: "Microsoft YaHei UI",
    menuFontSize: 12,
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
    taskbarContextMenuActions: ["open_panel", "refresh", "settings", "quit"],
    taskbarTooltipEntries: [],
    ...overrides,
  };
}

function bootstrap(
  settingsOverrides: Partial<SettingsSnapshot> = {},
  catalog: ProviderCatalogEntry[] = [],
): BootstrapState {
  return {
    contractVersion: "v1",
    providers: catalog,
    settings: settings(settingsOverrides),
  };
}

function renderTrayPanel(
  providers: ProviderUsageSnapshot[],
  settingsOverrides: Partial<SettingsSnapshot> = {},
  catalog: ProviderCatalogEntry[] = [],
) {
  const effectiveSettings = settings({
    enabledProviders: providers.map((provider) => provider.providerId),
    ...settingsOverrides,
  });
  tauriMocks.getCachedProviders.mockResolvedValue(providers);
  tauriMocks.getSettingsSnapshot.mockResolvedValue(effectiveSettings);
  return render(
    <LocaleProvider>
      <TrayPanel state={{
        contractVersion: "v1",
        providers: catalog,
        settings: effectiveSettings,
      }} />
    </LocaleProvider>,
  );
}

function emitEvent(event: string, payload: unknown) {
  for (const listener of eventMocks.listeners.get(event) ?? []) {
    listener({ payload });
  }
}

describe("TrayPanel provider grid", () => {
  beforeEach(() => {
    vi.clearAllMocks();
    eventMocks.listeners.clear();
    tauriMocks.refreshProviders.mockResolvedValue(undefined);
    tauriMocks.refreshProvidersIfStale.mockResolvedValue(undefined);
    tauriMocks.dismissTrayPanel.mockResolvedValue(undefined);
    tauriMocks.reanchorTrayPanel.mockResolvedValue(undefined);
    tauriMocks.getCurrentSurfaceState.mockResolvedValue({
      mode: "trayPanel",
      target: { kind: "summary" },
    });
    tauriMocks.getSettingsSnapshot.mockResolvedValue(settings());
    tauriMocks.getOutputSpeedSnapshot.mockResolvedValue({
      codex: {
        providerId: "codex",
        status: "recent",
        tokensPerSecond: 24.5,
        outputTokens: 120,
        updatedAtMs: 1,
        approximate: true,
      },
      claude: {
        providerId: "claude",
        status: "recent",
        tokensPerSecond: 18.2,
        outputTokens: 90,
        updatedAtMs: 1,
        approximate: true,
      },
    });
    tauriMocks.updateSettings.mockResolvedValue(settings());
    tauriMocks.getUpdateState.mockResolvedValue({
      status: "idle",
      version: null,
      error: null,
      progress: null,
      releaseUrl: null,
      canDownload: false,
      canApply: false,
      lastCheckedAt: null,
    });
    tauriMocks.getProviderChartData.mockResolvedValue({
      providerId: "codex",
      costHistory: [],
      creditsHistory: [],
      usageBreakdown: [],
      localUsage: null,
    });
    tauriMocks.getLocaleStrings.mockResolvedValue(
      buildBundle({
        ActionRefresh: "Refresh",
        TrayOpenDashboard: "Open Dashboard",
        MenuQuit: "Quit",
        MenuSettings: "Settings...",
        PanelAllProviders: "All providers",
        PanelAllProvidersShort: "All",
        PanelLeftSuffix: "left",
        PanelShowAllProviders: "Show all providers",
        PanelShowFewerProviders: "Show fewer providers",
        PanelUsedSuffix: "used",
        ResetsInHoursMinutes: "Resets in {}h {}m",
        ResetsInDaysHours: "Resets in {}d {}h",
        TodayAt: "Today at {}",
        TomorrowAt: "Tomorrow at {}",
      }),
    );
    eventMocks.listen.mockImplementation(
      (event: string, handler: (event: { payload: unknown }) => void) => {
        const listeners = eventMocks.listeners.get(event) ?? [];
        listeners.push(handler);
        eventMocks.listeners.set(event, listeners);
        return Promise.resolve(() => {});
      },
    );
  });
  afterEach(() => {
    // Unmount while the Tauri bridge mocks still retain their Promise-returning
    // implementations. Restoring first can race pending MenuCard effects in the
    // full suite and turn getProviderChartData() into undefined during cleanup.
    cleanup();
    vi.restoreAllMocks();
  });

  it("reveals regardless of the shared surface-mode snapshot (TrayPanel now runs in its own dedicated window)", async () => {
    // TrayPanel is now hosted exclusively in the dedicated `flyout` OS
    // window (see App.tsx's isFlyoutWindow() routing), so it must not depend
    // on `main`'s surface-mode machine to know it's "open" — that machine
    // can never report "trayPanel" anymore (main only holds
    // Hidden/PopOut/Settings post-refactor). Overriding the snapshot mock to
    // something else confirms the native-size restore + reveal gate
    // is no longer wired to useSurfaceMode() at all.
    tauriMocks.getCurrentSurfaceState.mockResolvedValue({
      mode: "popOut",
      target: { kind: "dashboard" },
    });

    const { container } = renderTrayPanel([provider("claude", "Claude", 35)]);

    await waitFor(() => {
      expect(container.querySelector(".tray-panel-reveal--ready")).not.toBeNull();
    });
  });

  it("reveals before the first provider refresh completes", async () => {
    // The native flyout starts hidden. A slow cache/network response must not
    // leave it invisible: data is content, not the window reveal handshake.
    tauriMocks.getCachedProviders.mockReturnValue(new Promise(() => {}));

    const { container } = renderTrayPanel([]);

    await waitFor(() => {
      expect(container.querySelector(".tray-panel-reveal--ready")).not.toBeNull();
    });
  });

  it("renders a hero quota row plus a one-line summary in minimal mode", async () => {
    // Minimal density always renders inside the normal .menu-stack card
    // list (MenuCard's densityMode="minimal"), not a separate flat list —
    // only the secondary metric row is dropped, folded into the summary
    // line instead alongside speed/pace.
    const { container } = renderTrayPanel(
      [provider("claude", "Claude", 35)],
      { menuBarDisplayMode: "minimal" },
    );

    await waitFor(() => {
      expect(container.querySelector(".menu-stack")).not.toBeNull();
    });
    expect(container.querySelector(".menu-card__name")?.textContent).toBe("Claude");
    expect(container.querySelector(".provider-quota__hero-pct")?.textContent).toBe("35%");
    expect(container.querySelector(".menu-card__minimal-line")).not.toBeNull();
    expect(container.querySelector(".tray-minimal-summary")).toBeNull();
  });

  it("shows the balance amount instead of a meaningless 0% for balance-only providers in minimal mode", async () => {
    const deepseek: ProviderUsageSnapshot = {
      ...provider("deepseek", "DeepSeek", 0),
      primary: {
        ...rateWindow(0),
        resetDescription: "¥38.81 (Paid: ¥38.81 / Granted: ¥0.00)",
      },
    };
    const { container } = renderTrayPanel([deepseek], { menuBarDisplayMode: "minimal" });

    await waitFor(() => {
      expect(container.querySelector(".provider-balance__amount")).not.toBeNull();
    });
    expect(container.querySelector(".provider-balance__amount")?.textContent).toBe("¥38.81");
    expect(container.querySelector(".provider-quota__hero-pct")).toBeNull();
  });

  it("dismisses the tray panel on unmodified Escape", async () => {
    const { container } = renderTrayPanel([provider("claude", "Claude", 35)]);

    await waitFor(() => {
      expect(container.querySelector(".tray-panel-reveal--ready")).not.toBeNull();
    });

    fireEvent.keyDown(window, { key: "Escape" });

    await waitFor(() => {
      expect(tauriMocks.dismissTrayPanel).toHaveBeenCalledTimes(1);
    });
  });

  it("does not dismiss the tray panel on modified Escape", async () => {
    const { container } = renderTrayPanel([provider("claude", "Claude", 35)]);

    await waitFor(() => {
      expect(container.querySelector(".tray-panel-reveal--ready")).not.toBeNull();
    });

    fireEvent.keyDown(window, { key: "Escape", ctrlKey: true });
    fireEvent.keyDown(window, { key: "Escape", shiftKey: true });
    fireEvent.keyDown(window, { key: "Escape", altKey: true });
    fireEvent.keyDown(window, { key: "Escape", metaKey: true });

    expect(tauriMocks.dismissTrayPanel).not.toHaveBeenCalled();
  });

  it("keeps the existing Ctrl+R tray shortcut", async () => {
    const { container } = renderTrayPanel([provider("claude", "Claude", 35)]);

    await waitFor(() => {
      expect(container.querySelector(".tray-panel-reveal--ready")).not.toBeNull();
    });
    tauriMocks.refreshProviders.mockClear();

    fireEvent.keyDown(window, { key: "r", ctrlKey: true });

    await waitFor(() => {
      expect(tauriMocks.refreshProviders).toHaveBeenCalledTimes(1);
    });
  });

  it("localizes static tray panel labels in Japanese", async () => {
    tauriMocks.getLocaleStrings.mockResolvedValue(
      buildBundle(
        {
          ActionRefresh: "更新",
          MenuQuit: "終了",
          MenuSettings: "設定...",
          PanelAllProviders: "すべてのプロバイダー",
          PanelAllProvidersShort: "すべて",
          PanelSevenDayUsage: "過去7日間",
          PanelThirtyDayUsage: "過去30日間",
          PanelApiEquivalentValue: "API換算額",
          PanelTokenUnit: "Token",
          PanelLocalEstimateShort: "ローカルログによる参考値",
          PanelTopModelPrefix: "トップモデル",
          UpdatedDaysAgo: "{}日前",
        },
        "japanese",
      ),
    );
    tauriMocks.getProviderChartData.mockResolvedValue({
      providerId: "codex",
      costHistory: [{ date: "2026-05-24", value: 1.23 }],
      creditsHistory: [],
      usageBreakdown: [],
      localUsage: {
        todayCost: null,
        todayTokens: null,
        sevenDayCost: null,
        sevenDayTokens: 1200,
        thirtyDayCost: 1.23,
        thirtyDayTokens: 584_000,
        todayTopModel: null,
        sevenDayTopModel: "gpt-5.5",
        thirtyDayTopModel: "gpt-5.5",
        estimateNote: "Estimated from local logs",
      },
    });

    const { container } = renderTrayPanel([provider("codex", "Codex", 35)]);

    await waitFor(() => {
      expect(
        container.querySelector('.provider-grid__item[aria-label="すべてのプロバイダー"]'),
      ).not.toBeNull();
    });
    expect(container.querySelector(".provider-grid__item")?.textContent).toContain("すべて");
    expect(screen.getByText("更新")).toBeInTheDocument();
    expect(screen.getByText("設定...")).toBeInTheDocument();
    expect(screen.getByText("終了")).toBeInTheDocument();
    expect(await screen.findByText("過去7日間")).toBeInTheDocument();
    expect(screen.queryByText("過去30日間")).not.toBeInTheDocument();
    expect(container.querySelector(".menu-card__subtitle")?.textContent).toContain("日前");
    expect(screen.getByText("1,200")).toBeInTheDocument();
    // The tray density card prints the count without a "Token" unit word, as
    // the reference card does (design/floatbar-reference.html shows
    // "921,605  ≈92.2万"). The legacy Settings/PopOut card still labels it.
    expect(screen.queryByText("Token")).not.toBeInTheDocument();
    expect(screen.getByText("トップモデル: gpt-5.5")).toBeInTheDocument();
    // The local-estimate note line was removed from the token usage block.
    expect(screen.queryByText("ローカルログによる参考値")).not.toBeInTheDocument();
  });

  it("localizes the expanded dense grid collapse label in Japanese", async () => {
    tauriMocks.getLocaleStrings.mockResolvedValue(
      buildBundle(
        {
          PanelAllProviders: "すべてのプロバイダー",
          PanelAllProvidersShort: "すべて",
          PanelShowAllProviders: "すべてのプロバイダーを表示",
          PanelShowFewerProviders: "表示を減らす",
        },
        "japanese",
      ),
    );
    const providers = TEST_PROVIDER_CATALOG.map(([id, displayName], index) =>
      provider(id, displayName, (index * 7) % 100),
    );

    const { container } = renderTrayPanel(providers);

    await waitFor(() => {
      expect(container.querySelector(".provider-grid--compact")).not.toBeNull();
    });

    fireEvent.click(
      container.querySelector<HTMLButtonElement>(
        '.provider-grid__item--more[aria-label="すべてのプロバイダーを表示"]',
      )!,
    );

    expect(await screen.findByText("表示を減らす")).toBeInTheDocument();
  });

  it.each([
    [1, true],
    [2, true],
    [5, true],
    [6, false],
    [12, false],
  ])("uses expected density for %i providers plus overview", async (providerCount, shouldBeSparse) => {
      const providers = [
        provider("codex", "Codex"),
        provider("claude", "Claude"),
        provider("copilot", "GitHub Copilot"),
        provider("cursor", "Cursor"),
        provider("gemini", "Gemini"),
        provider("kiro", "Kiro"),
        provider("zai", "z.ai"),
        provider("minimax", "MiniMax"),
        provider("vertexai", "Vertex AI"),
        provider("augment", "Augment"),
        provider("opencode", "OpenCode"),
        provider("kimi", "Kimi"),
      ].slice(0, providerCount);

      const { container } = renderTrayPanel(providers);

      await waitFor(() => {
        expect(container.querySelector(".provider-grid")).not.toBeNull();
      });

      const grid = container.querySelector(".provider-grid");
      expect(grid?.classList.contains("provider-grid--sparse")).toBe(
        shouldBeSparse,
      );
    },
  );

  it("only requests chart data for providers that can render charts", async () => {
    renderTrayPanel([
      provider("codex", "Codex"),
      provider("claude", "Claude"),
      provider("copilot", "GitHub Copilot"),
      provider("cursor", "Cursor"),
      provider("deepseek", "DeepSeek"),
    ]);

    await waitFor(() => {
      expect(tauriMocks.getProviderChartData).toHaveBeenCalledTimes(2);
    });

    expect(tauriMocks.getProviderChartData).toHaveBeenCalledWith("codex", undefined);
    expect(tauriMocks.getProviderChartData).toHaveBeenCalledWith("claude", undefined);
  });

  it("renders providers in settings catalog order instead of fetch completion order", async () => {
    const catalog: ProviderCatalogEntry[] = [
      { id: "codex", displayName: "Codex", cookieDomain: null },
      { id: "claude", displayName: "Claude", cookieDomain: null },
      { id: "cursor", displayName: "Cursor", cookieDomain: null },
      { id: "factory", displayName: "Factory", cookieDomain: null },
      { id: "gemini", displayName: "Gemini", cookieDomain: null },
    ];
    const providers = [
      provider("gemini", "Gemini", 10),
      provider("cursor", "Cursor", 20),
      { ...provider("codex", "Codex", 80), error: "Authentication required" },
      provider("factory", "Factory", 30),
      { ...provider("claude", "Claude", 40), error: "Claude sign-in missing" },
    ];

    const { container } = renderTrayPanel(
      providers,
      { enabledProviders: catalog.map((entry) => entry.id) },
      catalog,
    );

    await waitFor(() => {
      expect(container.querySelectorAll(".provider-grid__item")).toHaveLength(6);
    });

    const labels = Array.from(container.querySelectorAll(".provider-grid__item"))
      .map((node) => node.getAttribute("aria-label"));
    expect(labels).toEqual([
      "All providers",
      "Codex",
      "Claude",
      "Cursor",
      "Factory",
      "Gemini",
    ]);
    expect(
      Array.from(container.querySelectorAll(".menu-card__name")).map(
        (node) => node.textContent,
      ),
    ).toEqual(["Codex", "Claude", "Cursor", "Factory", "Gemini"]);
  });

  it("keeps vertically stacked cards while native sizing owns the panel width", async () => {
    const providers = [
      provider("codex", "Codex"),
      provider("claude", "Claude"),
      provider("antigravity", "Antigravity"),
      provider("copilot", "GitHub Copilot"),
    ];

    const { container } = renderTrayPanel(providers, {
      enabledProviders: providers.map((snapshot) => snapshot.providerId),
    });

    await waitFor(() => {
      expect(container.querySelector(".tray-panel-reveal--native-size")).not.toBeNull();
    });

    // The frontend must not resize the native flyout. Its default/remembered
    // dimensions come from the Rust window builder so a user drag persists.
    expect(windowMocks.PhysicalSize).not.toHaveBeenCalled();
    expect(
      Array.from(container.querySelectorAll(".menu-stack__item")).map((item) => item.id),
    ).toEqual([
      "card-codex",
      "card-claude",
      "card-antigravity",
      "card-copilot",
    ]);
    expect(container.querySelector(".menu-stack__column")).toBeNull();
    expect(container.querySelectorAll(".menu-stack__sep")).toHaveLength(3);
  });

  it("collapses and expands the full provider catalog in the dense tray grid", async () => {
    const providers = TEST_PROVIDER_CATALOG.map(([id, displayName], index) =>
      provider(id, displayName, (index * 7) % 100),
    );

    const { container } = renderTrayPanel(providers);

    await waitFor(() => {
      expect(container.querySelectorAll(".provider-grid__item")).toHaveLength(
        20,
      );
    });

    const grid = container.querySelector(".provider-grid");
    expect(grid?.classList.contains("provider-grid--sparse")).toBe(false);
    expect(grid?.classList.contains("provider-grid--compact")).toBe(true);
    expect(grid?.getAttribute("data-expanded")).toBe("false");
    expect(grid?.getAttribute("data-provider-count")).toBe(
      String(providers.length + 1),
    );
    expect(container.querySelectorAll(".menu-stack__item")).toHaveLength(4);

    const expand = container.querySelector<HTMLButtonElement>(
      '.provider-grid__item--more[aria-label="Show all providers"]',
    );
    expect(expand).not.toBeNull();
    expect(expand?.textContent).toContain(`+${providers.length - 18}`);

    fireEvent.click(expand!);

    await waitFor(() => {
      expect(container.querySelectorAll(".provider-grid__item")).toHaveLength(
        providers.length + 2,
      );
    });
    expect(grid?.getAttribute("data-expanded")).toBe("true");
    expect(container.querySelectorAll(".menu-stack__item")).toHaveLength(
      providers.length,
    );
    for (const [id, displayName] of TEST_PROVIDER_CATALOG) {
      expect(
        container.querySelector(`.provider-grid__item[aria-label="${displayName}"]`),
        id,
      ).not.toBeNull();
    }
  });

  it("uses compact provider labels for huge catalogs without losing full accessible labels", async () => {
    const providers = TEST_PROVIDER_CATALOG.slice(0, 36).map(
      ([id, displayName], index) => provider(id, displayName, (index * 7) % 100),
    );

    const { container } = renderTrayPanel(providers);

    await waitFor(() => {
      expect(container.querySelector(".provider-grid--compact")).not.toBeNull();
    });

    const expand = container.querySelector<HTMLButtonElement>(
      '.provider-grid__item--more[aria-label="Show all providers"]',
    );
    expect(expand).not.toBeNull();

    fireEvent.click(expand!);

    await waitFor(() => {
      expect(
        container.querySelector('.provider-grid__item[aria-label="Copilot"]'),
      ).not.toBeNull();
    });

    const copilot = container.querySelector(
      '.provider-grid__item[aria-label="Copilot"]',
    );
    expect(copilot).not.toBeNull();
    expect(copilot?.getAttribute("aria-label")).toBe("Copilot");
    expect(copilot?.querySelector(".provider-grid__label")?.textContent).toBe(
      "Copi",
    );
  });

  it("provider grid indicator follows the dashboard show-as-used setting", async () => {
    const { container, rerender } = renderTrayPanel(
      [provider("claude", "Claude", 35)],
      { dashboardShowAsUsed: true },
    );

    await waitFor(() => {
      const track = container.querySelector<HTMLElement>(
        ".provider-grid__weekly-track",
      );
      expect(track?.style.getPropertyValue("--weekly-pct")).toBe("35%");
    });

    tauriMocks.getCachedProviders.mockResolvedValue([
      provider("claude", "Claude", 35),
    ]);
    tauriMocks.getSettingsSnapshot.mockResolvedValue(
      settings({ dashboardShowAsUsed: false }),
    );
    rerender(
      <LocaleProvider>
        <TrayPanel state={bootstrap({ dashboardShowAsUsed: false })} />
      </LocaleProvider>,
    );

    await waitFor(() => {
      const track = container.querySelector<HTMLElement>(
        ".provider-grid__weekly-track",
      );
      expect(track?.style.getPropertyValue("--weekly-pct")).toBe("65%");
    });
  });

  /**
   * The per-component split exists so one surface's preference cannot move
   * another's. The dashboard must ignore the floating bar's and the taskbar
   * strip's choice even when they disagree with its own.
   */
  it("ignores the floating bar and taskbar show-as-used settings", async () => {
    const { container } = renderTrayPanel([provider("claude", "Claude", 35)], {
      dashboardShowAsUsed: true,
      floatBarShowAsUsed: false,
      taskbarShowAsUsed: false,
    taskbarContextMenuActions: ["open_panel", "refresh", "settings", "quit"],
    taskbarTooltipEntries: [],
    });

    await waitFor(() => {
      const track = container.querySelector<HTMLElement>(
        ".provider-grid__weekly-track",
      );
      expect(track?.style.getPropertyValue("--weekly-pct")).toBe("35%");
    });
  });

  /**
   * A provider with no percentage quota (sub2api-style "Subscription active"
   * rows, or a balance provider's synthetic carrier window) must not get a
   * percentage track in the grid — a 0%/empty bar reads as a real measurement.
   */
  it("draws no percentage track for a provider with no percentage quota", async () => {
    const informational = provider("sub2api", "Sub2API", 0);
    informational.primary = {
      ...informational.primary,
      isInformational: true,
      resetDescription: "Subscription active",
    };
    const quota = provider("claude", "Claude", 35);

    const { container } = renderTrayPanel([quota, informational]);

    await waitFor(() => {
      expect(
        container.querySelectorAll(".provider-grid__weekly-track").length,
      ).toBe(1);
    });

    const informationalItem = container.querySelector<HTMLElement>(
      '.provider-grid__item[aria-label="Sub2API"]',
    );
    expect(informationalItem).not.toBeNull();
    expect(
      informationalItem?.querySelector(".provider-grid__weekly-track"),
    ).toBeNull();
  });

  it("hides provider grid icons when the display setting is disabled", async () => {
    const { container } = renderTrayPanel(
      [provider("codex", "Codex"), provider("claude", "Claude")],
      { switcherShowsIcons: false },
    );

    await waitFor(() => {
      expect(container.querySelector(".provider-grid")).not.toBeNull();
    });

    const grid = container.querySelector(".provider-grid");
    expect(grid?.getAttribute("data-show-icons")).toBe("false");
    expect(grid?.classList.contains("provider-grid--no-icons")).toBe(true);
    expect(container.querySelector(".provider-icon")).toBeNull();
    expect(container.querySelector(".provider-grid__icon-overview")).toBeNull();
  });

  it("keeps tray content unscaled while native resizing owns the window", async () => {
    const { container } = renderTrayPanel(
      [provider("claude", "Claude", 35)],
      { trayScalePercent: 150 },
    );

    await waitFor(() => {
      expect(container.querySelector(".menu-surface--tray")).not.toBeNull();
    });

    expect(container.querySelector(".menu-surface__footer-zoom")).toBeNull();
    const surface = container.querySelector<HTMLElement>(".menu-surface--tray")!;
    expect(surface.style.getPropertyValue("zoom")).toBe("");
  });

  it("opens the full dashboard from the tray and then dismisses the flyout", async () => {
    renderTrayPanel([provider("claude", "Claude", 35)]);

    fireEvent.click(await screen.findByText("Open Dashboard"));

    await waitFor(() => {
      expect(tauriMocks.setSurfaceMode).toHaveBeenCalledWith("popOut", {
        kind: "dashboard",
      });
      expect(tauriMocks.dismissTrayPanel).toHaveBeenCalledTimes(1);
    });
  });

  it("reveals the tray panel without a frontend native resize", async () => {
    const warn = vi.spyOn(console, "warn").mockImplementation(() => {});
    windowMocks.getCurrentWindow.mockReturnValue({
      setSize: vi.fn().mockRejectedValue(new Error("resize failed")),
      close: vi.fn().mockResolvedValue(undefined),
      scaleFactor: vi.fn().mockResolvedValue(1),
      onResized: vi.fn().mockResolvedValue(() => {}),
      innerSize: vi.fn().mockResolvedValue({ width: 328, height: 200 }),
    });

    const { container } = renderTrayPanel([provider("claude", "Claude", 35)]);

    await waitFor(() => {
      expect(container.querySelector(".tray-panel-reveal--ready")).not.toBeNull();
    });

    warn.mockRestore();
  });

  it("does not resize the native tray window for usage-only provider updates", async () => {
    const setSize = vi.fn().mockResolvedValue(undefined);
    windowMocks.getCurrentWindow.mockReturnValue({
      setSize,
      close: vi.fn().mockResolvedValue(undefined),
      scaleFactor: vi.fn().mockResolvedValue(1),
      onResized: vi.fn().mockResolvedValue(() => {}),
      innerSize: vi.fn().mockResolvedValue({ width: 328, height: 200 }),
    });

    const { container } = renderTrayPanel([provider("claude", "Claude", 35)]);

    await waitFor(() => {
      expect(container.querySelector(".tray-panel-reveal--ready")).not.toBeNull();
    });
    setSize.mockClear();
    tauriMocks.reanchorTrayPanel.mockClear();

    act(() => {
      emitEvent("provider-updated", provider("claude", "Claude", 52));
    });
    await act(async () => {
      await new Promise((resolve) => setTimeout(resolve, 200));
    });

    expect(setSize).not.toHaveBeenCalled();
    expect(tauriMocks.reanchorTrayPanel).not.toHaveBeenCalled();
  });

  it("keeps dense provider cards in the scrollable body", async () => {
    const denseProviders = TEST_PROVIDER_CATALOG.slice(0, 36).map(([id, displayName]) =>
      provider(id, displayName),
    );

    const { container } = renderTrayPanel(denseProviders, {
      enabledProviders: denseProviders.map((snapshot) => snapshot.providerId),
    });

    await waitFor(() => {
      expect(container.querySelector(".tray-panel-reveal--native-size")).not.toBeNull();
    });
    const body = container.querySelector(".menu-surface__body");
    expect(body?.querySelectorAll(".menu-stack__item").length).toBeGreaterThan(1);
    expect(container.querySelector(".menu-surface__fixed-header .provider-grid")).not.toBeNull();
    expect(container.querySelector(".menu-surface__footer")).not.toBeNull();
  });

  it("keeps switcher and command rows outside the selected provider scroll body", async () => {
    const errorProvider = {
      ...provider("abacus", "Abacus AI", 0),
      error: "Source mode `Cli` not supported for this provider",
    };

    const { container } = renderTrayPanel([errorProvider]);

    await waitFor(() => {
      expect(container.querySelector(".tray-panel-reveal--ready")).not.toBeNull();
    });
    fireEvent.click(
      container.querySelector<HTMLButtonElement>(
        '.provider-grid__item[aria-label="Abacus AI"]',
      )!,
    );

    await waitFor(() => {
      expect(container.querySelector(".menu-stack__item--selected")).not.toBeNull();
    });
    const body = container.querySelector(".menu-surface__body");
    expect(body?.querySelector(".menu-surface__fixed-header")).toBeNull();
    expect(body?.querySelector(".menu-surface__footer")).toBeNull();
    expect(container.querySelector(".menu-surface__fixed-header")).not.toBeNull();
    expect(container.querySelector(".menu-surface__footer")).not.toBeNull();
  });

  /**
   * The reset-time mode toggle, end to end.
   *
   * `dashboardResetTimeRelative` had unit coverage on both ends — Rust proved the
   * patch persists, `quotaDisplay.test.ts` proved the formatter branches — and
   * nothing proved the setting actually reaches a rendered card. That is the same
   * gap that let the taskbar entry composer ship inert, so the assertion belongs
   * here, on a real card built from a real settings snapshot.
   */
  describe("reset time mode", () => {
    function providerWithReset(): ProviderUsageSnapshot {
      const base = provider("claude", "Claude", 35);
      return {
        ...base,
        primary: {
          ...base.primary,
kind: "session",
                    windowMinutes: 300,
          // Far enough out to be unambiguous, close enough that the wording is
          // either "today" or "tomorrow" — both carry the label, which is what
          // this asserts. Anchoring to the real clock keeps the fake-timer
          // machinery out of an async render.
          resetsAt: new Date(Date.now() + 4 * 3600_000 + 33 * 60_000).toISOString(),
        },
      };
    }

    async function resetTextWith(relative: boolean): Promise<string> {
      const { container, unmount } = renderTrayPanel([providerWithReset()], {
        dashboardResetTimeRelative: relative,
        dashboardProviderIds: [],
      });
      await waitFor(() => {
        expect(container.querySelector(".provider-quota__reset")).not.toBeNull();
      });
      const text = container.querySelector(".provider-quota__reset")!.textContent ?? "";
      unmount();
      return text;
    }

    it("counts down while the toggle is on", async () => {
      const text = await resetTextWith(true);
      expect(text).toMatch(/^Resets in \d+h \d+m$/);
    });

    it("switches the same row to a labelled wall-clock time when the toggle is off", async () => {
      const text = await resetTextWith(false);
      expect(text).toMatch(/^(Today|Tomorrow) at /);
      expect(text).not.toContain("Resets in");
    });
  });
});
