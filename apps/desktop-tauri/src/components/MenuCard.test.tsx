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
import type { LocalUsagePeriod, ProviderUsageSnapshot } from "../types/bridge";
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
    resetsAt?: string | null;
  } = {},
) {
  return {
    usedPercent,
    remainingPercent: 100 - usedPercent,
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
    onLayoutChange?: () => void;
    localUsagePeriod?: LocalUsagePeriod;
  } = {},
) {
  return render(
    <LocaleProvider>
      <MenuCard
        provider={snapshot}
        resetTimeRelative={true}
        showAsUsed={opts.showAsUsed}
        onLayoutChange={opts.onLayoutChange}
        localUsagePeriod={opts.localUsagePeriod}
      />
    </LocaleProvider>,
  );
}

describe("MenuCard", () => {
  beforeEach(() => {
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
    snapshot.primary = rateWindow(85, { windowMinutes: 7 * 24 * 60 });
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

  it("notifies the tray panel after async local usage data loads", async () => {
    const onLayoutChange = vi.fn();

    renderCard(provider(null), { onLayoutChange });

    await waitFor(() => {
      expect(onLayoutChange).toHaveBeenCalled();
    });
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

  it("shows the current-pace forecast as remaining hours", async () => {
    const resetAt = new Date(
      Date.now() + 0.6 * 7 * 24 * 60 * 60 * 1000,
    );
    const snapshot = provider(null, 20);
    snapshot.primary = rateWindow(20, {
      reservePercent: 20,
      reserveWillLastToReset: true,
      windowMinutes: 7 * 24 * 60,
      resetsAt: resetAt.toISOString(),
    });

    renderCard(snapshot);

    expect(await screen.findByText("Usage forecast")).toBeInTheDocument();
    expect(screen.getByText(/≈ .* hours remaining/)).toBeInTheDocument();
    expect(screen.getByText("Enough to last until the next reset")).toBeInTheDocument();
    expect(screen.queryByText("On-pace budget")).not.toBeInTheDocument();
  });

  it("shows a forecast when timing exists without reserve metadata", async () => {
    const resetAt = new Date(Date.now() + 6 * 24 * 60 * 60 * 1000);
    const snapshot = provider(null, 31);
    snapshot.primary = rateWindow(31, {
      windowMinutes: 7 * 24 * 60,
      resetsAt: resetAt.toISOString(),
    });

    renderCard(snapshot);

    expect(await screen.findByText("Usage forecast")).toBeInTheDocument();
    expect(screen.getByText(/hours remaining/)).toBeInTheDocument();
    expect(screen.queryByText(/in reserve/)).not.toBeInTheDocument();
  });

  it("does not show pace budgets for a five-hour session window", async () => {
    const resetAt = new Date(Date.now() + 4 * 60 * 60 * 1000);
    const snapshot = provider(null, 31);
    snapshot.primary = rateWindow(31, {
      windowMinutes: 5 * 60,
      resetsAt: resetAt.toISOString(),
    });

    renderCard(snapshot);

    expect(await screen.findByText("69%")).toBeInTheDocument();
    expect(screen.queryByText("Usage forecast")).not.toBeInTheDocument();
    expect(
      screen.queryByRole("img", { name: /PaceChartAriaLabel/i }),
    ).not.toBeInTheDocument();
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

  it("renders reset credits as a remaining count instead of a quota percent", async () => {
    const snapshot = provider(null, 20);
    snapshot.providerId = "codex";
    snapshot.extraRateWindows = [{
      id: "reset-credits",
      title: "Reset credits",
      window: rateWindow(0, { resetDescription: "3 reset credits available" }),
    }];

    const { container } = renderCard(snapshot);

    expect(await screen.findByText("Extra resets")).toBeInTheDocument();
    expect(screen.getByText("Remaining 3 uses")).toBeInTheDocument();
    expect(container.querySelectorAll(".provider-quota")).toHaveLength(1);
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
