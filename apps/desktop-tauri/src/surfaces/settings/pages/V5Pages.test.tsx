import { fireEvent, render, screen, waitFor } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";
import { TEST_PROVIDER_CATALOG } from "../../../test/providerCatalog";
import type { SettingsSnapshot } from "../../../types/bridge";
import ProvidersPage from "./ProvidersPage";
import TrayPanelPage from "./TrayPanelPage";
import { buildProviderCatalog } from "./htmlFixture";

const tauriMocks = vi.hoisted(() => ({
  getProviderDetail: vi.fn(),
  refreshProviders: vi.fn(),
  openProviderDashboard: vi.fn(),
  openProviderStatusPage: vi.fn(),
  triggerProviderLogin: vi.fn(),
  revokeProviderCredentials: vi.fn(),
  getProviderChartData: vi.fn(),
  getSettingsSnapshot: vi.fn(),
  getApiKeys: vi.fn(),
  getManualCookies: vi.fn(),
  getTokenAccounts: vi.fn(),
  getGeminiCliSignedIn: vi.fn(),
  importCookieFile: vi.fn(),
  openProviderLogin: vi.fn(),
  captureProviderLogin: vi.fn(),
  closeProviderLogin: vi.fn(),
  setManualCookie: vi.fn(),
  removeManualCookie: vi.fn(),
  setProviderCookieSource: vi.fn(),
  setApiKey: vi.fn(),
  removeApiKey: vi.fn(),
}));

vi.mock("../../../hooks/useLocale", () => ({
  useLocale: () => ({ t: (key: string) => key }),
}));

import { clearChartCache } from "../../../core/chartAccess";

vi.mock("../../../lib/tauri", () => ({
  getProviderDetail: tauriMocks.getProviderDetail,
  refreshProviders: tauriMocks.refreshProviders,
  openProviderDashboard: tauriMocks.openProviderDashboard,
  openProviderStatusPage: tauriMocks.openProviderStatusPage,
  triggerProviderLogin: tauriMocks.triggerProviderLogin,
  revokeProviderCredentials: tauriMocks.revokeProviderCredentials,
  getProviderChartData: tauriMocks.getProviderChartData,
  getSettingsSnapshot: tauriMocks.getSettingsSnapshot,
  getApiKeys: tauriMocks.getApiKeys,
  getManualCookies: tauriMocks.getManualCookies,
  getTokenAccounts: tauriMocks.getTokenAccounts,
  getGeminiCliSignedIn: tauriMocks.getGeminiCliSignedIn,
  importCookieFile: tauriMocks.importCookieFile,
  openProviderLogin: tauriMocks.openProviderLogin,
  captureProviderLogin: tauriMocks.captureProviderLogin,
  closeProviderLogin: tauriMocks.closeProviderLogin,
  setManualCookie: tauriMocks.setManualCookie,
  removeManualCookie: tauriMocks.removeManualCookie,
  setProviderCookieSource: tauriMocks.setProviderCookieSource,
  setApiKey: tauriMocks.setApiKey,
  removeApiKey: tauriMocks.removeApiKey,
}));

vi.mock("../../../hooks/useProviders", () => ({
  useProviders: () => ({
    providers: [
      {
        providerId: "claude",
        displayName: "Claude",
        primary: {
          usedPercent: 41,
          remainingPercent: 59,
          kind: "session",
          windowMinutes: 300,
          resetsAt: null,
          resetDescription: "3h",
          isExhausted: false,
          reservePercent: null,
          reserveDescription: null,
        },
        updatedAt: new Date().toISOString(),
        error: null,
      },
    ],
    isRefreshing: false,
    refresh: vi.fn(),
    lastRefresh: null,
    hasCachedData: true,
    hasLoadedCache: true,
  }),
}));

vi.mock("../../../hooks/useOutputSpeedSnapshot", () => ({
  useOutputSpeedSnapshot: () => null,
}));

const snapshot = {
  enabledProviders: ["claude", "codex", "deepseek"],
  providerOrder: ["claude", "codex", "deepseek", "cursor", "gemini", "azure"],
  menuBarDisplayMode: "detailed",
  dashboardShowAsUsed: true,
  dashboardResetTimeRelative: true,
  localUsagePeriod: "today",
  outputSpeedEnabled: true,
  showAllTokenAccountsInMenu: false,
  trayIconMode: "single",
  switcherShowsIcons: true,
  menuBarShowsHighestUsage: false,
  trayScalePercent: 100,
  providerMetrics: {},
} as unknown as SettingsSnapshot;

describe("V5 settings pages from HTML", () => {
  tauriMocks.getProviderDetail.mockResolvedValue({
    id: "claude",
    displayName: "Claude",
    enabled: true,
    email: "a***@example.com",
    plan: "专业版",
    hasSnapshot: true,
    session: {
      usedPercent: 41,
      remainingPercent: 59,
      kind: "session",
      windowMinutes: 300,
      resetsAt: null,
      resetDescription: "3h",
      isExhausted: false,
      reservePercent: null,
      reserveDescription: null,
    },
    weekly: null,
    modelSpecific: null,
    tertiary: null,
    extraRateWindows: [],
    cost: null,
    pace: null,
    dashboardUrl: "https://claude.ai/settings/usage",
    statusPageUrl: "https://status.claude.com/",
    buyCreditsUrl: "https://claude.ai/upgrade",
    lastError: null,
  });
  tauriMocks.getProviderChartData.mockResolvedValue(null);
  clearChartCache();
  tauriMocks.getSettingsSnapshot.mockResolvedValue({ enableAnimations: false });
  tauriMocks.refreshProviders.mockResolvedValue(undefined);
  tauriMocks.openProviderDashboard.mockResolvedValue(undefined);
  tauriMocks.openProviderStatusPage.mockResolvedValue(undefined);
  tauriMocks.triggerProviderLogin.mockResolvedValue(undefined);
  tauriMocks.getApiKeys.mockResolvedValue([]);
  tauriMocks.getManualCookies.mockResolvedValue([]);
  tauriMocks.getTokenAccounts.mockResolvedValue({
    providerId: "claude",
    support: { providerId: "claude", displayName: "Claude", title: "", subtitle: "", placeholder: "" },
    accounts: [],
    activeIndex: -1,
  });
  tauriMocks.getGeminiCliSignedIn.mockResolvedValue({ signedIn: false, credentialsPath: null });
  tauriMocks.importCookieFile.mockResolvedValue([]);
  tauriMocks.openProviderLogin.mockResolvedValue({ providerId: "claude", provider: "Claude", url: "https://claude.ai/" });
  tauriMocks.captureProviderLogin.mockResolvedValue([]);
  tauriMocks.closeProviderLogin.mockResolvedValue(undefined);
  tauriMocks.setManualCookie.mockResolvedValue([]);
  tauriMocks.removeManualCookie.mockResolvedValue([]);
  tauriMocks.setProviderCookieSource.mockResolvedValue(undefined);
  tauriMocks.setApiKey.mockResolvedValue([]);
  tauriMocks.removeApiKey.mockResolvedValue([]);
  tauriMocks.revokeProviderCredentials.mockResolvedValue(undefined);

  it("tray page uses HTML groups and preview, not old dashboard chrome", () => {
    render(
      <TrayPanelPage settings={snapshot} set={() => {}} saving={false} />,
    );
    expect(screen.getByText("卡片内容")).toBeInTheDocument();
    expect(screen.getByText("通知区与网格")).toBeInTheDocument();
    expect(screen.getByText("显示密度")).toBeInTheDocument();
    expect(screen.getByText("实时预览")).toBeInTheDocument();
    expect(screen.getAllByText("Claude").length).toBeGreaterThan(0);
    expect(document.querySelector(".tray-panel-reveal")).not.toBeNull();
    expect(document.querySelector(".tray-panel")).not.toBeNull();
    expect(document.querySelector(".provider-grid")).not.toBeNull();
    expect(document.querySelector(".flyout-footer")).not.toBeNull();
    expect(document.querySelector(".menu-card")).toBeNull();
    expect(document.querySelector(".settings-surf-page")).toBeNull();
  });

  it("providers page uses HTML list-detail and three login methods", () => {
    render(
      <ProvidersPage settings={snapshot} set={() => {}} saving={false} />,
    );
    expect(screen.getByText("批量导入网页会话")).toBeInTheDocument();
    expect(screen.getByText("怎么登录")).toBeInTheDocument();
    expect(screen.getByText("打开网页登录")).toBeInTheDocument();
    expect(screen.queryByText("打开登录")).not.toBeInTheDocument();
    fireEvent.click(screen.getByRole("radio", { name: "密钥" }));
    expect(screen.getByText("这个服务用密钥。填一次并保存，之后会自动刷新用量。")).toBeInTheDocument();
    expect(document.querySelector(".provider-detail")).toBeNull();
    expect(document.querySelector(".identity-section")).toBeNull();
  });

    it("does not claim a saved API key when none is stored", async () => {
      render(
        <ProvidersPage settings={snapshot} set={() => {}} saving={false} />,
      );
      fireEvent.click(screen.getByRole("radio", { name: "密钥" }));
      expect(screen.queryByText("密钥已保存")).not.toBeInTheDocument();
      expect(await screen.findByText("还没有保存密钥。")).toBeInTheDocument();
    });

  it("lists the full catalog with brand icons, not letter marks", () => {
    render(
      <ProvidersPage settings={snapshot} set={() => {}} saving={false} />,
    );
    const rows = document.querySelectorAll(".s5-prow");
    expect(rows.length).toBe(TEST_PROVIDER_CATALOG.length);
    expect(screen.getByText("Factory")).toBeInTheDocument();
    expect(screen.getByText("Grok")).toBeInTheDocument();
    expect(screen.getByText("OpenCode")).toBeInTheDocument();
    expect(screen.getByText("Azure OpenAI")).toBeInTheDocument();
    expect(document.querySelectorAll(".s5-prow .provider-icon--svg").length).toBe(
      TEST_PROVIDER_CATALOG.length,
    );
    expect(document.querySelector(".s5-prow-icon")?.textContent).not.toMatch(
      /^[A-Z]{1,2}$/,
    );
    const mark = document.querySelector(".s5-prow-icon .provider-icon") as HTMLElement;
    expect(mark.style.width).toBe("28px");
    expect(mark.style.height).toBe("28px");
  });

  it("wires the original provider header actions with HTML stroke icons", async () => {
    render(
      <ProvidersPage settings={snapshot} set={() => {}} saving={false} />,
    );
    expect(await screen.findByRole("button", { name: "刷新" })).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "切换账号" })).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "用量页" })).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "状态" })).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "买额度" })).toBeInTheDocument();
    expect(document.querySelectorAll(".s5-pd-act svg").length).toBeGreaterThanOrEqual(5);

    fireEvent.click(screen.getByRole("button", { name: "刷新" }));
    await waitFor(() => {
      expect(tauriMocks.refreshProviders).toHaveBeenCalled();
    });
    fireEvent.click(screen.getByRole("button", { name: "用量页" }));
    await waitFor(() => {
      expect(tauriMocks.openProviderDashboard).toHaveBeenCalledWith("claude");
    });
    fireEvent.click(screen.getByRole("button", { name: "状态" }));
    await waitFor(() => {
      expect(tauriMocks.openProviderStatusPage).toHaveBeenCalledWith("claude");
    });
    expect(document.querySelector(".s5-pd-act")?.textContent).toMatch(/刷新/);
  });

  it("renders live quota from getProviderDetail instead of the HTML fixture percents", async () => {
    render(
      <ProvidersPage settings={snapshot} set={() => {}} saving={false} />,
    );
    expect(await screen.findByText("41%")).toBeInTheDocument();
    expect(screen.queryByText("62%")).not.toBeInTheDocument();
    expect(screen.queryByText("48.2 t/s")).not.toBeInTheDocument();
  });

  it("follows the tray panel used/remaining choice for list and detail quota", async () => {
    render(
      <ProvidersPage
        settings={{ ...snapshot, dashboardShowAsUsed: false }}
        set={() => {}}
        saving={false}
      />,
    );
    expect(await screen.findAllByText("59%")).not.toHaveLength(0);
    expect(screen.getByText(/剩余/)).toBeInTheDocument();
    expect(screen.queryByText("41%")).not.toBeInTheDocument();
  });

  it("shows the HTML period and chart-type options without the old Token 用量 chrome", async () => {
    render(
      <ProvidersPage settings={snapshot} set={() => {}} saving={false} />,
    );
    expect(await screen.findByRole("radio", { name: "7 天" })).toBeInTheDocument();
    expect(screen.getByRole("radio", { name: "30 天" })).toBeInTheDocument();
    expect(screen.getByRole("radio", { name: "季度" })).toBeInTheDocument();
    expect(screen.getByRole("radio", { name: "年" })).toBeInTheDocument();
    expect(screen.queryByText("Token 用量")).not.toBeInTheDocument();
    expect(screen.queryByRole("radio", { name: "费用" })).not.toBeInTheDocument();
    expect(screen.queryByRole("radio", { name: "积分" })).not.toBeInTheDocument();
    expect(screen.queryByRole("radio", { name: "构成" })).not.toBeInTheDocument();
  });

  it("adds tray-html usage approx and API equivalent, without extra stat tabs", async () => {
    tauriMocks.getProviderChartData.mockResolvedValue({
      providerId: "claude",
      costHistory: [
        { date: "2026-08-10", value: 0.04 },
        { date: "2026-08-11", value: 0.06 },
      ],
      creditsHistory: [],
      usageBreakdown: [],
      localUsage: {
        todayCost: 0.04,
        todayTokens: 6000,
        sevenDayCost: 0.28,
        sevenDayTokens: 42000,
        thirtyDayCost: 1.12,
        thirtyDayTokens: 180000,
        todayTopModel: "claude-3-7-sonnet",
        sevenDayTopModel: "claude-3-7-sonnet",
        thirtyDayTopModel: "claude-3-7-sonnet",
        estimateNote: "",
      },
    });
    render(
      <ProvidersPage settings={snapshot} set={() => {}} saving={false} />,
    );
    expect(await screen.findByText("近 7 天使用")).toBeInTheDocument();
    expect(screen.getByText("≈ 4.2万")).toBeInTheDocument();
    expect(
      screen.getByText("等额 API 价值 ≈ $0.28 · ¥2.02 · claude-3-7-sonnet"),
    ).toBeInTheDocument();
    expect(screen.queryByRole("radio", { name: "速度" })).not.toBeInTheDocument();
    expect(screen.queryByRole("radio", { name: "费用" })).not.toBeInTheDocument();
    expect(screen.queryByRole("radio", { name: "积分" })).not.toBeInTheDocument();
    expect(screen.queryByRole("radio", { name: "构成" })).not.toBeInTheDocument();
  });

  it("lists a relative update time without 已登录 or 更新 after minutes", () => {
    render(
      <ProvidersPage settings={snapshot} set={() => {}} saving={false} />,
    );
    expect(screen.queryByText(/已登录/)).not.toBeInTheDocument();
    expect(screen.getByText("刚刚更新")).toBeInTheDocument();
  });

  it("buildProviderCatalog keeps every catalog id", () => {
    const rows = buildProviderCatalog();
    expect(rows.map((row) => row.id)).toEqual(
      TEST_PROVIDER_CATALOG.map(([id]) => id),
    );
  });
});
