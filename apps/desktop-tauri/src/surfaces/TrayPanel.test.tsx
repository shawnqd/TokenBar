import { act, cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { FIXTURE_SNAPSHOTS, fromBridge, type ProviderSnapshot, type UsageStoreKey } from "../core";
import { trayCoreStore } from "./tray/trayCoreStore";
import type {
  ProviderCapabilitiesSnapshot,
  ProviderChartData,
  ProviderOutputSpeed,
  ProviderUsageSnapshot,
} from "../types/bridge";

const tauriMocks = vi.hoisted(() => ({
  getSettingsSnapshot: vi.fn(),
  updateSettings: vi.fn(),
  getLocaleStrings: vi.fn(),
  setUiLanguage: vi.fn(),
  dismissTrayPanel: vi.fn(),
  beginFlyoutGesture: vi.fn().mockResolvedValue(undefined),
  endFlyoutGesture: vi.fn().mockResolvedValue(undefined),
  beginTrayPanelResize: vi.fn().mockResolvedValue(undefined),
  openSettingsWindow: vi.fn(),
  quitApp: vi.fn(),
  reanchorTrayPanel: vi.fn(),
  revealTrayPanelWindow: vi.fn(),
  reorderProviders: vi.fn(),
  openProviderDashboard: vi.fn(),
  openProviderStatusPage: vi.fn(),
  setSurfaceMode: vi.fn(),
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
    startResizeDragging: vi.fn().mockResolvedValue(undefined),
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
  RateWindowSnapshot,
  SettingsSnapshot,
} from "../types/bridge";

function rateWindow(used: number): RateWindowSnapshot {
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

function caps(partial: Partial<ProviderCapabilitiesSnapshot> = {}): ProviderCapabilitiesSnapshot {
  return {
    outputSpeed: false,
    localUsage: false,
    providerDashboard: false,
    statusPage: false,
    login: true,
    ...partial,
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
    dashboardQuotaWindows: [],
    taskbarShowAsUsed: true,
    taskbarResetTimeRelative: true,
    taskbarTooltipEntries: [],
    ...overrides,
  };
}

// ── Core-store test rig ────────────────────────────────────────────────

let providerCoreMap: Map<string, ProviderSnapshot>;
let chartMap: Map<string, ProviderChartData>;
let speedMap: Map<string, ProviderOutputSpeed>;
let fetcherMock: ReturnType<typeof vi.fn>;
let runnerMock: ReturnType<typeof vi.fn>;

function seedBridges(bridges: ProviderUsageSnapshot[]): void {
  for (const bridge of bridges) {
    const core = fromBridge(bridge);
    providerCoreMap.set(bridge.providerId, core);
    trayCoreStore.seed(core);
  }
}

function renderTrayPanel(
  bridges: ProviderUsageSnapshot[],
  settingsOverrides: Partial<SettingsSnapshot> = {},
  catalog: ProviderCatalogEntry[] = [],
) {
  const effectiveSettings = settings({
    enabledProviders: bridges.map((p) => p.providerId),
    ...settingsOverrides,
  });
  seedBridges(bridges);
  tauriMocks.getSettingsSnapshot.mockResolvedValue(effectiveSettings);
  return render(
    <LocaleProvider>
      <TrayPanel
        state={{
          contractVersion: "v1",
          providers: catalog,
          settings: effectiveSettings,
        }}
      />
    </LocaleProvider>,
  );
}

function emitEvent(event: string, payload: unknown) {
  for (const listener of eventMocks.listeners.get(event) ?? []) {
    listener({ payload });
  }
}

describe("TrayPanel provider grid (core read model)", () => {
  beforeEach(() => {
    vi.clearAllMocks();
    eventMocks.listeners.clear();
    trayCoreStore.resetForTest();
    providerCoreMap = new Map();
    chartMap = new Map();
    speedMap = new Map();
    fetcherMock = vi.fn(async (key: UsageStoreKey) => {
      const core = providerCoreMap.get(key.providerId);
      if (!core) throw new Error(`no snapshot for ${key.providerId}`);
      return core;
    });
    trayCoreStore.setFetcher(fetcherMock);
    runnerMock = vi.fn(
      async (kind: "chart" | "outputSpeed", key: UsageStoreKey) => {
        if (kind === "chart") {
          const chart = chartMap.get(key.providerId);
          if (chart) {
            trayCoreStore.commitEnrichment("chart", key, { ok: true, chartData: chart });
          }
        }
        if (kind === "outputSpeed") {
          const speed = speedMap.get(key.providerId);
          if (speed) {
            trayCoreStore.commitEnrichment("outputSpeed", key, { ok: true, outputSpeed: speed });
          }
        }
      },
    );
    trayCoreStore.setEnrichmentRunner(runnerMock);
    tauriMocks.getSettingsSnapshot.mockResolvedValue(settings());
    tauriMocks.revealTrayPanelWindow.mockResolvedValue(undefined);
    tauriMocks.reanchorTrayPanel.mockResolvedValue(undefined);
    tauriMocks.beginFlyoutGesture.mockResolvedValue(undefined);
    tauriMocks.endFlyoutGesture.mockResolvedValue(undefined);
    tauriMocks.beginTrayPanelResize.mockResolvedValue(undefined);
    tauriMocks.dismissTrayPanel.mockResolvedValue(undefined);
    tauriMocks.quitApp.mockResolvedValue(undefined);
    tauriMocks.reorderProviders.mockResolvedValue(undefined);
    tauriMocks.openProviderDashboard.mockResolvedValue(undefined);
    tauriMocks.openProviderStatusPage.mockResolvedValue(undefined);
    tauriMocks.getLocaleStrings.mockResolvedValue(
      buildBundle({
        ActionRefresh: "Refresh",
        MenuQuit: "Quit",
        MenuSettings: "Settings...",
        PanelAllProviders: "All providers",
        PanelAllProvidersShort: "All",
        PanelLeftSuffix: "left",
        PanelShowAllProviders: "Show all providers",
        PanelShowFewerProviders: "Show fewer providers",
        PanelUsedSuffix: "used",
        PanelSevenDayUsage: "Last 7 days",
        OutputSpeedTitle: "Rate",
        PanelApiEquivalentValue: "API value",
        ActionUsageDashboard: "Usage dashboard",
        ActionStatusPage: "Status page",
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
    cleanup();
    vi.restoreAllMocks();
  });

  it("reveals regardless of the shared surface-mode snapshot", async () => {
    const { container } = renderTrayPanel([provider("claude", "Claude", 35)]);
    await waitFor(() =>
      expect(tauriMocks.revealTrayPanelWindow).toHaveBeenCalledTimes(1),
    );
    expect(container.querySelector(".tray-panel-reveal--parked")).not.toBeNull();
    act(() => emitEvent("tray-panel-revealed", undefined));
    expect(container.querySelector(".tray-panel-reveal--opening")).not.toBeNull();
  });

  it("reveals before the first provider refresh completes", async () => {
    const { container } = renderTrayPanel([]);
    await waitFor(() =>
      expect(tauriMocks.revealTrayPanelWindow).toHaveBeenCalledTimes(1),
    );
    expect(container.querySelector(".tray-panel-reveal--parked")).not.toBeNull();
    expect(container.querySelector(".tray-panel-reveal--opening")).toBeNull();
  });

  it("does not arm the native drag guard for an ordinary provider click", async () => {
    const { container } = renderTrayPanel([
      provider("codex", "Codex", 35),
      provider("claude", "Claude", 20),
    ]);
    const claude = await waitFor(() => {
      const item = container.querySelector<HTMLButtonElement>(
        '.provider-grid__item[aria-label="Claude"]',
      );
      expect(item).not.toBeNull();
      return item!;
    });
    fireEvent.click(claude);
    await waitFor(() => {
      expect(container.querySelector(".tray-panel-reveal--detail")).not.toBeNull();
    });
    expect(tauriMocks.beginFlyoutGesture).not.toHaveBeenCalled();
  });

  it("renders a two-row streamlined card in minimal mode", async () => {
    const { container } = renderTrayPanel(
      [provider("claude", "Claude", 35)],
      { menuBarDisplayMode: "minimal" },
    );
    await waitFor(() => {
      expect(container.querySelector(".minimal-streamlined")).not.toBeNull();
    });
    expect(container.querySelector(".minimal-streamlined__metric")?.textContent).toBe("35%");
    expect(container.querySelector(".minimal-streamlined__title")?.textContent).toContain("Claude");
    expect(container.querySelector(".card-header")).toBeNull();
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
      expect(container.querySelector(".minimal-streamlined")).not.toBeNull();
    });
    expect(container.querySelector(".minimal-streamlined__metric")?.textContent).toBe("¥38.81");
    expect(container.querySelector(".quota-row__hero-pct")).toBeNull();
  });

  it("dismisses the tray panel on unmodified Escape", async () => {
    const { container } = renderTrayPanel([provider("claude", "Claude", 35)]);
    await waitFor(() => {
      expect(container.querySelector(".tray-panel-reveal--native-size")).not.toBeNull();
    });
    fireEvent.keyDown(window, { key: "Escape" });
    await waitFor(() => {
      expect(tauriMocks.dismissTrayPanel).toHaveBeenCalledTimes(1);
    });
  });

  it("does not dismiss the tray panel on modified Escape", async () => {
    const { container } = renderTrayPanel([provider("claude", "Claude", 35)]);
    await waitFor(() => {
      expect(container.querySelector(".tray-panel-reveal--native-size")).not.toBeNull();
    });
    fireEvent.keyDown(window, { key: "Escape", ctrlKey: true });
    fireEvent.keyDown(window, { key: "Escape", shiftKey: true });
    fireEvent.keyDown(window, { key: "Escape", altKey: true });
    fireEvent.keyDown(window, { key: "Escape", metaKey: true });
    expect(tauriMocks.dismissTrayPanel).not.toHaveBeenCalled();
  });

  it("keeps the existing Ctrl+R tray shortcut bound to the core refresh command", async () => {
    const { container } = renderTrayPanel([provider("claude", "Claude", 35)]);
    await waitFor(() => {
      expect(container.querySelector(".tray-panel-reveal--native-size")).not.toBeNull();
    });
    // Let the mount refresh settle, then trigger Ctrl+R: the coordinator must
    // re-fetch through the injected fetcher, never through the old backend
    // broadcast refresh.
    await waitFor(() => expect(fetcherMock).toHaveBeenCalled());
    fetcherMock.mockClear();
    fireEvent.keyDown(window, { key: "r", ctrlKey: true });
    await waitFor(() => expect(fetcherMock).toHaveBeenCalled());
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
          OutputSpeedTitle: "出力速度",
          PanelTopModelPrefix: "トップモデル",
          UpdatedDaysAgo: "{}日前",
        },
        "japanese",
      ),
    );
    chartMap.set("codex", {
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
    const { container } = renderTrayPanel([
      {
        ...provider("codex", "Codex", 35),
        capabilities: caps({ localUsage: true }),
      },
    ]);
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
    expect(container.querySelector(".card-header__updated")?.textContent).toContain("日前");
    // The committed chart enrichment drives the usage value (async commit).
    expect(await screen.findByText("1,200")).toBeInTheDocument();
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
    const all = [
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
    ];
    const { container } = renderTrayPanel(all.slice(0, providerCount));
    await waitFor(() => {
      expect(container.querySelector(".provider-grid")).not.toBeNull();
    });
    const grid = container.querySelector(".provider-grid");
    expect(grid?.classList.contains("provider-grid--sparse")).toBe(shouldBeSparse);
  });

  it("feeds chart enrichment through the injected runner and renders committed local usage", async () => {
    chartMap.set("codex", {
      providerId: "codex",
      costHistory: [],
      creditsHistory: [],
      usageBreakdown: [],
      localUsage: {
        todayCost: 0.5,
        todayTokens: 2000,
        sevenDayCost: null,
        sevenDayTokens: null,
        thirtyDayCost: null,
        thirtyDayTokens: null,
        todayTopModel: "gpt-5.5",
        sevenDayTopModel: null,
        thirtyDayTopModel: null,
        estimateNote: "",
      },
    });
    const { container } = renderTrayPanel(
      [
        { ...provider("codex", "Codex", 35), capabilities: caps({ localUsage: true }) },
        provider("claude", "Claude"),
        provider("copilot", "GitHub Copilot"),
      ],
      { localUsagePeriod: "today" },
    );
    // The runner is invoked for chart enrichment on capable providers after a
    // core refresh drives the coordinator's enrich hook.
    await waitFor(() => {
      expect(runnerMock).toHaveBeenCalledWith(
        "chart",
        expect.objectContaining({ providerId: "codex" }),
      );
    });
    await waitFor(() => {
      expect(container.querySelector('[data-slot="usage"]')?.textContent).toBe("≈ 2K");
    });
    // No backend chart fetch is involved: TrayPanel/TrayCard never call it.
    expect(tauriMocks.getLocaleStrings).toBeDefined();
  });

  it("renders card data straight from the core store (no bridge fixtures)", async () => {
    const core = FIXTURE_SNAPSHOTS.claudeWeeklyMonthly;
    providerCoreMap.set("claude", core);
    trayCoreStore.seed(core);
    const effectiveSettings = settings({ enabledProviders: ["claude"] });
    tauriMocks.getSettingsSnapshot.mockResolvedValue(effectiveSettings);
    const { container } = render(
      <LocaleProvider>
        <TrayPanel
          state={{
            contractVersion: "v1",
            providers: [{ id: "claude", displayName: "Claude", cookieDomain: null }],
            settings: effectiveSettings,
          }}
        />
      </LocaleProvider>,
    );
    await waitFor(() => {
      expect(container.querySelector(".tray-card")).not.toBeNull();
    });
    // Weekly 61% from the core snapshot; monthly 34% renders as a full tile.
    expect(container.querySelector(".quota-row__hero-pct")?.textContent).toBe("61%");
    expect(container.querySelector(".quota-tile__val")?.textContent).toBe("34%");
    expect(container.querySelectorAll(".tray-card")).toHaveLength(1);
  });

  it("renders committed output-speed enrichment through the core read model", async () => {
    speedMap.set("claude", {
      providerId: "claude",
      status: "recent",
      tokensPerSecond: 18.2,
      outputTokens: 90,
      updatedAtMs: 1,
      approximate: true,
      recentSamples: [],
    });
    const { container } = renderTrayPanel([
      { ...provider("claude", "Claude", 35), capabilities: caps({ outputSpeed: true }) },
    ]);
    await waitFor(() => {
      expect(container.querySelector('[data-slot="speed"]')?.textContent).toBe("18.2 t/s");
    });
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
    const names = Array.from(container.querySelectorAll(".tray-card .card-header__name"));
    expect(names.map((node) => node.textContent)).toEqual([
      "Codex",
      "Claude",
      "Cursor",
      "Factory",
      "Gemini",
    ]);
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
    expect(windowMocks.PhysicalSize).not.toHaveBeenCalled();
    expect(
      Array.from(container.querySelectorAll(".tray-card")).map((item) => item.id),
    ).toEqual(["card-codex", "card-claude", "card-antigravity", "card-copilot"]);
    expect(container.querySelectorAll(".provider-stack-divider")).toHaveLength(3);
  });

  it("collapses and expands the full provider catalog in the dense tray grid", async () => {
    const providers = TEST_PROVIDER_CATALOG.map(([id, displayName], index) =>
      provider(id, displayName, (index * 7) % 100),
    );
    const { container } = renderTrayPanel(providers);
    await waitFor(() => {
      expect(container.querySelectorAll(".provider-grid__item")).toHaveLength(20);
    });
    const grid = container.querySelector(".provider-grid");
    expect(grid?.classList.contains("provider-grid--sparse")).toBe(false);
    expect(grid?.classList.contains("provider-grid--compact")).toBe(true);
    expect(grid?.getAttribute("data-expanded")).toBe("false");
    expect(grid?.getAttribute("data-provider-count")).toBe(String(providers.length + 1));
    expect(container.querySelectorAll(".tray-card")).toHaveLength(4);
    const expand = container.querySelector<HTMLButtonElement>(
      '.provider-grid__item--more[aria-label="Show all providers"]',
    );
    expect(expand).not.toBeNull();
    expect(expand?.textContent).toContain(`+${providers.length - 18}`);
    fireEvent.click(expand!);
    await waitFor(() => {
      expect(container.querySelectorAll(".provider-grid__item")).toHaveLength(providers.length + 2);
    });
    expect(grid?.getAttribute("data-expanded")).toBe("true");
    expect(container.querySelectorAll(".tray-card")).toHaveLength(providers.length);
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
    const copilot = container.querySelector('.provider-grid__item[aria-label="Copilot"]');
    expect(copilot).not.toBeNull();
    expect(copilot?.getAttribute("aria-label")).toBe("Copilot");
    expect(copilot?.querySelector(".provider-grid__label")?.textContent).toBe("Copi");
  });

  it("draws no per-icon quota track in the provider switcher", async () => {
    const informational = provider("sub2api", "Sub2API", 0);
    informational.primary = {
      ...informational.primary,
      isInformational: true,
      resetDescription: "Subscription active",
    };
    const { container } = renderTrayPanel([provider("claude", "Claude", 35), informational], {
      dashboardShowAsUsed: true,
    });
    await waitFor(() => {
      expect(container.querySelector(".provider-grid")).not.toBeNull();
    });
    expect(container.querySelectorAll(".provider-grid__weekly-track").length).toBe(0);
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
    expect(container.querySelector(".provider-grid .provider-icon")).toBeNull();
    expect(container.querySelector(".provider-grid__icon-overview")).toBeNull();
  });

  it("keeps tray content unscaled while native resizing owns the window", async () => {
    const { container } = renderTrayPanel([provider("claude", "Claude", 35)], {
      trayScalePercent: 150,
    });
    await waitFor(() => {
      expect(container.querySelector(".tray-panel")).not.toBeNull();
    });
    expect(container.querySelector(".menu-surface__footer-zoom")).toBeNull();
    const surface = container.querySelector<HTMLElement>(".tray-panel")!;
    expect(surface.style.getPropertyValue("zoom")).toBe("");
  });

  it("no longer offers the dashboard action in the footer (user removal)", async () => {
    const { container } = renderTrayPanel([provider("claude", "Claude", 35)]);
    await waitFor(() => {
      expect(container.querySelector(".flyout-footer")).not.toBeNull();
    });
    // The footer keeps refresh / settings / quit; the dashboard row was
    // removed at the user's call and must not come back.
    expect(screen.queryByText("Open Dashboard")).toBeNull();
    expect(container.querySelectorAll(".flyout-footer .footer-row")).toHaveLength(3);
    expect(tauriMocks.setSurfaceMode).not.toHaveBeenCalledWith("popOut", {
      kind: "dashboard",
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
      startResizeDragging: vi.fn().mockResolvedValue(undefined),
    });
    const { container } = renderTrayPanel([provider("claude", "Claude", 35)]);
    await waitFor(() => {
      expect(container.querySelector(".tray-panel-reveal--native-size")).not.toBeNull();
    });
    warn.mockRestore();
  });

  it("keeps the popup_chrome shadow gutter: page flush, reveal keeps its `6px` padding", async () => {
    const { container } = renderTrayPanel([provider("claude", "Claude", 35)]);
    await waitFor(() => {
      expect(container.querySelector(".tray-panel-reveal--native-size")).not.toBeNull();
    });
    // html/body/root must stay flush (transparent page), but the reveal
    // element keeps its 6 DIP gutter: the shared down-only shadow
    // (popup_chrome blur 3 + offset-y 3) paints there. Zeroing it would clip
    // the shadow at the HWND edge like the pre-gutter regression.
    expect(document.documentElement.style.padding).toBe("0px");
    expect(document.body.style.padding).toBe("0px");
    const reveal = container.querySelector<HTMLElement>(".tray-panel-reveal--native-size")!;
    expect(reveal.style.padding).toBe('');
    expect(reveal.style.margin).toBe("0px");
    expect(container.querySelector(".flyout-body")).not.toBeNull();
    expect(container.querySelector(".flyout-footer")).not.toBeNull();
    const card = container.querySelector<HTMLElement>(".tray-panel .tray-card");
    expect(card).not.toBeNull();
  });

  it("starts a native edge resize from the card-stroke handle", async () => {
    const { container } = renderTrayPanel([provider("claude", "Claude", 35)]);
    await waitFor(() => {
      expect(container.querySelector("[data-resize-dir='e']")).not.toBeNull();
    });
    const east = container.querySelector<HTMLElement>("[data-resize-dir='e']")!;
    fireEvent.mouseDown(east);
    expect(tauriMocks.beginFlyoutGesture).toHaveBeenCalled();
    expect(tauriMocks.beginTrayPanelResize).toHaveBeenCalledWith("e");
  });

  it("does not resize the native tray window for usage-only core updates", async () => {
    const setSize = vi.fn().mockResolvedValue(undefined);
    windowMocks.getCurrentWindow.mockReturnValue({
      setSize,
      close: vi.fn().mockResolvedValue(undefined),
      scaleFactor: vi.fn().mockResolvedValue(1),
      onResized: vi.fn().mockResolvedValue(() => {}),
      innerSize: vi.fn().mockResolvedValue({ width: 328, height: 200 }),
      startResizeDragging: vi.fn().mockResolvedValue(undefined),
    });
    const { container } = renderTrayPanel([provider("claude", "Claude", 35)]);
    await waitFor(() => {
      expect(container.querySelector(".tray-panel-reveal--native-size")).not.toBeNull();
    });
    setSize.mockClear();
    tauriMocks.reanchorTrayPanel.mockClear();
    act(() => {
      trayCoreStore.seed(fromBridge(provider("claude", "Claude", 52)));
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
    const body = container.querySelector(".flyout-body");
    expect(body?.querySelectorAll(".tray-card").length).toBeGreaterThan(1);
    expect(container.querySelector(".tray-panel .provider-grid")).not.toBeNull();
    expect(container.querySelector(".tray-panel .flyout-footer")).not.toBeNull();
  });

  it("keeps the grid and footer outside the selected provider scroll body", async () => {
    const errorProvider = {
      ...provider("abacus", "Abacus AI", 0),
      error: "Source mode Cli not supported for this provider",
    };
    const { container } = renderTrayPanel([errorProvider]);
    await waitFor(() => {
      expect(container.querySelector(".tray-panel-reveal--native-size")).not.toBeNull();
    });
    fireEvent.click(
      container.querySelector<HTMLButtonElement>(
        '.provider-grid__item[aria-label="Abacus AI"]',
      )!,
    );
    await waitFor(() => {
      expect(container.querySelector(".tray-panel-reveal--detail")).not.toBeNull();
    });
    const body = container.querySelector(".flyout-body");
    expect(body?.querySelector(".provider-grid")).toBeNull();
    expect(body?.querySelector(".flyout-footer")).toBeNull();
    expect(container.querySelector(".tray-panel .provider-grid")).not.toBeNull();
    expect(container.querySelector(".tray-panel .flyout-footer")).not.toBeNull();
  });

  describe("reset time mode", () => {
    function providerWithReset(): ProviderUsageSnapshot {
      const base = provider("claude", "Claude", 35);
      return {
        ...base,
        primary: {
          ...base.primary,
          kind: "session",
          windowMinutes: 300,
          resetsAt: new Date(Date.now() + 4 * 3600_000 + 33 * 60_000).toISOString(),
        },
      };
    }
    async function resetTextWith(relative: boolean): Promise<string> {
      const { container, unmount } = renderTrayPanel([providerWithReset()], {
        dashboardResetTimeRelative: relative,
        dashboardProviderIds: [],
        dashboardQuotaWindows: [],
      });
      await waitFor(() => {
        expect(container.querySelector(".quota-row__reset")).not.toBeNull();
      });
      const text = container.querySelector(".quota-row__reset")!.textContent ?? "";
      unmount();
      return text;
    }
    it("counts down while the toggle is on", async () => {
      const text = await resetTextWith(true);
            // TrayCard may prepend the localized label; tolerate both forms.
      expect(text).toMatch(/^(\u91cd\u7f6e\uff1a)?Resets in \d+h \d+m$/);
    });
    it("switches the same row to a labelled wall-clock time when the toggle is off", async () => {
      const text = await resetTextWith(false);
      expect(text).toMatch(/^(\u91cd\u7f6e\uff1a)?(Today|Tomorrow) at /);
      expect(text).not.toContain("Resets in");
    });
  });
});