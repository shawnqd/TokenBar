import { fireEvent, render, screen, waitFor } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { TEST_PROVIDER_CATALOG } from "../../../test/providerCatalog";
import type { SettingsSnapshot } from "../../../types/bridge";
import { createUsageStore } from "../../../core/usageStore";
import { fromBridge } from "../../../core/fromBridge";
import { bridgeRateWindow, bridgeSnapshot } from "../../../core/fixtures";
import ProvidersPage from "./ProvidersPage";
import TrayPanelPage from "./TrayPanelPage";
import { buildProviderCatalog } from "./htmlFixture";

const tauriMocks = vi.hoisted(() => {
  const state: { usageSource?: string } = { usageSource: undefined };
  return {
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
  getProviderCookieSource: vi.fn(),
  getProviderCookieSourceOptions: vi.fn(),
  getProviderAuthCapabilities: vi.fn(),
  getTokenAccountProviders: vi.fn(),
  onProviderUpdated: vi.fn(),
  setApiKey: vi.fn(),
  removeApiKey: vi.fn(),
  state,
  invokeSurfaceAction: vi.fn(async (action?: { type?: string; source?: string }) => {
    if (action && action.type === "setUsageSource") {
      state.usageSource = action.source;
    }
    return "ok";
  }),
  };
});

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
  getProviderCookieSource: tauriMocks.getProviderCookieSource,
  getProviderCookieSourceOptions: tauriMocks.getProviderCookieSourceOptions,
  getProviderAuthCapabilities: tauriMocks.getProviderAuthCapabilities,
  getTokenAccountProviders: tauriMocks.getTokenAccountProviders,
  onProviderUpdated: tauriMocks.onProviderUpdated,
  setApiKey: tauriMocks.setApiKey,
  removeApiKey: tauriMocks.removeApiKey,
  getProviderRegionOptions: vi.fn().mockResolvedValue([]),
  getProviderRegion: vi.fn().mockResolvedValue(null),
  setProviderRegion: vi.fn(),
  getSafeDiagnostics: vi.fn().mockResolvedValue({
    appVersion: "0.0.0-test",
    platform: "test",
    schemaVersion: 1,
    enabledProviders: [],
    providerCookieSources: {},
    hasManualCookies: [],
    hasApiKeys: [],
    hidePersonalInfo: false,
    refreshIntervalSecs: 300,
  }),
  resetSettings: vi.fn(),
  invokeSurfaceAction: tauriMocks.invokeSurfaceAction,
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

// The runtime catalog shape the page consumes exclusively (bootstrap
// payload). TEST_PROVIDER_CATALOG stays a test fixture and is only used to
// build this runtime-shaped input.
const runtimeCatalog = TEST_PROVIDER_CATALOG.map(([id, displayName]) => ({
  id,
  displayName,
  cookieDomain: id === "claude" ? "claude.ai" : null,
}));

function claudeLiveStore() {
  const store = createUsageStore();
  store.upsert(
    fromBridge(
      bridgeSnapshot({
        providerId: "claude",
        displayName: "Claude",
        updatedAt: new Date().toISOString(),
        primary: bridgeRateWindow({
          kind: "session",
          windowMinutes: 300,
          usedPercent: 41,
          remainingPercent: 59,
          resetDescription: "3h",
        }),
      }),
    ),
  );
  return store;
}

describe("V5 settings pages from HTML", () => {
  beforeEach(() => {
    tauriMocks.state.usageSource = undefined;
  });

  tauriMocks.getProviderDetail.mockImplementation((providerId: string) =>
    Promise.resolve({
      usageSource: tauriMocks.state.usageSource,
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
    loginFlow: "claude_cli",
  }));
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
  tauriMocks.getProviderCookieSource.mockResolvedValue("manual");
  // 后端选项目录使用全称标签；codex 没有 Cookie 来源目录（真实口径）。
  tauriMocks.getProviderCookieSourceOptions.mockImplementation((providerId: string) =>
    Promise.resolve(
      providerId === "codex"
        ? []
        : [
            {
              value: "auto",
              label: "自动读取浏览器",
              description: "自动读取浏览器里已登录的会话。",
            },
            {
              value: "manual",
              label: "使用已保存 Cookie",
              description: "只使用你在应用里保存或导入的 Cookie（默认）。",
            },
          ],
    ),
  );
  // caps 对齐真实目录：密钥型仅 requires_api_key 的服务商（DeepSeek）；
  // Grok/Claude 的用量抓取不读粘贴密钥，绝不支持 API 密钥读取方式。
  tauriMocks.getProviderAuthCapabilities.mockImplementation((providerId: string) =>
    Promise.resolve({
      supportsOAuth: false,
      supportsCli: false,
      supportsWeb: false,
      supportsApiKey: providerId === "deepseek",
      hasCookieDomain: false,
      loginFlow: null,
      availableSources: ["auto"],
      ...(providerId === "claude"
        ? {
            supportsWeb: true,
            hasCookieDomain: true,
            loginFlow: "claude_cli",
            availableSources: ["auto", "oauth", "web", "cli"],
          }
        : {}),
      ...(providerId === "codex"
        ? { supportsCli: true, loginFlow: "codex_cli", availableSources: ["auto", "oauth", "cli"] }
        : {}),
      ...(providerId === "grok"
        ? {
            supportsWeb: true,
            hasCookieDomain: true,
            availableSources: ["auto", "cli", "oauth", "web"],
          }
        : {}),
      ...(providerId === "deepseek"
        ? { supportsApiKey: true, availableSources: ["auto", "oauth"] }
        : {}),
    }),
  );
  tauriMocks.setApiKey.mockResolvedValue([]);
  tauriMocks.removeApiKey.mockResolvedValue([]);
  tauriMocks.getTokenAccountProviders.mockResolvedValue([]);
  tauriMocks.onProviderUpdated.mockResolvedValue(() => {});
  tauriMocks.revokeProviderCredentials.mockResolvedValue(undefined);

  it("tray page uses HTML groups and preview, not old dashboard chrome", () => {
    render(
      <TrayPanelPage settings={snapshot} set={() => {}} saving={false} />,
    );
    expect(screen.getByText("显示内容")).toBeInTheDocument();
    expect(screen.getByText("打开与账号")).toBeInTheDocument();
    expect(screen.getByText("通知区与布局")).toBeInTheDocument();
    expect(screen.getByText("显示密度")).toBeInTheDocument();
    expect(screen.getByText("设置打开时保留托盘")).toBeInTheDocument();
    expect(screen.getByText("实时预览")).toBeInTheDocument();
    expect(screen.getAllByText("Claude").length).toBeGreaterThan(0);
    expect(document.querySelector(".s5-tray-flyout")).not.toBeNull();
    expect(document.querySelector(".tray-panel")).not.toBeNull();
    expect(document.querySelector(".provider-grid")).not.toBeNull();
    expect(document.querySelector(".flyout-footer")).toBeNull();
    expect(document.querySelector(".menu-card")).toBeNull();
    expect(document.querySelector(".settings-surf-page")).toBeNull();
  });

  it("tray page scales s5-tray-flyout proportionally without distorting inner tray-panel", () => {
    const { unmount } = render(
      <TrayPanelPage
        settings={{ ...snapshot, trayScalePercent: 120 }}
        set={() => {}}
        saving={false}
      />,
    );
    const flyout = document.querySelector<HTMLElement>(".s5-tray-flyout");
    expect(flyout).not.toBeNull();
    expect(flyout?.style.transform).toBe("");
    expect(flyout?.style.zoom).toBe("1.02");
    const panel = document.querySelector<HTMLElement>(".tray-panel");
    expect(panel).not.toBeNull();
    expect(panel?.getAttribute("style")).toBeNull();
    unmount();

    const { unmount: unmountDefault } = render(
      <TrayPanelPage
        settings={{ ...snapshot, trayScalePercent: 100 }}
        set={() => {}}
        saving={false}
      />,
    );
    const defaultFlyout = document.querySelector<HTMLElement>(".s5-tray-flyout");
    expect(defaultFlyout?.style.zoom).toBe("0.85");
    unmountDefault();
  });

  it("providers page uses real auth capabilities with no generic WebView login", async () => {
    render(
      <ProvidersPage settings={snapshot} set={() => {}} saving={false} catalog={runtimeCatalog} />,
    );
    expect(screen.getByText("批量导入浏览器 Cookie")).toBeInTheDocument();
    // L1 用量读取方式来自异步能力查询，等它渲染后再断言。
    expect(await screen.findByText("用量读取方式")).toBeInTheDocument();
    // 自动选择：服务商自动决定通道，下方不渲染任何管理项。
    expect(screen.queryByPlaceholderText(/粘贴 Cookie 请求头/)).not.toBeInTheDocument();
    // 切到 Cookies：默认使用已保存 Cookie，出现 Cookie 来源子下拉与管理区。
    fireEvent.click(await screen.findByRole("button", { name: "自动选择" }));
    fireEvent.click(await screen.findByRole("option", { name: "Cookies" }));
    expect(await screen.findByText("Cookie 来源")).toBeInTheDocument();
    expect(
      await screen.findByText(/默认使用你在这里保存或导入的/),
    ).toBeInTheDocument();
    expect(await screen.findByPlaceholderText(/粘贴 Cookie 请求头/)).toBeInTheDocument();
    expect(screen.queryByText("打开网页登录")).not.toBeInTheDocument();
    expect(screen.queryByText("我已登录，捕获")).not.toBeInTheDocument();
    expect(screen.queryByText("打开登录")).not.toBeInTheDocument();
    expect(document.querySelector(".provider-detail")).toBeNull();
    expect(document.querySelector(".identity-section")).toBeNull();
  });

    it("does not claim a saved API key when none is stored", async () => {
      render(
        <ProvidersPage settings={snapshot} set={() => {}} saving={false} catalog={runtimeCatalog} />,
      );
      // 密钥型服务商（DeepSeek）：L1 切 API 密钥后显示密钥表单，未保存时
      // 不得声称已保存。
      fireEvent.click(await screen.findByText("DeepSeek"));
      fireEvent.click(await screen.findByRole("button", { name: "自动选择" }));
      fireEvent.click(await screen.findByRole("option", { name: "API 密钥" }));
      expect(screen.queryByText("API 密钥已保存")).not.toBeInTheDocument();
      expect(await screen.findByText("还没有保存 API 密钥。")).toBeInTheDocument();
    });

  it("lists the full catalog with brand icons, not letter marks", () => {
    render(
      <ProvidersPage settings={snapshot} set={() => {}} saving={false} catalog={runtimeCatalog} />,
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
      <ProvidersPage settings={snapshot} set={() => {}} saving={false} catalog={runtimeCatalog} />,
    );
    expect(await screen.findByRole("button", { name: "刷新" })).toBeInTheDocument();
    // 运行时无多账号切换：动作栏不得出现「切换账号」（规范 v2.2.2）。
    expect(screen.queryByRole("button", { name: "切换账号" })).not.toBeInTheDocument();
    expect(screen.getByRole("button", { name: "用量页" })).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "状态" })).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "买额度" })).toBeInTheDocument();
    expect(document.querySelectorAll(".s5-pd-act svg").length).toBeGreaterThanOrEqual(4);

    fireEvent.click(screen.getByRole("button", { name: "刷新" }));
    await waitFor(() => {
      expect(tauriMocks.invokeSurfaceAction).toHaveBeenCalledWith({
        type: "refresh",
        target: { kind: "provider", providerId: "claude" },
        force: true,
      });
    });
    fireEvent.click(screen.getByRole("button", { name: "用量页" }));
    await waitFor(() => {
      expect(tauriMocks.invokeSurfaceAction).toHaveBeenCalledWith({
        type: "openExternalUsage",
        target: { kind: "provider", providerId: "claude" },
      });
    });
    fireEvent.click(screen.getByRole("button", { name: "状态" }));
    await waitFor(() => {
      expect(tauriMocks.invokeSurfaceAction).toHaveBeenCalledWith({
        type: "openExternalStatus",
        target: { kind: "provider", providerId: "claude" },
      });
    });
    fireEvent.click(screen.getByRole("button", { name: "买额度" }));
    await waitFor(() => {
      expect(tauriMocks.invokeSurfaceAction).toHaveBeenCalledWith({
        type: "openExternalUrl",
        target: { kind: "app" },
        url: "https://claude.ai/upgrade",
      });
    });
    expect(document.querySelector(".s5-pd-act")?.textContent).toMatch(/刷新/);
  });

  it("renders live quota from getProviderDetail instead of the HTML fixture percents", async () => {
    render(
      <ProvidersPage settings={snapshot} set={() => {}} saving={false} catalog={runtimeCatalog} coreStore={claudeLiveStore()} />,
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
        catalog={runtimeCatalog}
        coreStore={claudeLiveStore()}
      />,
    );
    expect(await screen.findAllByText("59%")).not.toHaveLength(0);
    expect(screen.getByText(/剩余/)).toBeInTheDocument();
    expect(screen.queryByText("41%")).not.toBeInTheDocument();
  });

  it("shows the HTML period and chart-type options without the old Token 用量 chrome", async () => {
    render(
      <ProvidersPage settings={snapshot} set={() => {}} saving={false} catalog={runtimeCatalog} />,
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
      <ProvidersPage settings={snapshot} set={() => {}} saving={false} catalog={runtimeCatalog} />,
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

  it("lists healthy rows with the spec vocabulary 已登录 · 相对时间", () => {
    render(
      <ProvidersPage settings={snapshot} set={() => {}} saving={false} catalog={runtimeCatalog} coreStore={claudeLiveStore()} />,
    );
    // Spec §6.2: healthy list rows read 「已登录 · 时间」; a bare relative time
    // without the status word is the drift the spec removed.
    expect(screen.getByText("已登录 · 刚刚更新")).toBeInTheDocument();
  });

  it("buildProviderCatalog maps the runtime catalog 1:1 and never falls back to a static list", () => {
    const rows = buildProviderCatalog(runtimeCatalog);
    expect(rows.map((row) => row.id)).toEqual(
      TEST_PROVIDER_CATALOG.map(([id]) => id),
    );
    // Empty or failed bootstrap payload: honest empty result — no
    // test-catalog fallback in production.
    expect(buildProviderCatalog(undefined)).toEqual([]);
    expect(buildProviderCatalog([])).toEqual([]);
    // Capabilities not resolved yet: no fabricated auth entries.
    expect(
      buildProviderCatalog(runtimeCatalog, [], [], undefined)[0].methods,
    ).toEqual([]);
  });

  it("renders the runtime usage-source dropdown and persists a validated pin", async () => {
    render(
      <ProvidersPage settings={snapshot} set={() => {}} saving={false} catalog={runtimeCatalog} />,
    );
    // Claude advertises four sources → the dropdown lists them all.
    expect(await screen.findByText("用量读取方式")).toBeInTheDocument();
    fireEvent.click(screen.getByRole("button", { name: "自动选择" }));
    fireEvent.click(await screen.findByRole("option", { name: "CLI 登录" }));
    await waitFor(() => {
      expect(tauriMocks.invokeSurfaceAction).toHaveBeenCalledWith({
        type: "setUsageSource",
        target: { kind: "provider", providerId: "claude" },
        source: "cli",
      });
    });
  });

  it("hides the paste editor under 自动读取浏览器 and shows it under manual", async () => {
    tauriMocks.getProviderCookieSource.mockResolvedValue("auto");
    render(
      <ProvidersPage settings={snapshot} set={() => {}} saving={false} catalog={runtimeCatalog} />,
    );
    // 进入 Cookies 分支（L1 自动选择下不渲染任何管理项）。
    fireEvent.click(await screen.findByRole("button", { name: "自动选择" }));
    fireEvent.click(await screen.findByRole("option", { name: "Cookies" }));
    expect(await screen.findByText(/自动读取浏览器中的 Claude Cookie/)).toBeInTheDocument();
    // 已保存会话的全部管理 UI 属于「使用已保存 Cookie」分支：自动读取时
    // 粘贴框、保存、删除、已保存徽标都不出现。
    expect(screen.queryByPlaceholderText(/粘贴 Cookie 请求头/)).not.toBeInTheDocument();
    expect(screen.queryByRole("button", { name: "删除已保存 Cookie" })).not.toBeInTheDocument();
    expect(screen.queryByText("会话 Cookie 已保存")).not.toBeInTheDocument();
    fireEvent.click(await screen.findByRole("button", { name: "自动读取浏览器" }));
    fireEvent.click(await screen.findByRole("option", { name: "使用已保存 Cookie" }));
    expect(await screen.findByPlaceholderText(/粘贴 Cookie 请求头/)).toBeInTheDocument();
    tauriMocks.getProviderCookieSource.mockResolvedValue("manual");
  });

  it("distinguishes saved vs unsaved session-cookie states", async () => {
    render(
      <ProvidersPage settings={snapshot} set={() => {}} saving={false} catalog={runtimeCatalog} />,
    );
    fireEvent.click(await screen.findByRole("button", { name: "自动选择" }));
    fireEvent.click(await screen.findByRole("option", { name: "Cookies" }));
    // 未保存：虚线描边 + 中性「未保存」徽标。
    const area = await screen.findByPlaceholderText(/粘贴 Cookie 请求头/);
    expect(area.className).toContain("is-empty");
    expect(screen.getByText("未保存")).toBeInTheDocument();
    expect(screen.queryByText("已保存")).not.toBeInTheDocument();
  });

  it("marks the saved session with the green state and solid outline", async () => {
    tauriMocks.getManualCookies.mockResolvedValue([
      { providerId: "claude", value: "session=abc" },
    ] as never);
    render(
      <ProvidersPage settings={snapshot} set={() => {}} saving={false} catalog={runtimeCatalog} />,
    );
    fireEvent.click(await screen.findByRole("button", { name: "自动选择" }));
    fireEvent.click(await screen.findByRole("option", { name: "Cookies" }));
    const area = await screen.findByPlaceholderText(/已有一份。粘贴新的会覆盖旧的/);
    expect(area.className).toContain("is-saved");
    expect(await screen.findByText("已保存")).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "删除已保存 Cookie" })).toBeInTheDocument();
    tauriMocks.getManualCookies.mockResolvedValue([]);
  });

  it("names the real login action instead of a generic 打开登录", async () => {
    render(
      <ProvidersPage settings={snapshot} set={() => {}} saving={false} catalog={runtimeCatalog} />,
    );
    // Codex only offers its CLI flow: L1 切 CLI 登录后，按钮写明实际动作，
    // 引导文案描述 CLI 动作而非系统浏览器流程。
    fireEvent.click(await screen.findByText("Codex"));
    fireEvent.click(await screen.findByRole("button", { name: "自动选择" }));
    fireEvent.click(await screen.findByRole("option", { name: "CLI 登录" }));
    expect(await screen.findByRole("button", { name: "运行 Codex 登录" })).toBeInTheDocument();
    expect(screen.getByText("通过 Codex CLI 或 OAuth 完成登录。本页不需要粘贴密钥或 Cookie。")).toBeInTheDocument();
    expect(screen.queryByText("打开登录")).not.toBeInTheDocument();
  });

  it("cascades the card body from the pinned usage source", async () => {
    // L1 钉定 本机 CLI：下方只剩 CLI 登录块，不再显示认证方式分段。
    const base = await tauriMocks.getProviderDetail.getMockImplementation()?.call(null) ??
      (tauriMocks.getProviderDetail as unknown as { _mockDefault?: unknown })._mockDefault;
    void base;
    const previous = tauriMocks.getProviderDetail.getMockImplementation();
    tauriMocks.getProviderDetail.mockImplementation((providerId: string) =>
      Promise.resolve({
        id: providerId,
        displayName: providerId,
        enabled: true,
        email: null,
        plan: null,
        usageSource: "cli",
        hasSnapshot: false,
      }),
    );
    render(
      <ProvidersPage settings={snapshot} set={() => {}} saving={false} catalog={runtimeCatalog} />,
    );
    fireEvent.click(await screen.findByText("Codex"));
    expect(await screen.findByText("通过 Codex CLI 或 OAuth 完成登录。本页不需要粘贴密钥或 Cookie。")).toBeInTheDocument();
    expect(screen.queryByText("选一种就行")).not.toBeInTheDocument();
    expect(screen.queryByPlaceholderText(/粘贴 Cookie 请求头/)).not.toBeInTheDocument();
    if (previous) tauriMocks.getProviderDetail.mockImplementation(previous);
  });

  it("keeps the usage-source dropdown for single-source providers", async () => {
    render(
      <ProvidersPage settings={snapshot} set={() => {}} saving={false} catalog={runtimeCatalog} />,
    );
    // 用户决策：不管有几种方式，所有服务商都有「用量读取方式」下拉（默认 自动选择）。
    fireEvent.click(await screen.findByText("Grok"));
    expect(await screen.findByText("用量读取方式")).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "自动选择" })).toBeInTheDocument();
  });

  it("shows an honest empty state when the runtime catalog is missing", () => {
    render(
      <ProvidersPage settings={snapshot} set={() => {}} saving={false} catalog={[]} />,
    );
    expect(screen.getByText(/服务商目录尚未加载/)).toBeInTheDocument();
    expect(document.querySelector(".s5-prow")).toBeNull();
  });
});
