import { act, render, waitFor } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";

const tauriMocks = vi.hoisted(() => ({
  getCachedProviders: vi.fn(),
  getProviderChartData: vi.fn(),
  getProviderLocalUsageSummary: vi.fn(),
  refreshProviders: vi.fn(),
  refreshProvidersIfStale: vi.fn(),
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

import FloatBar from "./FloatBar";
import { LocaleProvider } from "../i18n/LocaleProvider";
import { buildBundle } from "../test/localeHarness";
import type {
  BootstrapState,
  FloatBarResetWindow,
  ProviderUsageSnapshot,
  SettingsSnapshot,
} from "../types/bridge";

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
    windowScalePercent: 125,
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
    taskbarShowAsUsed: true,
    taskbarResetTimeRelative: true,
    taskbarContextMenuActions: ["open_panel", "refresh", "settings", "quit"],
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

function renderFloatBar(state: BootstrapState) {
  return render(
    <LocaleProvider>
      <FloatBar state={state} />
    </LocaleProvider>,
  );
}

describe("FloatBar", () => {
  beforeEach(() => {
    vi.clearAllMocks();
    tauriMocks.refreshProviders.mockResolvedValue(undefined);
    tauriMocks.refreshProvidersIfStale.mockResolvedValue(undefined);
    tauriMocks.getProviderLocalUsageSummary.mockResolvedValue(null);
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

  it("renders a pill per enabled provider, sorted by usage descending", async () => {
    tauriMocks.getCachedProviders.mockResolvedValue([
      snapshot("claude", "Claude", 20),
      snapshot("codex", "Codex", 75),
    ]);
    tauriMocks.getSettingsSnapshot.mockResolvedValue(settings());

    const { container } = renderFloatBar(bootstrap());
    await waitFor(() => {
      const pills = container.querySelectorAll(".floatbar__pill");
      expect(pills.length).toBe(2);
    });

    const titles = Array.from(container.querySelectorAll(".floatbar__pill")).map(
      (el) => el.getAttribute("title") ?? "",
    );
    // Highest used (codex, 75%) shows first; display follows showAsUsed.
    expect(titles[0]).toMatch(/Codex: 75% used/);
    expect(titles[1]).toMatch(/Claude: 20% used/);
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

    renderFloatBar(bootstrap({ floatBarShowCost: true }));

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

    const { container } = renderFloatBar(bootstrap({ floatBarShowAsUsed: false }));

    await waitFor(() => {
      const title = container
        .querySelector(".floatbar__pill")
        ?.getAttribute("title");
      expect(title).toContain("Claude: 80% remaining");
    });
  });

  /** The floating bar must not follow the dashboard's or the taskbar's choice. */
  it("ignores the dashboard and taskbar show-as-used settings", async () => {
    tauriMocks.getCachedProviders.mockResolvedValue([
      snapshot("claude", "Claude", 20),
    ]);
    const overrides = {
      floatBarShowAsUsed: true,
      dashboardShowAsUsed: false,
      taskbarShowAsUsed: false,
    taskbarContextMenuActions: ["open_panel", "refresh", "settings", "quit"],
    taskbarTooltipEntries: [],
    };
    tauriMocks.getSettingsSnapshot.mockResolvedValue(settings(overrides));

    const { container } = renderFloatBar(bootstrap(overrides));

    await waitFor(() => {
      const title = container
        .querySelector(".floatbar__pill")
        ?.getAttribute("title");
      expect(title).toContain("Claude: 20%");
      expect(title).not.toContain("80%");
    });
  });

  it("applies warning tone when remaining drops below the high threshold", async () => {
    // highUsageThreshold = 70 → high-remaining cutoff = 30%.
    // claude at 80% used → 20% remaining → critical (also below crit cutoff 10).
    // Use 75% used → 25% remaining → warn (between 10 and 30).
    tauriMocks.getCachedProviders.mockResolvedValue([snapshot("claude", "Claude", 75)]);
    tauriMocks.getSettingsSnapshot.mockResolvedValue(settings());

    const { container } = renderFloatBar(bootstrap());
    await waitFor(() => {
      expect(container.querySelector(".floatbar__pill--warn")).not.toBeNull();
    });
  });

  it("applies critical tone when the provider is exhausted", async () => {
    tauriMocks.getCachedProviders.mockResolvedValue([
      snapshot("claude", "Claude", 100, { exhausted: true }),
    ]);
    tauriMocks.getSettingsSnapshot.mockResolvedValue(settings());

    const { container } = renderFloatBar(bootstrap());
    await waitFor(() => {
      expect(container.querySelector(".floatbar__pill--crit")).not.toBeNull();
    });
  });

  it("filters to the floatBarProviderIds allowlist when non-empty", async () => {
    tauriMocks.getCachedProviders.mockResolvedValue([
      snapshot("claude", "Claude", 30),
      snapshot("codex", "Codex", 50),
    ]);
    tauriMocks.getSettingsSnapshot.mockResolvedValue(
      settings({ floatBarProviderIds: ["codex"] }),
    );

    const { container } = renderFloatBar(
      bootstrap({ floatBarProviderIds: ["codex"] }),
    );
    await waitFor(() => {
      const pills = container.querySelectorAll(".floatbar__pill");
      expect(pills.length).toBe(1);
      expect(pills[0].getAttribute("title")).toMatch(/Codex/);
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

    const { container } = renderFloatBar(bootstrap({ enabledProviders: [] }));
    await waitFor(() => {
      expect(container.querySelectorAll(".floatbar__pill").length).toBe(0);
      expect(container.querySelector(".floatbar__empty")).not.toBeNull();
    });
  });

  it("shows an empty state when no providers match", async () => {
    tauriMocks.getCachedProviders.mockResolvedValue([]);
    tauriMocks.getSettingsSnapshot.mockResolvedValue(settings());

    const { container } = renderFloatBar(bootstrap());
    await waitFor(() => {
      expect(container.querySelector(".floatbar__empty")).not.toBeNull();
    });
  });

  it("applies the light-background class and CSS opacity", async () => {
    tauriMocks.getCachedProviders.mockResolvedValue([]);
    tauriMocks.getSettingsSnapshot.mockResolvedValue(
      settings({ floatBarDarkText: true, floatBarOpacity: 45 }),
    );

    const { container } = renderFloatBar(
      bootstrap({ floatBarDarkText: true, floatBarOpacity: 45 }),
    );

    await waitFor(() => {
      const bar = container.querySelector<HTMLElement>(".floatbar");
      expect(bar).not.toBeNull();
      expect(bar?.classList.contains("floatbar--light-bg")).toBe(true);
      expect(bar?.style.opacity).toBe("0.45");
    });
  });

  it("applies the configured scale as a CSS variable", async () => {
    tauriMocks.getCachedProviders.mockResolvedValue([]);
    tauriMocks.getSettingsSnapshot.mockResolvedValue(settings({ floatBarScale: 150 }));

    const { container } = renderFloatBar(bootstrap({ floatBarScale: 150 }));

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

    const { container } = renderFloatBar(bootstrap());

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

    const { container } = renderFloatBar(
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

  /// Providers disagree about which slot holds which cycle, so a named window
  /// has to be matched by declared LENGTH. Claude keeps its weekly quota in
  /// `secondary`; asking for "weekly" must not return the 5-hour `primary`.
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

    const { container } = renderFloatBar(bootstrap(withWindows));

    await waitFor(() => {
      const resets = container.querySelectorAll(".floatbar__reset");
      expect(resets).toHaveLength(2);
      // Two deadlines side by side are meaningless unless each is named. The
      // locale harness echoes these keys back rather than the display strings.
      expect(resets[0].textContent).toContain("TaskbarWindowSession");
      expect(resets[0].textContent).toMatch(/1h 59m|2h/);
      expect(resets[1].textContent).toContain("TaskbarWindowWeekly");
      expect(resets[1].textContent).toMatch(/1d|2d/);
    });
  });

  /// A provider that does not publish the requested cycle must show nothing
  /// for it, never somebody else's window standing in.
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

    const { container } = renderFloatBar(bootstrap(monthlyOnly));

    await waitFor(() => {
      expect(container.querySelector(".floatbar__pill")).not.toBeNull();
    });
    expect(container.querySelectorAll(".floatbar__reset")).toHaveLength(0);
  });

  /// `primary` and a named cycle routinely resolve to the same window — Codex's
  /// primary IS its weekly — and printing it twice would read as two deadlines.
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

    const { container } = renderFloatBar(bootstrap(both));

    await waitFor(() => {
      expect(container.querySelector(".floatbar__pill")).not.toBeNull();
    });
    expect(container.querySelectorAll(".floatbar__reset")).toHaveLength(1);
  });

  it("polls refreshProvidersIfStale on the configured interval", async () => {
    vi.useFakeTimers();
    try {
      tauriMocks.getCachedProviders.mockResolvedValue([]);
      tauriMocks.getSettingsSnapshot.mockResolvedValue(settings());
      // 60s minimum is enforced in FloatBar.tsx; use the floor here.
      await act(async () => {
        renderFloatBar(bootstrap({ refreshIntervalSecs: 60 }));
      });

      // Initial tick fires synchronously on mount; useProviders is passive here
      // so the floatbar does not double-request stale refreshes at startup.
      await vi.waitFor(() => {
        expect(tauriMocks.refreshProvidersIfStale).toHaveBeenCalledTimes(1);
      });
      const initialCalls = tauriMocks.refreshProvidersIfStale.mock.calls.length;

      // Advance the timer past the 60-second interval — the floatbar tick
      // should fire again.
      await vi.advanceTimersByTimeAsync(60_000);
      expect(tauriMocks.refreshProvidersIfStale.mock.calls.length).toBeGreaterThan(
        initialCalls,
      );
    } finally {
      vi.useRealTimers();
    }
  });

  /**
   * The floating bar's two "用量显示" switches, end to end.
   *
   * `floatBarShowAsUsed` and `floatBarResetTimeRelative` were reported as
   * having no effect on the bar. Both were covered only at their ends — Rust
   * proves the patch persists, `quotaDisplay.test.ts` proves the helpers branch
   * — with nothing asserting a rendered pill follows them.
   */
  describe("its own usage-display switches", () => {
    const RESETS_AT = new Date(Date.now() + 2 * 3600_000 + 30 * 60_000).toISOString();

    async function pillTitle(overrides: Partial<SettingsSnapshot>): Promise<string> {
      const effective = settings(overrides);
      tauriMocks.getCachedProviders.mockResolvedValue([
        snapshot("claude", "Claude", 71, { resetsAt: RESETS_AT }),
      ]);
      tauriMocks.getSettingsSnapshot.mockResolvedValue(effective);
      const { container, unmount } = renderFloatBar({
        contractVersion: "v1",
        providers: [],
        settings: effective,
      });
      await waitFor(() => {
        expect(container.querySelector(".floatbar__pill")).not.toBeNull();
      });
      const title = container.querySelector(".floatbar__pill")!.getAttribute("title") ?? "";
      unmount();
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

    /**
     * Why the switch was reported as dead.
     *
     * The bar renders a reset chip only when `floatBarShowResetInline` is on,
     * and that lives in a different section of the same page. With it off — the
     * default — the reset-time mode reaches the hover tooltip and nothing else,
     * which on screen is indistinguishable from a broken toggle. FloatBarTab
     * now says so in the field description.
     */
    it("prints no reset chip at all until inline resets are switched on", async () => {
      const withChips = async (showResetInline: boolean) => {
        const effective = settings({ floatBarShowResetInline: showResetInline });
        tauriMocks.getCachedProviders.mockResolvedValue([
          snapshot("claude", "Claude", 71, { resetsAt: RESETS_AT }),
        ]);
        tauriMocks.getSettingsSnapshot.mockResolvedValue(effective);
        const { container, unmount } = renderFloatBar({
          contractVersion: "v1",
          providers: [],
          settings: effective,
        });
        await waitFor(() => {
          expect(container.querySelector(".floatbar__pill")).not.toBeNull();
        });
        const count = container.querySelectorAll(".floatbar__reset").length;
        unmount();
        return count;
      };

      expect(await withChips(false)).toBe(0);
      expect(await withChips(true)).toBe(1);
    });

    /// The dashboard owns a separate copy; the bar must ignore it entirely.
    it("ignores the dashboard's copy of the same two choices", async () => {
      const title = await pillTitle({
        floatBarShowAsUsed: true,
        floatBarResetTimeRelative: true,
        dashboardShowAsUsed: false,
        dashboardResetTimeRelative: false,
      });
      expect(title).toContain("71% used");
      expect(title).toMatch(/Resets in \d+h \d+m/);
    });
  });
});
