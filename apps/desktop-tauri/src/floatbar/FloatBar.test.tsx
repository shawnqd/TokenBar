import { act, fireEvent, render, waitFor } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";

const tauriMocks = vi.hoisted(() => ({
  getCachedProviders: vi.fn(),
  getProviderChartData: vi.fn(),
  getProviderLocalUsageSummary: vi.fn(),
  refreshProviders: vi.fn(),
  refreshProvidersIfStale: vi.fn(),
  invokeSurfaceAction: vi.fn(async () => "ok"),
  getSettingsSnapshot: vi.fn(),
  updateSettings: vi.fn(),
  getLocaleStrings: vi.fn(),
  setUiLanguage: vi.fn(),
}));

const eventMocks = vi.hoisted(() => ({
  listen: vi.fn(),
}));

const windowMocks = vi.hoisted(() => ({
  getCurrentWindow: vi.fn(() => ({
    startDragging: vi.fn().mockResolvedValue(undefined),
  })),
}));

const coreMocks = vi.hoisted(() => ({
  invoke: vi.fn().mockResolvedValue(undefined),
}));

vi.mock("../lib/tauri", () => tauriMocks);
vi.mock("@tauri-apps/api/event", () => eventMocks);
vi.mock("@tauri-apps/api/window", () => windowMocks);
vi.mock("@tauri-apps/api/core", () => coreMocks);

import FloatBar, { computeFlyoutPlacement } from "./FloatBar";
import { LocaleProvider } from "../i18n/LocaleProvider";
import { buildBundle } from "../test/localeHarness";
import type {
  BootstrapState,
  FloatBarResetWindow,
  ProviderUsageSnapshot,
  SettingsSnapshot,
} from "../types/bridge";
import {
  setFloatBarLocalCostFetcher,
} from "./floatBarStore";
import {
  __clearFloatBarStoreForTest,
  seedFloatBarFromCacheForTest,
} from "./floatBarStore.testSupport";
import { TASKBAR_PROVIDER_AUTO } from "../types/bridge";

function rateWindow(
  used: number,
  opts: {
    exhausted?: boolean;
    resetsAt?: string | null;
    resetDescription?: string | null;
  } = {},
) {
  return {
    usedPercent: used,
    remainingPercent: 100 - used,
    kind: null,
    windowMinutes: null,
    resetsAt: opts.resetsAt ?? null,
    resetDescription: opts.resetDescription ?? null,
    isExhausted: opts.exhausted ?? false,
    reservePercent: null,
    reserveDescription: null,
  };
}

function snapshot(
  id: string,
  display: string,
  used: number,
  opts: {
    exhausted?: boolean;
    error?: string | null;
    resetsAt?: string | null;
    resetDescription?: string | null;
  } = {},
): ProviderUsageSnapshot {
  return {
    providerId: id,
    displayName: display,
    primary: rateWindow(used, opts),
    secondary: null,
    modelSpecific: null,
    tertiary: null,
    extraRateWindows: [],
    cost: null,
    planName: null,
    accountEmail: null,
    sourceLabel: "auto",
    updatedAt: "2026-05-15T00:00:00Z",
    error: opts.error ?? null,
    pace: null,
    accountOrganization: null,
    trayStatusLabel: null,
  };
}

function settings(overrides: Partial<SettingsSnapshot> = {}): SettingsSnapshot {
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
    floatBarProviderIds: [],
    floatBarEntries: undefined as any,
    floatBarDarkText: false,
    floatBarShowCost: false,
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
    ...overrides,
  };
}

function bootstrap(settingsOverrides: Partial<SettingsSnapshot> = {}): BootstrapState {
  return {
    contractVersion: "v1",
    providers: [],
    settings: settings(settingsOverrides),
  };
}

async function renderFloatBar(state: BootstrapState) {
  await seedFloatBarFromCacheForTest();
  return render(
    <LocaleProvider>
      <FloatBar state={state} />
    </LocaleProvider>,
  );
}

describe("FloatBar", () => {
  beforeEach(() => {
    vi.clearAllMocks();
    __clearFloatBarStoreForTest();
    setFloatBarLocalCostFetcher(null);
    tauriMocks.refreshProviders.mockResolvedValue(undefined);
    tauriMocks.refreshProvidersIfStale.mockResolvedValue(undefined);
    tauriMocks.getProviderLocalUsageSummary.mockResolvedValue(null);
    setFloatBarLocalCostFetcher(async (providerId) => {
      const summary = await tauriMocks.getProviderLocalUsageSummary(providerId);
      if (summary == null) return null;
      return {
        todayCost: summary.todayCost ?? 0,
        thirtyDayCost: summary.thirtyDayCost ?? 0,
      };
    });
    tauriMocks.getLocaleStrings.mockResolvedValue(
      buildBundle({
        ResetsInHoursMinutes: "Resets in {}h {}m",
        ResetsInDaysHours: "Resets in {}d {}h",
        TrayResetsDueNow: "Resetting",
        PanelToday: "Today",
        PanelUsedSuffix: "used",
        FloatBarSevenDayShort: "7d",
        FloatBarThirtyDayShort: "30d",
        FloatBarNoProviders: "No providers",
        FloatBarRemainingSuffix: "remaining",
        TodayAt: "Today at {}",
        TomorrowAt: "Tomorrow at {}",
      }),
    );
    eventMocks.listen.mockResolvedValue(() => {});
  });

  it("renders a pill per enabled provider in the configured order (spec 2.2)", async () => {
    tauriMocks.getCachedProviders.mockResolvedValue([
      snapshot("claude", "Claude", 20),
      snapshot("codex", "Codex", 75),
    ]);
    tauriMocks.getSettingsSnapshot.mockResolvedValue(settings());

    const { container } = await renderFloatBar(bootstrap());
    await waitFor(() => {
      const pills = container.querySelectorAll(".floatbar__pill");
      expect(pills.length).toBe(2);
    });

    // Spec §2.2: preserve the configured order — the bar must not re-sort by
    // usage urgency behind the user's back.
    const titles = Array.from(container.querySelectorAll(".floatbar__pill")).map(
      (el) => el.getAttribute("title") ?? "",
    );
    expect(titles[0]).toMatch(/Claude: 20% used/);
    expect(titles[1]).toMatch(/Codex: 75% used/);
  });

  it("loads local cost summaries without using the foreground chart endpoint", async () => {
    tauriMocks.getCachedProviders.mockResolvedValue([
      snapshot("codex", "Codex", 75),
    ]);
    tauriMocks.getSettingsSnapshot.mockResolvedValue(settings({ floatBarShowCost: true }));
    tauriMocks.getProviderLocalUsageSummary.mockResolvedValue({
      todayCost: null,
      todayTokens: null,
      sevenDayCost: 1.25,
      sevenDayTokens: 200,
      thirtyDayCost: 12.5,
      thirtyDayTokens: 1000,
      todayTopModel: null,
      sevenDayTopModel: "gpt-5",
      thirtyDayTopModel: "gpt-5",
      estimateNote: "Estimated from local logs",
    });

    await renderFloatBar(bootstrap({ floatBarShowCost: true }));

    await waitFor(() => {
      expect(tauriMocks.getProviderLocalUsageSummary).toHaveBeenCalledWith("codex");
    });
    expect(tauriMocks.getProviderChartData).not.toHaveBeenCalled();
  });

  it("can show remaining percentages when configured", async () => {
    tauriMocks.getCachedProviders.mockResolvedValue([
      snapshot("claude", "Claude", 20),
    ]);
    tauriMocks.getSettingsSnapshot.mockResolvedValue(
      settings({ floatBarShowAsUsed: false }),
    );

    const { container } = await renderFloatBar(bootstrap({ floatBarShowAsUsed: false }));

    await waitFor(() => {
      const title = container
        .querySelector(".floatbar__pill")
        ?.getAttribute("title");
      expect(title).toContain("Claude: 80% remaining");
    });
  });

  it("ignores the dashboard and taskbar show-as-used settings", async () => {
    tauriMocks.getCachedProviders.mockResolvedValue([
      snapshot("claude", "Claude", 20),
    ]);
    const overrides = {
      floatBarShowAsUsed: true,
      dashboardShowAsUsed: false,
      taskbarShowAsUsed: false,
      taskbarTooltipEntries: [],
    };
    tauriMocks.getSettingsSnapshot.mockResolvedValue(settings(overrides));

    const { container } = await renderFloatBar(bootstrap(overrides));

    await waitFor(() => {
      const title = container
        .querySelector(".floatbar__pill")
        ?.getAttribute("title");
      expect(title).toContain("Claude: 20%");
      expect(title).not.toContain("80%");
    });
  });

  it("applies warning tone when remaining drops below the high threshold", async () => {
    tauriMocks.getCachedProviders.mockResolvedValue([snapshot("claude", "Claude", 75)]);
    tauriMocks.getSettingsSnapshot.mockResolvedValue(settings());

    const { container } = await renderFloatBar(bootstrap());
    await waitFor(() => {
      expect(container.querySelector(".floatbar__pill--warn")).not.toBeNull();
    });
  });

  it("applies critical tone when the provider is exhausted", async () => {
    tauriMocks.getCachedProviders.mockResolvedValue([
      snapshot("claude", "Claude", 100, { exhausted: true }),
    ]);
    tauriMocks.getSettingsSnapshot.mockResolvedValue(settings());

    const { container } = await renderFloatBar(bootstrap());
    await waitFor(() => {
      expect(container.querySelector(".floatbar__pill--crit")).not.toBeNull();
    });
  });

  it("filters to the floatBarProviderIds allowlist when non-empty (legacy migration)", async () => {
    tauriMocks.getCachedProviders.mockResolvedValue([
      snapshot("claude", "Claude", 30),
      snapshot("codex", "Codex", 50),
    ]);
    tauriMocks.getSettingsSnapshot.mockResolvedValue(
      settings({ floatBarProviderIds: ["codex"] }),
    );

    const { container } = await renderFloatBar(
      bootstrap({ floatBarProviderIds: ["codex"] }),
    );
    await waitFor(() => {
      const pills = container.querySelectorAll(".floatbar__pill");
      expect(pills.length).toBe(1);
      expect(pills[0].getAttribute("title")).toMatch(/Codex/);
    });
  });

  it("renders only entries listed in floatBarEntries (provider+window) – runtime consumes entries", async () => {
    tauriMocks.getCachedProviders.mockResolvedValue([
      snapshot("claude", "Claude", 30),
      snapshot("codex", "Codex", 50),
      snapshot("cursor", "Cursor", 10),
    ]);
    const withEntries = {
      floatBarEntries: [
        { providerId: "codex", window: "primary" as const },
        { providerId: "claude", window: "primary" as const },
      ],
    };
    tauriMocks.getSettingsSnapshot.mockResolvedValue(settings(withEntries));

    const { container } = await renderFloatBar(bootstrap(withEntries));
    await waitFor(() => {
      const pills = container.querySelectorAll(".floatbar__pill");
      expect(pills.length).toBe(2);
      const titles = Array.from(pills).map((el) => el.getAttribute("title") ?? "");
      expect(titles.join(" ")).toMatch(/Codex/);
      expect(titles.join(" ")).toMatch(/Claude/);
      expect(titles.join(" ")).not.toMatch(/Cursor/);
    });
  });

  it("prefers floatBarEntries over legacy ids when both present", async () => {
    tauriMocks.getCachedProviders.mockResolvedValue([
      snapshot("claude", "Claude", 30),
      snapshot("codex", "Codex", 50),
    ]);
    const both = {
      floatBarProviderIds: ["claude"],
      floatBarEntries: [{ providerId: "codex", window: "primary" as const }],
    };
    tauriMocks.getSettingsSnapshot.mockResolvedValue(settings(both));

    const { container } = await renderFloatBar(bootstrap(both));
    await waitFor(() => {
      const pills = container.querySelectorAll(".floatbar__pill");
      expect(pills.length).toBe(1);
      expect(pills[0].getAttribute("title")).toMatch(/Codex/);
      expect(pills[0].getAttribute("title")).not.toMatch(/Claude/);
    });
  });

  it("expands auto entries positionally to enabled providers", async () => {
    tauriMocks.getCachedProviders.mockResolvedValue([
      snapshot("claude", "Claude", 30),
      snapshot("codex", "Codex", 50),
      snapshot("cursor", "Cursor", 10),
    ]);
    const withAuto = {
      enabledProviders: ["claude", "codex", "cursor"],
      floatBarEntries: [
        { providerId: TASKBAR_PROVIDER_AUTO, window: "session" as const },
        { providerId: TASKBAR_PROVIDER_AUTO, window: "weekly" as const },
      ],
    };
    tauriMocks.getSettingsSnapshot.mockResolvedValue(settings(withAuto));

    const { container } = await renderFloatBar(bootstrap(withAuto));
    await waitFor(() => {
      const pills = container.querySelectorAll(".floatbar__pill");
      // auto session -> claude, auto weekly -> codex (positional)
      expect(pills.length).toBe(2);
      const titles = Array.from(pills).map((el) => el.getAttribute("title") ?? "");
      expect(titles.join(" ")).toMatch(/Claude/);
      expect(titles.join(" ")).toMatch(/Codex/);
    });
  });

  it("does not show stale cached providers when all providers are disabled", async () => {
    tauriMocks.getCachedProviders.mockResolvedValue([
      snapshot("claude", "Claude", 30),
      snapshot("codex", "Codex", 50),
    ]);
    tauriMocks.getSettingsSnapshot.mockResolvedValue(
      settings({ enabledProviders: [] }),
    );

    const { container } = await renderFloatBar(bootstrap({ enabledProviders: [] }));
    await waitFor(() => {
      expect(container.querySelectorAll(".floatbar__pill").length).toBe(0);
      expect(container.querySelector(".floatbar__empty")).not.toBeNull();
    });
  });

  it("renders unknown pills for configured entries without snapshots (spec §2.2)", async () => {
    tauriMocks.getCachedProviders.mockResolvedValue([]);
    tauriMocks.getSettingsSnapshot.mockResolvedValue(settings());

    const { container } = await renderFloatBar(bootstrap());
    await waitFor(() => {
      // 配置了条目但服务商暂无快照：渲染明确的未知态胶囊（—），
      // 不再无声消失（2026-09-06 契约）。
      expect(
        container.querySelectorAll(".floatbar__pill--unsupported").length,
      ).toBeGreaterThan(0);
    });
    expect(container.querySelector(".floatbar__empty")).toBeNull();
  });

  // 注：resolveFloatBarEntries 在条目与旧 id 都为空时回退默认条目，
  // 因此悬浮栏实际不存在“零条目空态”；无快照条目按 §2.2 渲染未知态胶囊。

  it("applies the light-background class and CSS opacity", async () => {
    tauriMocks.getCachedProviders.mockResolvedValue([]);
    tauriMocks.getSettingsSnapshot.mockResolvedValue(
      settings({ floatBarDarkText: true, floatBarOpacity: 45 }),
    );

    const { container } = await renderFloatBar(
      bootstrap({ floatBarDarkText: true, floatBarOpacity: 45 }),
    );

    await waitFor(() => {
      const bar = container.querySelector<HTMLElement>(".floatbar");
      expect(bar).not.toBeNull();
      expect(bar?.classList.contains("floatbar--light-bg")).toBe(true);
      expect(bar?.style.opacity).toBe("0.45");

      // Hovering restores opacity to 1
      act(() => {
        fireEvent.mouseEnter(bar!);
      });
      expect(bar?.style.opacity).toBe("1");

      // Mouse leave restores opacity back to configured 0.45
      act(() => {
        fireEvent.mouseLeave(bar!);
      });
      expect(bar?.style.opacity).toBe("0.45");
    });
  });

  it("applies the configured scale as a CSS variable", async () => {
    tauriMocks.getCachedProviders.mockResolvedValue([]);
    tauriMocks.getSettingsSnapshot.mockResolvedValue(settings({ floatBarScale: 150 }));

    const { container } = await renderFloatBar(bootstrap({ floatBarScale: 150 }));

    await waitFor(() => {
      const bar = container.querySelector<HTMLElement>(".floatbar");
      expect(bar).not.toBeNull();
      expect(bar?.style.getPropertyValue("--floatbar-scale")).toBe("1.5");
    });
  });

  it("uses the localized reset formatter in pill tooltips", async () => {
    const resetsAt = new Date(Date.now() + 3 * 60 * 60_000 + 42 * 60_000).toISOString();
    tauriMocks.getCachedProviders.mockResolvedValue([
      snapshot("claude", "Claude", 20, { resetsAt }),
    ]);
    tauriMocks.getSettingsSnapshot.mockResolvedValue(settings());

    const { container } = await renderFloatBar(bootstrap());

    await waitFor(() => {
      const title = container
        .querySelector(".floatbar__pill")
        ?.getAttribute("title");
      expect(title).toContain("Claude: 20% used");
      expect(title).toMatch(/Resets in 3h 4[12]m/);
      expect(title).not.toContain("Resets in due now");
    });
  });

  it("can render a next reset icon and time in provider pills", async () => {
    const resetsAt = new Date(Date.now() + 2 * 60 * 60_000 + 5 * 60_000).toISOString();
    tauriMocks.getCachedProviders.mockResolvedValue([
      snapshot("claude", "Claude", 20, { resetsAt }),
    ]);
    tauriMocks.getSettingsSnapshot.mockResolvedValue(
      settings({ floatBarShowResetInline: true }),
    );

    const { container } = await renderFloatBar(
      bootstrap({ floatBarShowResetInline: true }),
    );

    await waitFor(() => {
      const reset = container.querySelector(".floatbar__reset");
      expect(reset).not.toBeNull();
      expect(reset?.getAttribute("aria-label")).toMatch(/Resets in 2h [45]m/);
      expect(reset?.textContent).toMatch(/2h [45]m/);
      expect(reset?.textContent).not.toContain("Resets in");
    });
  });

  it("shows a labelled reset per configured window, matched by cycle length", async () => {
    const sessionReset = new Date(Date.now() + 2 * 60 * 60_000).toISOString();
    const weeklyReset = new Date(Date.now() + 48 * 60 * 60_000).toISOString();
    const provider = snapshot("claude", "Claude", 20, {
      resetsAt: sessionReset,
    });
    provider.primary.windowMinutes = 300;
    provider.primary.kind = "session";
    provider.secondary = {
      ...rateWindow(40, { resetsAt: weeklyReset }),
      kind: "weekly",
      windowMinutes: 10080,
    };
    tauriMocks.getCachedProviders.mockResolvedValue([provider]);
    const withWindows = {
      floatBarShowResetInline: true,
      floatBarResetWindows: ["session", "weekly"] as FloatBarResetWindow[],
    };
    tauriMocks.getSettingsSnapshot.mockResolvedValue(settings(withWindows));

    const { container } = await renderFloatBar(bootstrap(withWindows));

    await waitFor(() => {
      const resets = container.querySelectorAll(".floatbar__reset");
      expect(resets).toHaveLength(2);
      expect(resets[0].textContent).toContain("TaskbarWindowSession");
      expect(resets[0].textContent).toMatch(/1h 59m|2h/);
      expect(resets[1].textContent).toContain("TaskbarWindowWeekly");
      expect(resets[1].textContent).toMatch(/1d|2d/);
    });
  });

  it("prints nothing for a window the provider does not publish", async () => {
    const provider = snapshot("codex", "Codex", 20, {
      resetsAt: new Date(Date.now() + 3 * 60 * 60_000).toISOString(),
    });
    provider.primary.windowMinutes = 10080;
    provider.primary.kind = "weekly";
    tauriMocks.getCachedProviders.mockResolvedValue([provider]);
    const monthlyOnly = {
      floatBarShowResetInline: true,
      floatBarResetWindows: ["monthly"] as FloatBarResetWindow[],
    };
    tauriMocks.getSettingsSnapshot.mockResolvedValue(settings(monthlyOnly));

    const { container } = await renderFloatBar(bootstrap(monthlyOnly));

    await waitFor(() => {
      expect(container.querySelector(".floatbar__pill")).not.toBeNull();
    });
    expect(container.querySelectorAll(".floatbar__reset")).toHaveLength(0);
  });

  it("does not print the same window twice", async () => {
    const provider = snapshot("codex", "Codex", 20, {
      resetsAt: new Date(Date.now() + 24 * 60 * 60_000).toISOString(),
    });
    provider.primary.windowMinutes = 10080;
    provider.primary.kind = "weekly";
    tauriMocks.getCachedProviders.mockResolvedValue([provider]);
    const both = {
      floatBarShowResetInline: true,
      floatBarResetWindows: ["primary", "weekly"] as FloatBarResetWindow[],
    };
    tauriMocks.getSettingsSnapshot.mockResolvedValue(settings(both));

    const { container } = await renderFloatBar(bootstrap(both));

    await waitFor(() => {
      expect(container.querySelector(".floatbar__pill")).not.toBeNull();
    });
    expect(container.querySelectorAll(".floatbar__reset")).toHaveLength(1);
  });

  it("does not poll refreshProvidersIfStale on interval – data refresh is via core store, UI tick remains", async () => {
    vi.useFakeTimers();
    try {
      tauriMocks.getCachedProviders.mockResolvedValue([]);
      tauriMocks.getSettingsSnapshot.mockResolvedValue(settings());
      await act(async () => {
        await renderFloatBar(bootstrap({ refreshIntervalSecs: 60 }));
      });

      // FloatBar should not call refreshProvidersIfStale on mount; it uses core store sync instead
      // Allow microtasks to flush
      await act(async () => {
        await Promise.resolve();
      });
      expect(tauriMocks.refreshProvidersIfStale).not.toHaveBeenCalled();

      // Advance 60s – UI tick should fire but not trigger refresh
      await vi.advanceTimersByTimeAsync(60_000);
      expect(tauriMocks.refreshProvidersIfStale).not.toHaveBeenCalled();
    } finally {
      vi.useRealTimers();
    }
  });

  describe("its own usage-display switches", () => {
    const RESETS_AT = new Date(Date.now() + 2 * 3600_000 + 30 * 60_000).toISOString();

    async function pillTitle(overrides: Partial<SettingsSnapshot>): Promise<string> {
      const effective = settings(overrides);
      tauriMocks.getCachedProviders.mockResolvedValue([
        snapshot("claude", "Claude", 71, { resetsAt: RESETS_AT }),
      ]);
      tauriMocks.getSettingsSnapshot.mockResolvedValue(effective);
      const { container, unmount } = await renderFloatBar({
        contractVersion: "v1",
        providers: [],
        settings: effective,
      });
      await waitFor(() => {
        expect(container.querySelector(".floatbar__pill")).not.toBeNull();
      });
      const title = container.querySelector(".floatbar__pill")!.getAttribute("title") ?? "";
      unmount();
      __clearFloatBarStoreForTest();
      return title;
    }

    it("flips the pill between used and remaining", async () => {
      expect(await pillTitle({ floatBarShowAsUsed: true })).toContain("71% used");
      expect(await pillTitle({ floatBarShowAsUsed: false })).toContain("29% remaining");
    });

    it("flips the pill tooltip between a countdown and a wall-clock time", async () => {
      expect(await pillTitle({ floatBarResetTimeRelative: true })).toMatch(
        /Resets in \d+h \d+m/,
      );
      const absolute = await pillTitle({ floatBarResetTimeRelative: false });
      expect(absolute).toMatch(/(Today|Tomorrow) at /);
      expect(absolute).not.toContain("Resets in");
    });

    it("prints no reset chip at all until inline resets are switched on", async () => {
      const withChips = async (showResetInline: boolean) => {
        const effective = settings({ floatBarShowResetInline: showResetInline });
        tauriMocks.getCachedProviders.mockResolvedValue([
          snapshot("claude", "Claude", 71, { resetsAt: RESETS_AT }),
        ]);
        tauriMocks.getSettingsSnapshot.mockResolvedValue(effective);
        const { container, unmount } = await renderFloatBar({
          contractVersion: "v1",
          providers: [],
          settings: effective,
        });
        await waitFor(() => {
          expect(container.querySelector(".floatbar__pill")).not.toBeNull();
        });
        const count = container.querySelectorAll(".floatbar__reset").length;
        unmount();
        __clearFloatBarStoreForTest();
        return count;
      };

      expect(await withChips(false)).toBe(0);
      expect(await withChips(true)).toBe(1);
    });

    it("ignores the dashboard's copy of the same two choices", async () => {
      const title = await pillTitle({
        floatBarShowAsUsed: true,
        floatBarResetTimeRelative: true,
        dashboardShowAsUsed: false,
        dashboardResetTimeRelative: false,
        dashboardProviderIds: [],
        dashboardQuotaWindows: [],
      });
      expect(title).toContain("71% used");
      expect(title).toMatch(/Resets in \d+h \d+m/);
    });

    it("renders a micro ring gauge with computed stroke-dashoffset matching quota percentage", async () => {
      tauriMocks.getCachedProviders.mockResolvedValue([
        snapshot("claude", "Claude", 40),
      ]);
      tauriMocks.getSettingsSnapshot.mockResolvedValue(settings());

      const { container } = await renderFloatBar(bootstrap());
      await waitFor(() => {
        const gauge = container.querySelector(".floatbar__icon-gauge");
        expect(gauge).not.toBeNull();
        const fill = container.querySelector(".floatbar__ring-fill");
        expect(fill).not.toBeNull();
        expect(fill?.getAttribute("stroke-dasharray")).toBe("50.265");
        // 40% used -> 50.265 * (1 - 0.40) = 30.159 -> 30.16
        expect(fill?.getAttribute("stroke-dashoffset")).toBe("30.16");
      });
    });
  });

  it("does not render drag handle element", async () => {
    tauriMocks.getCachedProviders.mockResolvedValue([
      snapshot("claude", "Claude", 50),
    ]);
    tauriMocks.getSettingsSnapshot.mockResolvedValue(settings());

    const { container } = await renderFloatBar(bootstrap());
    await waitFor(() => {
      expect(container.querySelector(".floatbar__pill")).not.toBeNull();
    });
    expect(container.querySelector(".floatbar__handle")).toBeNull();
  });

  it("reveals HoverFlyout when hovering a pill and hides on mouse leave", async () => {
    tauriMocks.getCachedProviders.mockResolvedValue([
      snapshot("claude", "Claude", 60),
    ]);
    tauriMocks.getSettingsSnapshot.mockResolvedValue(settings());

    const { container } = await renderFloatBar(bootstrap());
    await waitFor(() => {
      expect(container.querySelector(".floatbar__pill")).not.toBeNull();
    });

    const pill = container.querySelector(".floatbar__pill")!;
    expect(container.querySelector(".floatbar__hover-flyout")).toBeNull();

    // Hover on pill reveals flyout
    act(() => {
      fireEvent.mouseEnter(pill);
    });

    await waitFor(() => {
      const flyout = container.querySelector(".floatbar__hover-flyout");
      expect(flyout).not.toBeNull();
      expect(flyout?.querySelector(".hf-title")?.textContent).toBe("Claude");
      expect(flyout?.querySelector(".hf-metric__val")?.textContent).toBe("60%");
    });

    // Mouse leave with timeout hides flyout
    act(() => {
      fireEvent.mouseLeave(pill);
    });

    await waitFor(() => {
      expect(container.querySelector(".floatbar__hover-flyout")).toBeNull();
    });
  });

  describe("computeFlyoutPlacement", () => {
    const workArea = { top: 0, bottom: 1080, left: 0, right: 1920 };

    it("places flyout at bottom-left when plenty of screen space below and to the right", () => {
      const result = computeFlyoutPlacement({
        pillRect: { top: 10, bottom: 38, left: 10, right: 120 },
        basePos: { x: 500, y: 300 },
        workArea,
        orientation: "horizontal",
      });
      expect(result).toEqual({
        placementY: "bottom",
        placementX: "left",
        padTop: 0,
        padLeft: 0,
      });
    });

    it("flips flyout upward (placementY: top) with padTop when bottom screen space is insufficient", () => {
      // FloatBar placed near the taskbar at screen bottom
      const result = computeFlyoutPlacement({
        pillRect: { top: 10, bottom: 38, left: 10, right: 120 },
        basePos: { x: 500, y: 1040 },
        workArea,
        orientation: "horizontal",
      });
      expect(result.placementY).toBe("top");
      expect(result.padTop).toBeGreaterThanOrEqual(150);
    });

    it("flips flyout to the right (placementX: right) and calculates padLeft when right screen space is insufficient", () => {
      // FloatBar placed near screen right edge
      const result = computeFlyoutPlacement({
        pillRect: { top: 10, bottom: 38, left: 10, right: 100 },
        basePos: { x: 1850, y: 300 },
        workArea,
        orientation: "horizontal",
      });
      expect(result.placementX).toBe("right");
      expect(result.padLeft).toBeGreaterThan(0);
    });

    it("supports vertical bar placement flipping on left/bottom constraints", () => {
      // Vertical bar placed near screen bottom right
      const result = computeFlyoutPlacement({
        pillRect: { top: 10, bottom: 40, left: 10, right: 120 },
        basePos: { x: 1850, y: 1000 },
        workArea,
        orientation: "vertical",
      });
      expect(result.placementX).toBe("left");
      expect(result.placementY).toBe("bottom-aligned");
      expect(result.padLeft).toBeGreaterThanOrEqual(220);
    });
  });

  it("suppresses native title attributes on hovered pill and child reset chips to prevent tooltip collisions", async () => {
    const futureReset = new Date(Date.now() + 2 * 3600_000 + 30 * 60_000).toISOString();
    tauriMocks.getCachedProviders.mockResolvedValue([
      snapshot("claude", "Claude", 45, { resetsAt: futureReset }),
    ]);
    tauriMocks.getSettingsSnapshot.mockResolvedValue(
      settings({
        floatBarShowResetInline: true,
        floatBarResetWindows: ["primary"],
      }),
    );

    const { container } = await renderFloatBar(
      bootstrap({
        floatBarShowResetInline: true,
        floatBarResetWindows: ["primary"],
      }),
    );
    await waitFor(() => {
      expect(container.querySelector(".floatbar__pill")).not.toBeNull();
      expect(container.querySelector(".floatbar__reset")).not.toBeNull();
    });

    const pill = container.querySelector(".floatbar__pill")!;
    const chip = container.querySelector(".floatbar__reset")!;

    // Before hover: titles are present
    expect(pill.getAttribute("title")).toBeTruthy();
    expect(chip.getAttribute("title")).toBeTruthy();

    // Hover pill
    act(() => {
      fireEvent.mouseEnter(pill);
    });

    await waitFor(() => {
      expect(container.querySelector(".floatbar__hover-flyout")).not.toBeNull();
    });

    // While hovered: titles are suppressed (undefined -> null in DOM)
    expect(pill.getAttribute("title")).toBeNull();
    expect(chip.getAttribute("title")).toBeNull();

    // Leave pill
    act(() => {
      fireEvent.mouseLeave(pill);
    });

    await waitFor(() => {
      expect(container.querySelector(".floatbar__hover-flyout")).toBeNull();
    });

    // After leave: titles are restored
    expect(pill.getAttribute("title")).toBeTruthy();
    expect(chip.getAttribute("title")).toBeTruthy();
  });

  it("adjusts float bar geometry with isExpanded flag when hovering and unhovering", async () => {
    tauriMocks.getCachedProviders.mockResolvedValue([
      snapshot("claude", "Claude", 60),
    ]);
    tauriMocks.getSettingsSnapshot.mockResolvedValue(settings());

    const { container } = await renderFloatBar(bootstrap());
    await waitFor(() => {
      expect(container.querySelector(".floatbar__pill")).not.toBeNull();
    });

    const pill = container.querySelector(".floatbar__pill")!;

    // Hover pill
    act(() => {
      fireEvent.mouseEnter(pill);
    });

    await waitFor(() => {
      expect(coreMocks.invoke).toHaveBeenCalledWith(
        "adjust_float_bar_geometry",
        expect.objectContaining({ isExpanded: true }),
      );
    });

    // Leave pill
    act(() => {
      fireEvent.mouseLeave(pill);
    });

    await waitFor(() => {
      expect(coreMocks.invoke).toHaveBeenCalledWith(
        "adjust_float_bar_geometry",
        expect.objectContaining({ isExpanded: false }),
      );
    });
  });

  describe("right-click context menu", () => {
    it("opens custom context menu on right click and dismisses on Escape", async () => {
      const provider = snapshot("codex", "Codex", 45);
      tauriMocks.getCachedProviders.mockResolvedValue([provider]);
      const { container } = await renderFloatBar(bootstrap({ enabledProviders: ["codex"] }));

      await waitFor(() => {
        expect(container.querySelector(".floatbar")).not.toBeNull();
      });

      const floatbar = container.querySelector(".floatbar")!;
      expect(container.querySelector(".floatbar__context-menu")).toBeNull();

      // Right-click on the bar
      act(() => {
        fireEvent.contextMenu(floatbar);
      });

      // Context menu should appear with 4 buttons
      const menu = container.querySelector(".floatbar__context-menu");
      expect(menu).not.toBeNull();
      const items = menu!.querySelectorAll(".floatbar__context-item");
      expect(items.length).toBe(4);

      // Press Escape to dismiss
      act(() => {
        fireEvent.keyDown(window, { key: "Escape" });
      });

      expect(container.querySelector(".floatbar__context-menu")).toBeNull();
    });

    it("triggers refreshProviders on clicking the refresh item", async () => {
      const provider = snapshot("codex", "Codex", 45);
      tauriMocks.getCachedProviders.mockResolvedValue([provider]);
      const { container } = await renderFloatBar(bootstrap({ enabledProviders: ["codex"] }));

      await waitFor(() => {
        expect(container.querySelector(".floatbar")).not.toBeNull();
      });

      const floatbar = container.querySelector(".floatbar")!;
      act(() => {
        fireEvent.contextMenu(floatbar);
      });

      const refreshBtn = container.querySelector(".floatbar__context-item") as HTMLButtonElement;
      expect(refreshBtn).not.toBeNull();

      act(() => {
        fireEvent.click(refreshBtn);
      });

      expect(tauriMocks.invokeSurfaceAction).toHaveBeenCalledWith({
        type: "refresh",
        force: true,
      });
      expect(container.querySelector(".floatbar__context-menu")).toBeNull();
    });
  });
});
