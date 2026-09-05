import { fireEvent, render, screen, waitFor } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";

const tauriMocks = vi.hoisted(() => ({
  getProviderChartData: vi.fn(),
  getLocaleStrings: vi.fn(),
  setUiLanguage: vi.fn(),
}));

const eventMocks = vi.hoisted(() => ({
  listen: vi.fn(),
}));

vi.mock("../lib/tauri", async (importOriginal) => ({
  ...(await importOriginal<typeof import("../lib/tauri")>()),
  ...tauriMocks,
}));
vi.mock("@tauri-apps/api/event", () => eventMocks);

import { LocaleProvider } from "../i18n/LocaleProvider";
import { buildBundle } from "../test/localeHarness";
import { clearChartCache } from "../core/chartAccess";
import type {
  QuotaCycleKind,
  LocalUsagePeriod,
  MenuBarDisplayMode,
  ProviderUsageSnapshot,
} from "../types/bridge";
import MenuCard from "./MenuCard";

function rateWindow(
  usedPercent = 0,
  opts: {
    exhausted?: boolean;
    resetDescription?: string | null;
    reservePercent?: number | null;
    reserveDescription?: string | null;
    reserveWillLastToReset?: boolean;
    reserveEtaSeconds?: number | null;
    windowMinutes?: number | null;
    kind?: QuotaCycleKind | null;
    resetsAt?: string | null;
  } = {},
) {
  return {
    usedPercent,
    remainingPercent: 100 - usedPercent,
    // Stated, not derived: the backend decides the cycle (`quota_cycle.rs`) and
    // ships it. A fixture that re-derived it from `windowMinutes` here would be
    // the fourth copy of the rule this field exists to delete.
    kind: opts.kind ?? null,
    windowMinutes: opts.windowMinutes ?? null,
    resetsAt: opts.resetsAt ?? null,
    resetDescription: opts.resetDescription ?? null,
    isExhausted: opts.exhausted ?? false,
    reservePercent: opts.reservePercent ?? null,
    reserveDescription: opts.reserveDescription ?? null,
    reserveWillLastToReset: opts.reserveWillLastToReset ?? false,
    reserveEtaSeconds: opts.reserveEtaSeconds ?? null,
  };
}

function provider(
  error: string | null,
  usedPercent = 0,
  opts: { exhausted?: boolean; resetDescription?: string | null } = {},
): ProviderUsageSnapshot {
  return {
    providerId: "claude",
    displayName: "Claude",
    primary: rateWindow(usedPercent, opts),
    primaryLabel: "Session",
    secondary: null,
    modelSpecific: null,
    tertiary: null,
    extraRateWindows: [],
    cost: null,
    planName: null,
    accountEmail: null,
    sourceLabel: "oauth",
    updatedAt: "2026-05-24T00:00:00Z",
    error,
    pace: null,
    accountOrganization: null,
    trayStatusLabel: null,
    fetchDurationMs: null,
  };
}

function renderCard(
  snapshot: ProviderUsageSnapshot,
  opts: {
    showAsUsed?: boolean;
    localUsagePeriod?: LocalUsagePeriod;
    densityMode?: MenuBarDisplayMode;
  } = {},
) {
  return render(
    <LocaleProvider>
      <MenuCard
        provider={snapshot}
        display={{
          showAsUsed: opts.showAsUsed ?? false,
          resetTimeRelative: true,
          highUsageThreshold: 70,
          criticalUsageThreshold: 90,
        }}
        localUsagePeriod={opts.localUsagePeriod}
        densityMode={opts.densityMode}
      />
    </LocaleProvider>,
  );
}

describe("MenuCard", () => {
  beforeEach(() => {
    // MenuCard now uses the shared chart loader. Clear an unfinished request
    // from the previous test so a deliberately pending fixture cannot bleed
    // into the next case.
    clearChartCache();
    vi.clearAllMocks();
    tauriMocks.getLocaleStrings.mockResolvedValue(
      buildBundle({
        ActionCopyError: "Copy error",
        PanelEstimatedFromLocalLogs: "Estimated from local logs",
        PanelSevenDayUsage: "Last 7 days",
        PanelThirtyDayUsage: "Last 30 days",
        PanelApiEquivalentValue: "Equivalent API value",
        PanelNoUsageSevenDays: "No usage in the last 7 days",
        PanelNoUsageThirtyDays: "No usage recorded in the last 30 days",
        PanelTokenUnit: "Token",
        PanelLocalEstimateShort: "Local log estimate, for reference",
        PanelLeftSuffix: "left",
        PanelNow: "now",
        PanelOneHour: "1h",
        PanelFiveHours: "5h",
        PanelOnPaceBudget: "On-pace budget",
        PanelUsageForecast: "Usage forecast",
        PanelForecastPrefix: "At the current pace, about",
        PanelForecastHoursUnit: "hours remaining",
        PanelForecastLessThanHour: "less than 1 hour",
        PanelForecastUntilReset: "Enough to last until the next reset",
        PanelResetCreditsTitle: "Extra resets",
        PanelResetCreditsRemaining: "Remaining",
        PanelResetCreditsUnit: "uses",
        PanelReserveSuffix: "in reserve",
        PanelThirtyDayCost: "30d cost",
        PanelThirtyDayTokens: "30d tokens",
        PanelTodayBudget: "today",
        PanelUsedSuffix: "used",
        PanelPaceWeekElapsed: "Week elapsed",
        QuotaPaceOnPace: "On pace",
        QuotaPaceInReserve: "in reserve",
        QuotaPaceInDeficit: "in deficit",
        PanelPaceOverBy: ", over by",
        PanelPaceUnderBy: ", under by",
        DetailPaceWillLastToReset: "Enough to last until the next reset",
        DetailPaceRunsOutIn: "Runs out in",
        QuotaForecastUnavailable: "Not enough data to project this week",
      }),
    );
    tauriMocks.getProviderChartData.mockResolvedValue({
      providerId: "claude",
      costHistory: [{ date: "2026-05-24", value: 1.23 }],
      creditsHistory: [],
      usageBreakdown: [],
      localUsage: {
        todayCost: null,
        todayTokens: null,
        sevenDayCost: null,
        sevenDayTokens: null,
        thirtyDayCost: 1.23,
        thirtyDayTokens: 584_000,
        todayTopModel: null,
        sevenDayTopModel: "glim-4.6",
        thirtyDayTopModel: "glim-4.6",
        estimateNote: "Estimated from local logs",
      },
    });
    eventMocks.listen.mockResolvedValue(() => {});
  });

  it("does not mix stale local usage into an error card", async () => {
    const { container } = renderCard(
      provider("OAuth error: Claude OAuth credentials not found."),
    );

    expect(
      await screen.findByText("ProviderIssueSignInRequired"),
    ).toBeInTheDocument();
    expect(container.querySelector(".menu-card--header-only")).toBeInTheDocument();
    expect(container.querySelector(".menu-card--with-details")).not.toBeInTheDocument();

    await waitFor(() => {
      expect(tauriMocks.getProviderChartData).toHaveBeenCalled();
    });

    expect(screen.queryByText("30d cost")).not.toBeInTheDocument();
    expect(screen.queryByText("30d tokens")).not.toBeInTheDocument();
    expect(screen.queryByText("Estimated from local logs")).not.toBeInTheDocument();
  });

  it("can render metric bars as used instead of remaining", async () => {
    renderCard(provider(null, 35), { showAsUsed: true });

    // The hero (first) window shows its percentage once, as the big
    // standalone number — no "35% used" text row.
    expect(await screen.findByText("35%")).toBeInTheDocument();
    expect(screen.queryByText("65% left")).not.toBeInTheDocument();

    const fill = document.querySelector<HTMLElement>(".provider-quota__fill");
    expect(fill?.style.width).toBe("35%");
  });

  it("keeps the compact quota block aligned with the reference layout", async () => {
    const snapshot = provider(null, 78);
    snapshot.planName = "ChatGPT Plus";
    snapshot.primary = rateWindow(78, {
      windowMinutes: 5 * 60,
      resetDescription: "3h 57m",
    });
    snapshot.secondaryLabel = "Weekly";
    snapshot.secondary = rateWindow(48, {
      windowMinutes: 7 * 24 * 60, kind: "weekly",
      resetDescription: "3d 2h",
    });

    const { container } = renderCard(snapshot, { densityMode: "compact" });

    expect(await screen.findByText("ProviderWeeklyLabel")).toBeInTheDocument();
    expect(container.querySelector(".menu-card--compact")).toBeInTheDocument();
    expect(container.querySelector(".menu-card__compact-secondary-track")).toBeInTheDocument();
    expect(container.querySelector(".provider-quota__fill-label")).toBeNull();
    expect(screen.queryByText("ChatGPT Plus")).not.toBeInTheDocument();
  });

  // Package item 4: 三个档位使用同一套预测视觉语言，简洁和平衡只减少信息密度，
  // 不能出现详细模式有预测、其他模式只剩空白的情况。
  //
  // The fixture is the shape Claude and Codex actually ship: the hero window is
  // the 5-hour one and the *weekly* window — the only one `pace` measures — sits
  // in `secondary`. That is precisely the case the previous implementation
  // dropped, because it attached the forecast to the hero row or not at all.
  function weeklySecondPaced() {
    const snapshot = provider(null, 78);
    snapshot.primaryLabel = "Session";
    snapshot.primary = rateWindow(78, { windowMinutes: 5 * 60 });
    snapshot.secondaryLabel = "Weekly";
    snapshot.secondary = rateWindow(52, {
      windowMinutes: 7 * 24 * 60,
      kind: "weekly",
      resetsAt: "2026-05-30T00:00:00Z",
    });
    snapshot.pace = {
      stage: "ahead",
      deltaPercent: 12.3,
      willLastToReset: true,
      etaSeconds: null,
      expectedUsedPercent: 40,
      actualUsedPercent: 52.3,
      speedMultiplierToReset: 1.5,
    };
    return snapshot;
  }

  it.each(["detailed", "compact", "minimal"] as const)(
    "renders the weekly forecast in the %s tier",
    async (densityMode) => {
      const { container } = renderCard(weeklySecondPaced(), { densityMode });

      await screen.findByText("Claude");
      const forecast = container.querySelector(".menu-metric__forecast");
      expect(forecast).toBeInTheDocument();
      // Same words in every tier, not a tier-specific paraphrase.
      expect(forecast).toHaveTextContent("12.3% in deficit");
      expect(forecast).toHaveTextContent("Enough to last until the next reset");
      expect(container.querySelector(".menu-metric__forecast--unavailable")).toBeNull();
    },
  );

  it("hangs the forecast off the weekly row, not the hero, in every tier", async () => {
    const detailed = renderCard(weeklySecondPaced(), { densityMode: "detailed" });
    await screen.findByText("Claude");
    // Detailed draws both windows as quota blocks; the forecast belongs to the
    // second one (weekly), never the first (5-hour).
    const blocks = detailed.container.querySelectorAll(".provider-quota");
    expect(blocks.length).toBeGreaterThan(1);
    expect(blocks[0].querySelector(".menu-metric__forecast")).toBeNull();
    detailed.unmount();

    const compact = renderCard(weeklySecondPaced(), { densityMode: "compact" });
    await screen.findByText("Claude");
    // Compact keeps the weekly window as its secondary row, so the forecast
    // rides along with it rather than annotating the 5-hour hero.
    expect(
      compact.container.querySelector(".menu-card__compact-secondary .menu-metric__forecast"),
    ).toBeInTheDocument();
    compact.unmount();

    const minimal = renderCard(weeklySecondPaced(), { densityMode: "minimal" });
    await screen.findByText("Claude");
    // Minimal draws one quota row only, so the weekly forecast has no row to
    // sit under and gets the standalone block instead — with no label, because
    // the summary line above already names the weekly window.
    const orphan = minimal.container.querySelector(".menu-card__orphan-forecast");
    expect(orphan).toBeInTheDocument();
    expect(orphan?.querySelector(".menu-card__orphan-forecast-label")).toBeNull();
    expect(minimal.container.querySelector(".provider-quota .menu-metric__forecast")).toBeNull();
  });

  it("drops the tier-specific pace chip and segment in favour of the forecast", async () => {
    const compact = renderCard(weeklySecondPaced(), { densityMode: "compact" });
    await screen.findByText("Claude");
    // The old compact chip printed a bare signed delta with a stage caption.
    expect(compact.container.querySelector('[data-pace-direction]')).toBeNull();
    expect(screen.queryByText("+12.3%")).not.toBeInTheDocument();
    compact.unmount();

    const minimal = renderCard(weeklySecondPaced(), { densityMode: "minimal" });
    await screen.findByText("Claude");
    expect(minimal.container.querySelector('[data-pace-direction]')).toBeNull();
    expect(screen.queryByText("+12.3%")).not.toBeInTheDocument();
  });

  it("scales the detailed provider mark to the reference proportion", async () => {
    const { container } = renderCard(provider(null, 35), {
      densityMode: "detailed",
    });

    expect(await screen.findByText("65%")).toBeInTheDocument();
    const icon = container.querySelector<HTMLElement>(".menu-card__provider-icon");
    expect(icon?.style.width).toBe("22px");
    expect(icon?.style.height).toBe("22px");
  });

  it("removes the plan badge from the minimal reference tier", async () => {
    const snapshot = provider(null, 22);
    snapshot.planName = "ChatGPT Plus";
    renderCard(snapshot, { densityMode: "minimal", showAsUsed: true });

    expect(await screen.findByText("22%")).toBeInTheDocument();
    expect(document.querySelector(".menu-card--minimal")).toBeInTheDocument();
    expect(screen.queryByText("ChatGPT Plus")).not.toBeInTheDocument();
  });

  it("displays over-quota usage without overflowing the bar", async () => {
    renderCard(provider(null, 115, { exhausted: true, resetDescription: "115% used" }), {
      showAsUsed: true,
    });

    expect(await screen.findAllByText("115% used")).not.toHaveLength(0);
    const fill = document.querySelector<HTMLElement>(".provider-quota__fill");
    expect(fill?.style.width).toBe("100%");
  });

  it("renders additional Copilot budget windows", async () => {
    const snapshot = provider(null, 20);
    snapshot.providerId = "copilot";
    snapshot.displayName = "GitHub Copilot";
    snapshot.extraRateWindows = [
      {
        id: "additional_budget",
        title: "Additional Budget",
        window: rateWindow(42),
      },
    ];

    renderCard(snapshot);

    expect(await screen.findByText("Additional Budget")).toBeInTheDocument();
    expect(screen.getByText("58%")).toBeInTheDocument();
  });

  it("uses one quota structure and duration-based labels for Codex and Claude", async () => {
    const snapshot = provider(null, 38);
    snapshot.primaryLabel = "Session";
    snapshot.primary = rateWindow(38, { windowMinutes: 5 * 60 });
    snapshot.secondaryLabel = "Weekly";
    snapshot.secondary = rateWindow(47, { windowMinutes: 7 * 24 * 60 });

    const { container } = renderCard(snapshot, { showAsUsed: true });

    expect(await screen.findByText("ProviderSessionLabel")).toBeInTheDocument();
    expect(screen.getByText("ProviderWeeklyLabel")).toBeInTheDocument();
    expect(container.querySelectorAll(".provider-quota")).toHaveLength(2);
  });

  it("does not render an empty Codex placeholder as a zero-percent quota", async () => {
    const snapshot = provider(null, 85);
    snapshot.providerId = "codex";
    snapshot.primaryLabel = "Session";
    snapshot.primary = rateWindow(85, {
      kind: "weekly",
      windowMinutes: 7 * 24 * 60,
    });
    snapshot.secondaryLabel = "Weekly";
    snapshot.secondary = rateWindow(0);

    const { container } = renderCard(snapshot, { showAsUsed: true });

    expect(await screen.findByText("ProviderWeeklyLabel")).toBeInTheDocument();
    expect(container.querySelectorAll(".provider-quota")).toHaveLength(1);
    expect(screen.queryByText("0% used")).not.toBeInTheDocument();
  });

  it("renders a DeepSeek prepaid balance without a synthetic usage row or balance badge", async () => {
    const snapshot = provider(null, 0, {
      resetDescription: "¥38.81 (Paid: ¥38.81 / Granted: ¥0.00)",
    });
    snapshot.providerId = "deepseek";
    snapshot.displayName = "DeepSeek";
    snapshot.primaryLabel = "Balance";
    snapshot.planName = "CNY balance: ¥38.81";

    renderCard(snapshot, { showAsUsed: true });

    expect(await screen.findByText("余额")).toBeInTheDocument();
    expect(screen.getByText("¥38.81")).toBeInTheDocument();
    expect(screen.queryByText("Balance")).not.toBeInTheDocument();
    expect(screen.queryByText("0% used")).not.toBeInTheDocument();
    expect(screen.queryByText("CNY balance: ¥38.81")).not.toBeInTheDocument();
  });

  it("keeps a local-usage placeholder in place while chart data loads", async () => {
    tauriMocks.getProviderChartData.mockReturnValue(new Promise(() => {}));

    const { container } = renderCard(provider(null));

    await waitFor(() => {
      expect(
        container.querySelector(".menu-card__local-usage--loading"),
      ).toBeInTheDocument();
    });
    expect(container.querySelector(".menu-card__skeleton--value")).toBeInTheDocument();
  });

  it("renders local token and cost totals after chart data loads", async () => {
    const { container } = renderCard(provider(null), { localUsagePeriod: "30d" });

    expect(await screen.findByText("Last 30 days")).toBeInTheDocument();
    expect(container.querySelector(".menu-card--with-details")).toBeInTheDocument();
    expect(container.querySelector(".menu-card--header-only")).not.toBeInTheDocument();
    expect(screen.queryByText("Last 7 days")).not.toBeInTheDocument();
    expect(screen.getByText("584,000")).toBeInTheDocument();
    expect(screen.getByText("Token")).toBeInTheDocument();
    expect(
      screen.getByText(/Equivalent API value.*\$1\.23.*¥8\.86/),
    ).toBeInTheDocument();
    // The local-estimate note line was removed from the token usage block.
    expect(
      screen.queryByText("Local log estimate, for reference"),
    ).not.toBeInTheDocument();
  });

  it("states the quota lasts to reset without printing a countdown", async () => {
    const resetAt = new Date(
      Date.now() + 0.6 * 7 * 24 * 60 * 60 * 1000,
    );
    const snapshot = provider(null, 20);
    snapshot.primary = rateWindow(20, {
      reservePercent: 20,
      reserveWillLastToReset: true,
      windowMinutes: 7 * 24 * 60, kind: "weekly",
      resetsAt: resetAt.toISOString(),
    });

    renderCard(snapshot);

    // Item A: the forecast lives INSIDE the weekly quota block, and the bar
    // itself carries the pace position. The verdict is the macOS-style
    // reserve/deficit label, not a repeated "week elapsed" yardstick — that
    // number is now the punched stripe on the bar.
    //
    // This window lasts to reset, so per macOS `detailRightLabel` it says so and
    // shows NO duration. Printing hours here made a healthy quota read like a
    // countdown to exhaustion.
    expect(
      await screen.findByText(/Enough to last until the next reset/),
    ).toBeInTheDocument();
    expect(screen.queryByText(/hours remaining/)).not.toBeInTheDocument();
    const state = document.querySelector(".menu-metric__forecast-state");
    expect(state).not.toBeNull();
    expect(state?.getAttribute("data-pace-state")).toMatch(/reserve|deficit|on-pace/);

    // The pace position interrupts the fill and carries the same state.
    const bar = document.querySelector<HTMLElement>(".provider-quota__bar");
    expect(bar).not.toBeNull();
    expect(bar?.style.getPropertyValue("--pace-x")).toMatch(/%$/);
    const track = bar?.querySelector(".provider-quota__track[data-pace]");
    expect(track).not.toBeNull();
    const stripe = bar?.querySelector<HTMLElement>(".provider-quota__pace");
    expect(stripe).not.toBeNull();
    expect(stripe?.getAttribute("data-pace-state")).toBe(
      state?.getAttribute("data-pace-state"),
    );

    // A CSS mask applies to its whole subtree, so the stripe must never live
    // inside the masked element — that bug erased the stripe and left a bare
    // gap. The mask is on the fill; the stripe is a sibling of it.
    const fill = bar?.querySelector<HTMLElement>(".provider-quota__fill");
    expect(fill).not.toBeNull();
    expect(fill?.contains(stripe!)).toBe(false);

    // The notch is expressed against the fill's own box, so its position must
    // convert back to the same place on the track as the stripe.
    const fillPercent = parseFloat(fill!.style.width);
    const paceOnTrack = parseFloat(bar!.style.getPropertyValue("--pace-x"));
    if (fill!.hasAttribute("data-notch")) {
      const fillX = parseFloat(fill!.style.getPropertyValue("--pace-fill-x"));
      expect((fillX / 100) * fillPercent).toBeCloseTo(paceOnTrack, 1);
    } else {
      // No notch only when the pace sits beyond the fill — the rail is already
      // bare there, so there is nothing to interrupt.
      expect(paceOnTrack).toBeGreaterThan(fillPercent);
    }

    // The duplicate weekly pace bar is gone.
    expect(document.querySelectorAll(".menu-card__pace-track")).toHaveLength(0);
    expect(screen.queryByText("On-pace budget")).not.toBeInTheDocument();
  });

  it("shows a forecast when timing exists without reserve metadata", async () => {
    const resetAt = new Date(Date.now() + 6 * 24 * 60 * 60 * 1000);
    const snapshot = provider(null, 31);
    snapshot.primary = rateWindow(31, {
      windowMinutes: 7 * 24 * 60, kind: "weekly",
      resetsAt: resetAt.toISOString(),
    });

    renderCard(snapshot);

    // No provider pace block here: the forecast is derived from the window's own
    // length and reset time, which is real data, so the merged block must still
    // render rather than claiming there is not enough data.
    expect(await screen.findByText(/hours remaining/)).toBeInTheDocument();
    expect(
      document.querySelector(".provider-quota__bar .provider-quota__track[data-pace]"),
    ).not.toBeNull();
    expect(
      screen.queryByText("Not enough data to project this week"),
    ).not.toBeInTheDocument();
  });

  it("shows a generic forecast for a five-hour session window with full metadata", async () => {
    const resetAt = new Date(Date.now() + 4 * 60 * 60 * 1000);
    const snapshot = provider(null, 31);
    snapshot.primary = rateWindow(31, {
      windowMinutes: 5 * 60,
      kind: "session",
      resetsAt: resetAt.toISOString(),
    });

    renderCard(snapshot);

    expect(await screen.findByText("69%")).toBeInTheDocument();
    // Session windows with a length and reset boundary now project their own
    // forecast from the window-generic logic — no pace block required.
    const forecast = document.querySelector(".menu-metric__forecast");
    expect(forecast).toBeInTheDocument();
    expect(forecast).toHaveTextContent("in deficit");
    expect(forecast).toHaveTextContent("Runs out in");
  });

  it("ignores provider.pace for a session window forecast and uses the window's own metadata", async () => {
    const resetAt = new Date(Date.now() + 4 * 60 * 60 * 1000);
    const snapshot = provider(null, 40);
    snapshot.primary = rateWindow(40, {
      kind: "session",
      windowMinutes: 5 * 60,
      resetsAt: resetAt.toISOString(),
    });
    // provider.pace measures the weekly window; this session window must
    // project from its own length/reset metadata, not the pace value.
    snapshot.pace = {
      stage: "ahead",
      deltaPercent: 12.3,
      willLastToReset: true,
      etaSeconds: null,
      expectedUsedPercent: 40,
      actualUsedPercent: 52.3,
      speedMultiplierToReset: 1.5,
    };

    renderCard(snapshot);

    expect(await screen.findByText("60%")).toBeInTheDocument();
    const forecast = document.querySelector(".menu-metric__forecast");
    expect(forecast).toBeInTheDocument();
    // The weekly pace would say "12.3% in deficit" and "Enough to last until
    // the next reset" — the session window derives its own delta and runway.
    expect(forecast).not.toHaveTextContent("12.3%");
    expect(forecast).toHaveTextContent("Runs out in");
    expect(forecast).toHaveTextContent("in deficit");
  });

  it("keeps the reserve row when timing data is incomplete", async () => {
    const snapshot = provider(null, 20);
    snapshot.primary = rateWindow(20, {
        reservePercent: 12,
        reserveWillLastToReset: true,
      });

    renderCard(snapshot);

    expect(await screen.findByText("12% in reserve")).toBeInTheDocument();
    expect(screen.queryByText("Usage forecast")).not.toBeInTheDocument();
  });

  it("renders only one forecast for a weekly window carrying both pace and reserve", async () => {
    const resetAt = new Date(Date.now() + 6 * 24 * 60 * 60 * 1000);
    const snapshot = provider(null, 52);
    snapshot.primary = rateWindow(52, {
      kind: "weekly",
      windowMinutes: 7 * 24 * 60,
      resetsAt: resetAt.toISOString(),
      reservePercent: 20,
      reserveWillLastToReset: true,
    });
    snapshot.pace = {
      stage: "ahead",
      deltaPercent: 12.3,
      willLastToReset: true,
      etaSeconds: null,
      expectedUsedPercent: 40,
      actualUsedPercent: 52.3,
      speedMultiplierToReset: 1.5,
    };

    renderCard(snapshot);

    expect(await screen.findByText("Claude")).toBeInTheDocument();
    // The new pace forecast wins; the legacy reserve row must not render
    // beside it for the same window.
    const forecasts = document.querySelectorAll(".menu-metric__forecast");
    expect(forecasts.length).toBe(1);
    expect(forecasts[0]).toHaveTextContent("12.3% in deficit");
    expect(document.querySelector(".menu-metric__reserve")).toBeNull();
  });

  it("shows a generic forecast for a monthly window with full metadata", async () => {
    const resetAt = new Date(Date.now() + 20 * 24 * 60 * 60 * 1000);
    const snapshot = provider(null, 40);
    snapshot.primary = rateWindow(40, {
      kind: "monthly",
      windowMinutes: 30 * 24 * 60,
      resetsAt: resetAt.toISOString(),
    });
    // provider.pace always measures the weekly window. Its delta/expected
    // values are deliberately far from what the monthly window's own
    // length/reset metadata derives (~6.7% deficit at ~33% elapsed, runway
    // that runs out), so a forecast contaminated by pace would be visibly
    // wrong: "15.7% in deficit" plus "Enough to last until the next reset".
    snapshot.pace = {
      stage: "ahead",
      deltaPercent: 15.7,
      willLastToReset: true,
      etaSeconds: null,
      expectedUsedPercent: 24.3,
      actualUsedPercent: 40,
      speedMultiplierToReset: 1.3,
    };

    renderCard(snapshot);

    expect(await screen.findByText("Claude")).toBeInTheDocument();
    const forecast = document.querySelector(".menu-metric__forecast");
    expect(forecast).toBeInTheDocument();
    expect(forecast).toHaveTextContent("in deficit");
    // The monthly window projects from its own length/reset metadata, not from
    // provider.pace (which measures the weekly window): neither the pace's
    // delta nor its "Enough to last" runway may appear, and its own shorter
    // runway must.
    expect(forecast).not.toHaveTextContent("15.7%");
    expect(forecast).not.toHaveTextContent("Enough to last");
    expect(forecast).toHaveTextContent("Runs out in");
    // The bar marker is still present, derived from the monthly projection.
    expect(document.querySelector(".provider-quota__bar [data-pace]")).not.toBeNull();
  });

  it("does not fabricate a forecast when window metadata is missing", async () => {
    const snapshot = provider(null, 20);
    snapshot.primary = rateWindow(20, {
      kind: "weekly",
      windowMinutes: null,
      resetsAt: null,
    });

    renderCard(snapshot);

    expect(await screen.findByText("80%")).toBeInTheDocument();
    // The weekly window gets the shared unavailable fallback rather than a
    // fabricated number. Non-weekly windows omit the slot entirely.
    expect(document.querySelector(".menu-metric__forecast--unavailable")).toBeInTheDocument();
    // No pace marker on the bar without a projection.
    expect(document.querySelector(".provider-quota__bar [data-pace]")).toBeNull();
  });

  it("does not project a forecast for informational windows", async () => {
    const snapshot = provider(null, 0);
    snapshot.primary = rateWindow(0, {
      kind: "weekly",
      resetDescription: "Subscription active",
    });
    snapshot.primary.isInformational = true;

    renderCard(snapshot);

    expect(await screen.findByText("Subscription active")).toBeInTheDocument();
    expect(document.querySelector(".menu-metric__forecast")).toBeNull();
    expect(document.querySelector(".menu-metric__reserve")).toBeNull();
  });

  it("does not project a forecast for exhausted windows", async () => {
    const resetAt = new Date(Date.now() + 6 * 24 * 60 * 60 * 1000);
    const snapshot = provider(null, 100, { exhausted: true });
    snapshot.primary = rateWindow(100, {
      kind: "weekly",
      windowMinutes: 7 * 24 * 60,
      resetsAt: resetAt.toISOString(),
      exhausted: true,
      reservePercent: 20,
      reserveWillLastToReset: true,
    });

    renderCard(snapshot);

    expect(await screen.findByText("DetailWindowExhausted")).toBeInTheDocument();
    expect(document.querySelector(".menu-metric__forecast")).toBeNull();
    expect(document.querySelector(".menu-metric__reserve")).toBeNull();
  });

  it("falls back to the legacy reserve row when a weekly window has no forecast data", async () => {
    const snapshot = provider(null, 20);
    snapshot.primary = rateWindow(20, {
      kind: "weekly",
      reservePercent: 12,
      reserveWillLastToReset: true,
    });

    renderCard(snapshot);

    expect(await screen.findByText("12% in reserve")).toBeInTheDocument();
    // No forecast slot at all: the reserve row is the compatibility fallback.
    expect(document.querySelector(".menu-metric__forecast")).toBeNull();
    expect(document.querySelector(".provider-quota__bar [data-pace]")).toBeNull();
  });

  it("renders reset credits as a remaining count instead of a quota percent", async () => {
    const snapshot = provider(null, 20);
    snapshot.providerId = "codex";
    snapshot.extraRateWindows = [{
      id: "reset-credits",
      title: "Reset credits",
      window: rateWindow(0, { resetDescription: "3 reset credits available" }),
      inventoryExpiresAt: ["2026-07-12T01:33:14Z", "2026-07-18T02:39:26Z"],
    }];

    const { container } = renderCard(snapshot);

    expect(await screen.findByText("Extra resets")).toBeInTheDocument();
    expect(screen.getByText("Remaining 3 uses")).toBeInTheDocument();
    expect(container.querySelectorAll(".provider-quota")).toHaveLength(1);
    expect(container.querySelectorAll(".menu-card__reset-credits-expiries li")).toHaveLength(2);
  });

  it("localizes the relative updated-at time in Japanese without duplicated prefix", async () => {
    tauriMocks.getLocaleStrings.mockResolvedValue(
      buildBundle({
        UpdatedJustNow: "たった今",
        UpdatedMinutesAgo: "{}分前",
        UpdatedHoursAgo: "{}時間前",
        UpdatedDaysAgo: "{}日前",
      }),
    );

    const snapshot = provider(null, 20);
    snapshot.updatedAt = new Date(Date.now() - 3 * 60 * 1000).toISOString();
    renderCard(snapshot);

    expect(await screen.findByText("3分前")).toBeInTheDocument();
  });
});
