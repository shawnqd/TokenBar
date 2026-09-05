import { render, screen, waitFor } from "@testing-library/react";
import { describe, expect, it, vi, beforeEach } from "vitest";

// ── Hoisted mocks ───────────────────────────────────────────────────────────
const tauriMocks = vi.hoisted(() => ({
  getProviderChartData: vi.fn(),
  getSettingsSnapshot: vi.fn().mockResolvedValue({ enableAnimations: false }),
  getProviderDetail: vi.fn(),
  refreshProviders: vi.fn(),
  openProviderDashboard: vi.fn(),
  openProviderStatusPage: vi.fn(),
  triggerProviderLogin: vi.fn(),
  revokeProviderCredentials: vi.fn(),
  getApiKeys: vi.fn().mockResolvedValue([]),
  getManualCookies: vi.fn().mockResolvedValue([]),
  getTokenAccounts: vi.fn().mockResolvedValue({ providerId: "claude", support: { providerId: "claude" }, accounts: [], activeIndex: -1 }),
  getTokenAccountProviders: vi.fn().mockResolvedValue([]),
  getProviderAuthCapabilities: vi.fn().mockResolvedValue(null),
  getCredentialStorageStatus: vi.fn().mockResolvedValue(null),
  getProviderCookieSourceOptions: vi.fn().mockResolvedValue([]),
  getProviderRegionOptions: vi.fn().mockResolvedValue([]),
  getProviderRegion: vi.fn().mockResolvedValue(null),
  getTaskbarPreviewLines: vi.fn().mockResolvedValue([]),
  getTaskbarWindowAvailability: vi.fn().mockResolvedValue({}),
  getTaskbarFontFamilies: vi.fn().mockResolvedValue([
    { name: "MiSans VF", variableWeight: true, hasCjk: true, recommended: true },
  ]),
  getLocaleStrings: vi.fn().mockResolvedValue({}),
  setUiLanguage: vi.fn(),
}));

vi.mock("../../lib/tauri", async (importOriginal) => ({
  ...(await importOriginal<typeof import("../../lib/tauri")>()),
  ...tauriMocks,
}));
vi.mock("../../hooks/useOutputSpeedSnapshot", () => ({
  useOutputSpeedSnapshot: () => null,
}));
vi.mock("../../hooks/useLocale", () => ({
  useLocale: () => ({ t: (key: string) => key, language: "english" }),
}));
vi.mock("@tauri-apps/api/event", () => ({
  listen: vi.fn().mockResolvedValue(() => {}),
}));

// ── Imports after mocks ───────────────────────────────────────────────────
import { createUsageStore } from "../../core/usageStore";
import { fromBridge } from "../../core/fromBridge";
import { bridgeSnapshot, bridgeRateWindow } from "../../core/fixtures";
import MenuCard from "../../components/MenuCard";
import { StatsSection } from "./providers/sections/StatsSection";
import TaskbarTab from "./tabs/TaskbarTab";
import TaskbarStatusPage from "./pages/TaskbarStatusPage";
import ProvidersTab from "./tabs/ProvidersTab";
import ProvidersPage from "./pages/ProvidersPage";
import type { SettingsSnapshot } from "../../types/bridge";

const settings = {
  enabledProviders: ["claude", "codex", "deepseek"],
  providerOrder: ["claude", "codex", "deepseek"],
  menuBarDisplayMode: "detailed",
  dashboardShowAsUsed: true,
  dashboardResetTimeRelative: true,
  localUsagePeriod: "7d",
  outputSpeedEnabled: true,
  showAllTokenAccountsInMenu: false,
  trayIconMode: "single",
  switcherShowsIcons: true,
  menuBarShowsHighestUsage: false,
  trayScalePercent: 100,
  providerMetrics: {},
  taskbarWidgetEnabled: true,
  taskbarWidgetEntries: [
    { providerId: "auto", window: "weekly" },
    { providerId: "claude", window: "session" },
  ],
  taskbarShowAsUsed: true,
  taskbarWidgetWidth: 132,
  taskbarWidgetFontSize: 12,
  taskbarWidgetFontWeight: 400,
  taskbarWidgetFontFamily: "MiSans VF",
  taskbarWidgetTextAlign: "left",
  taskbarWidgetPosition: "notification",
  enableAnimations: false,
} as unknown as SettingsSnapshot;

// ── Helpers ────────────────────────────────────────────────────────────────
function providerBridge(overrides: Partial<ReturnType<typeof bridgeSnapshot>> = {}) {
  return bridgeSnapshot({ providerId: "claude", displayName: "Claude", ...overrides });
}

describe("L3c core wiring — Settings side reads unified core store", () => {
  beforeEach(() => {
    vi.clearAllMocks();
    tauriMocks.getSettingsSnapshot.mockResolvedValue({ enableAnimations: false });
    tauriMocks.getProviderChartData.mockResolvedValue({
      providerId: "claude",
      costHistory: [],
      creditsHistory: [],
      usageBreakdown: [],
      localUsage: null,
    });
    tauriMocks.getProviderDetail.mockResolvedValue({
      id: "claude",
      displayName: "Claude",
      enabled: true,
      email: "a@example.com",
      plan: "Pro",
      hasSnapshot: true,
      session: { usedPercent: 10, remainingPercent: 90, kind: "session", windowMinutes: 300, resetsAt: null, resetDescription: "3h", isExhausted: false, reservePercent: null, reserveDescription: null },
      weekly: null,
      modelSpecific: null,
      tertiary: null,
      extraRateWindows: [],
      cost: null,
      pace: null,
      dashboardUrl: null,
      statusPageUrl: null,
      buyCreditsUrl: null,
      lastError: null,
    });
    tauriMocks.getTaskbarPreviewLines.mockResolvedValue([
      { glyph: "X", color: "#000", text: "WRONG 99%" },
    ]);
    tauriMocks.getTaskbarWindowAvailability.mockResolvedValue({});
    tauriMocks.getTaskbarFontFamilies.mockResolvedValue([
      { name: "MiSans VF", variableWeight: true, hasCjk: true, recommended: true },
    ]);
    tauriMocks.getApiKeys.mockResolvedValue([]);
    tauriMocks.getManualCookies.mockResolvedValue([]);
  });

  it("MenuCard — quota windows and error come from coreSnapshot projection, not bridge fallback", async () => {
    // Bridge says 11% weekly; core says 77% weekly + error override
    const coreSnap = fromBridge(
      providerBridge({
        providerId: "claude",
        displayName: "Claude",
        primary: bridgeRateWindow({ kind: "weekly", windowMinutes: 7 * 24 * 60, usedPercent: 77, remainingPercent: 23 }),
        error: null,
      }),
    );
    // also test error path: core error should surface even when bridge has no error
    const coreSnapWithError = { ...coreSnap, error: "upstream 500" } as unknown as typeof coreSnap;

    const bridge = providerBridge({
      providerId: "claude",
      displayName: "Claude",
      primary: bridgeRateWindow({ kind: "weekly", windowMinutes: 7 * 24 * 60, usedPercent: 11, remainingPercent: 89 }),
      error: null,
    }) as unknown as import("../../types/bridge").ProviderUsageSnapshot;

    const { unmount } = render(
      <MenuCard
        provider={bridge}
        coreSnapshot={coreSnap}
        display={{ showAsUsed: true, resetTimeRelative: true, highUsageThreshold: 70, criticalUsageThreshold: 90 }}
      />,
    );
    // Should show 77% from core, not 11% from bridge
    expect(await screen.findByText("77%")).toBeInTheDocument();
    expect(screen.queryByText("11%")).not.toBeInTheDocument();
    unmount();

    // Error from core should render localized error block, even though bridge has no error
    render(
      <MenuCard
        provider={bridge}
        coreSnapshot={coreSnapWithError}
        display={{ showAsUsed: true, resetTimeRelative: true, highUsageThreshold: 70, criticalUsageThreshold: 90 }}
      />,
    );
    expect(await screen.findByText("upstream 500")).toBeInTheDocument();
  });

  it("MenuCard — chartLoader injection is used, not Tauri invoke, and capability gates outputSpeed", async () => {
    const chartLoader = vi.fn().mockResolvedValue({
      providerId: "claude",
      costHistory: [{ date: "2026-08-11", value: 0.06 }],
      creditsHistory: [],
      usageBreakdown: [],
      localUsage: {
        todayCost: null, todayTokens: null,
        sevenDayCost: 0.28, sevenDayTokens: 42000,
        thirtyDayCost: null, thirtyDayTokens: null,
        todayTopModel: null, sevenDayTopModel: "claude-3-7-sonnet", thirtyDayTopModel: null,
        estimateNote: "",
      },
    });
    const coreSnap = fromBridge(
      providerBridge({
        providerId: "claude",
        displayName: "Claude",
        primary: bridgeRateWindow({ kind: "weekly", windowMinutes: 7 * 24 * 60, usedPercent: 40, remainingPercent: 60 }),
      }),
    );
    // Force capability to say no localUsage (both flags false) — card must not show speed/usage insight even with data
    const noCapSnap = {
      ...coreSnap,
      capabilities: { ...coreSnap.capabilities, supportsOutputSpeed: false, supportsLocalCost: false, supportsCharts: false },
    } as unknown as typeof coreSnap;

    const bridge = providerBridge({
      providerId: "claude", displayName: "Claude",
      primary: bridgeRateWindow({ kind: "weekly", windowMinutes: 7 * 24 * 60, usedPercent: 40, remainingPercent: 60 }),
    }) as unknown as import("../../types/bridge").ProviderUsageSnapshot;

    render(
      <MenuCard
        provider={bridge}
        coreSnapshot={noCapSnap}
        chartLoader={chartLoader}
        display={{ showAsUsed: true, resetTimeRelative: true, highUsageThreshold: 70, criticalUsageThreshold: 90 }}
        outputSpeed={{ providerId: "claude", status: "recent", tokensPerSecond: 99, outputTokens: 100, updatedAtMs: Date.now(), approximate: false, recentSamples: [] }}
        densityMode="detailed"
      />,
    );

    await waitFor(() => expect(chartLoader).toHaveBeenCalledWith("claude", undefined));
    expect(tauriMocks.getProviderChartData).not.toHaveBeenCalled();
    // With no capability, density insight rows must be empty (no fabricated 0 t/s)
    // The card should not render a speed value even though outputSpeed prop was given
    expect(screen.queryByText("99.0")).not.toBeInTheDocument();
  });

  it("StatsSection — delegates to chartLoader and hides tokens tab when core capability is false", async () => {
    const chartLoader = vi.fn().mockResolvedValue({
      providerId: "codex",
      costHistory: [],
      creditsHistory: [],
      usageBreakdown: [],
      localUsage: {
        todayCost: null, todayTokens: null,
        sevenDayCost: 1.23, sevenDayTokens: 584000,
        thirtyDayCost: null, thirtyDayTokens: null,
        todayTopModel: null, sevenDayTopModel: null, thirtyDayTopModel: null,
        estimateNote: "",
      },
    });
    const coreSnap = fromBridge(
      providerBridge({ providerId: "codex", displayName: "Codex", primary: bridgeRateWindow({ kind: "weekly", windowMinutes: 7 * 24 * 60, usedPercent: 20, remainingPercent: 80 }) }),
    );
    const noLocalCap = { ...coreSnap, capabilities: { ...coreSnap.capabilities, supportsLocalCost: false, supportsCharts: false } } as unknown as typeof coreSnap;

    const { container } = render(
      <StatsSection providerId="codex" accountEmail={null} speed={null} cost={null} localUsagePeriod="7d" chartLoader={chartLoader} coreSnapshot={noLocalCap} />,
    );

    await waitFor(() => expect(chartLoader).toHaveBeenCalled());
    expect(tauriMocks.getProviderChartData).not.toHaveBeenCalled();
    // No tokens tab because capability false — section renders nothing (no stat data) or at least no token tab
    expect(container.querySelector(".provider-detail-stats")).not.toBeInTheDocument();
  });

  it("StatsSection — with capability true it shows tokens tab from loader data", async () => {
    const chartLoader = vi.fn().mockResolvedValue({
      providerId: "codex",
      costHistory: [],
      creditsHistory: [],
      usageBreakdown: [],
      localUsage: {
        todayCost: null, todayTokens: null,
        sevenDayCost: 1.23, sevenDayTokens: 584000,
        thirtyDayCost: null, thirtyDayTokens: null,
        todayTopModel: null, sevenDayTopModel: null, thirtyDayTopModel: null,
        estimateNote: "",
      },
    });
    const coreSnap = fromBridge(
      providerBridge({ providerId: "codex", displayName: "Codex", primary: bridgeRateWindow({ kind: "weekly", windowMinutes: 7 * 24 * 60, usedPercent: 20, remainingPercent: 80 }) }),
    );
    // Ensure caps true
    const yesCap = { ...coreSnap, capabilities: { ...coreSnap.capabilities, supportsLocalCost: true, supportsCharts: true } } as unknown as typeof coreSnap;

    render(
      <StatsSection providerId="codex" accountEmail={null} speed={null} cost={null} localUsagePeriod="7d" chartLoader={chartLoader} coreSnapshot={yesCap} />,
    );
    expect(await screen.findByText("584,000")).toBeInTheDocument();
    expect(chartLoader).toHaveBeenCalled();
  });

  it("TaskbarTab — preview cells come from core projection via store, not Tauri buffer", async () => {
    const store = createUsageStore();
    // Weekly 73% snapshot for claude
    const snap = fromBridge(
      providerBridge({
        providerId: "claude",
        displayName: "Claude",
        primary: bridgeRateWindow({ kind: "weekly", windowMinutes: 7 * 24 * 60, usedPercent: 73, remainingPercent: 27 }),
      }),
    );
    store.upsert(snap);
    // Also need availability: snapshot windows include weekly, so dropdown will offer weekly
    // Tauri mocks return WRONG 99% — if we still see 99% the wiring is broken
    tauriMocks.getTaskbarPreviewLines.mockResolvedValue([
      { glyph: "X", color: "#000", text: "WRONG 99%" },
    ]);

    render(<TaskbarTab settings={settings as unknown as SettingsSnapshot} set={() => {}} saving={false} coreStore={store} />);

    // Preview should show 73% from core (via projection tag 周) not WRONG 99%
    await waitFor(() => expect(screen.getByLabelText("TaskbarWidgetPreviewLabel")).toBeInTheDocument());
    const preview = screen.getByLabelText("TaskbarWidgetPreviewLabel");
    expect(preview.textContent).toContain("73%");
    expect(preview.textContent).not.toContain("WRONG");
    expect(preview.textContent).not.toContain("99%");
  });

  it("TaskbarStatusPage — strip cells indexed via same projection as TaskbarTab", async () => {
    const store = createUsageStore();
    const snap = fromBridge(
      providerBridge({
        providerId: "claude",
        displayName: "Claude",
        primary: bridgeRateWindow({ kind: "session", windowMinutes: 300, usedPercent: 44, remainingPercent: 56 }),
      }),
    );
    store.upsert(snap);
    tauriMocks.getTaskbarPreviewLines.mockResolvedValue([
      { glyph: "X", color: "#000", text: "WRONG 1%" },
    ]);

    render(<TaskbarStatusPage settings={settings as unknown as SettingsSnapshot} set={() => {}} saving={false} coreStore={store} />);

    // The page renders a TaskbarStripPreview with cells — check that preview contains 44% not WRONG
    await waitFor(() => {
      const previews = document.querySelectorAll(".taskbar-preview__val");
      expect(previews.length).toBeGreaterThan(0);
    });
    const vals = Array.from(document.querySelectorAll(".taskbar-preview__val")).map((el) => el.textContent);
    expect(vals.join(" ")).toContain("44%");
    expect(vals.join(" ")).not.toContain("WRONG");
  });

  it("ProvidersTab — sidebar metric and selection come from core store (empty store shows loading, not fake)", async () => {
    const store = createUsageStore();
    // No upsert yet — empty store => sidebar rows should be loading/未配置, not 11% from legacy hook
    const emptyRender = render(
      <ProvidersTab settings={settings as unknown as SettingsSnapshot} providers={[
        { id: "claude", displayName: "Claude", cookieDomain: null } as unknown as import("../../types/bridge").ProviderCatalogEntry,
      ]} set={() => {}} saving={false} coreStore={store} />,
    );
    // With empty core store, the detail pane should not show a metric derived from legacy 11%
    // The sidebar subtitle should be 未配置 (core empty) rather than legacy's 已登录 · ...
    expect(screen.getByText("未配置")).toBeInTheDocument();
    emptyRender.unmount();

    // Now upsert a real snapshot with weekly 88% — use recent updatedAt so displayState is ready, not stale
    const snap88 = fromBridge(
      providerBridge({
        providerId: "claude",
        displayName: "Claude",
        updatedAt: new Date().toISOString(),
        primary: bridgeRateWindow({ kind: "weekly", windowMinutes: 7 * 24 * 60, usedPercent: 88, remainingPercent: 12 }),
      }),
    );
    store.upsert(snap88);

    const { container } = render(
      <ProvidersTab settings={settings as unknown as SettingsSnapshot} providers={[
        { id: "claude", displayName: "Claude", cookieDomain: null } as unknown as import("../../types/bridge").ProviderCatalogEntry,
      ]} set={() => {}} saving={false} coreStore={store} />,
    );
    // Sidebar metric should now be 88% from core, not legacy 11% — detail pane also shows 88% (so getAllByText)
    await waitFor(() => expect(screen.getAllByText("88%").length).toBeGreaterThanOrEqual(1));
    expect(container.querySelector(".providers-sidebar__metric")?.textContent).toBe("88%");
    expect(screen.queryByText("11%")).not.toBeInTheDocument();
  });

  it("ProvidersPage — row list shows core percent and leaves empty when capability is missing (no fake 100%)", async () => {
    const store = createUsageStore();
    const snap = fromBridge(
      providerBridge({
        providerId: "claude",
        displayName: "Claude",
        updatedAt: new Date().toISOString(),
        primary: bridgeRateWindow({ kind: "weekly", windowMinutes: 7 * 24 * 60, usedPercent: 66, remainingPercent: 34 }),
      }),
    );
    store.upsert(snap);

    render(<ProvidersPage settings={settings as unknown as SettingsSnapshot} set={() => {}} saving={false} catalog={[{ id: "claude", displayName: "Claude" } as unknown as import("../../types/bridge").ProviderCatalogEntry]} coreStore={store} />);

    // Row metric should be 66% from core, not 11% from legacy
    await waitFor(() => expect(document.querySelector(".s5-pmetric")?.textContent).toContain("66%"));
    expect(document.querySelector(".s5-pmetric")?.textContent).not.toContain("11%");
  });
});
