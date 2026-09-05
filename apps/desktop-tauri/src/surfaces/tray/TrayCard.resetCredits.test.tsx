import { beforeEach, describe, expect, it, vi } from "vitest";
import { fireEvent, render, waitFor } from "@testing-library/react";
import { fromBridge } from "../../core";

const tauriMocks = vi.hoisted(() => ({
  getLocaleStrings: vi.fn(),
  openProviderDashboard: vi.fn(),
  openProviderStatusPage: vi.fn(),
  invokeSurfaceAction: vi.fn(async () => "ok"),
}));

vi.mock("../../lib/tauri", () => tauriMocks);

import { LocaleProvider } from "../../i18n/LocaleProvider";
import { buildBundle } from "../../test/localeHarness";
import TrayCard from "./TrayCard";
import type { ProviderSnapshot } from "../../core";
import type { ProviderUsageSnapshot, RateWindowSnapshot } from "../../types/bridge";

function rw(
  usedPercent: number,
  windowMinutes: number | null,
  kind: string | null,
  resetsAt: string | null = null,
  resetDescription: string | null = null,
): RateWindowSnapshot {
  return {
    usedPercent,
    remainingPercent: 100 - usedPercent,
    kind: kind as any,
    windowMinutes,
    resetsAt,
    resetDescription,
    isExhausted: false,
    reservePercent: null,
    reserveDescription: null,
  };
}

function codexBridgeWithResetCredits(): ProviderUsageSnapshot {
  return {
    providerId: "codex",
    displayName: "Codex",
    primary: rw(38, 10080, "weekly", "2026-09-10T12:00:00Z"),
    primaryLabel: "weekly",
    secondary: null,
    modelSpecific: null,
    tertiary: null,
    extraRateWindows: [
      {
        id: "reset-credits",
        title: "Reset credits",
        window: {
          ...rw(0, null, null, "2026-09-20T23:41:01Z", "2 reset credits available"),
          isInformational: true,
        },
        usageKnown: false,
        inventoryExpiresAt: ["2026-09-20T23:41:01Z", "2026-10-04T05:07:36Z"],
      },
    ],
    cost: null,
    planName: "Pro",
    accountEmail: null,
    sourceLabel: "cli",
    updatedAt: "2026-09-04T12:00:00Z",
    error: null,
    pace: null,
    accountOrganization: null,
    trayStatusLabel: null,
    fetchDurationMs: null,
  };
}

const DISPLAY = {
  showAsUsed: true,
  resetTimeRelative: true,
  highUsageThreshold: 75,
  criticalUsageThreshold: 95,
} as const;

function renderCard(
  provider: ProviderSnapshot | ProviderUsageSnapshot,
  density: "detailed" | "compact" | "minimal" = "detailed",
  chartData?: any,
) {
  return render(
    <LocaleProvider>
      <TrayCard
        provider={provider}
        densityMode={density}
        display={DISPLAY}
        showProviderIcon
        chartData={chartData}
      />
    </LocaleProvider>,
  );
}

describe("TrayCard reset credits companion line", () => {
  beforeEach(() => {
    vi.clearAllMocks();
    tauriMocks.getLocaleStrings.mockResolvedValue(
      buildBundle(
        {
          PanelResetCreditsTitle: "额外重置次数",
          PanelResetCreditsRemaining: "剩余",
          PanelResetCreditsUnit: "次",
          QuotaResetUnknown: "重置时间未知",
          QuotaResetExpiredWaiting: "重置中…",
          PanelSevenDayUsage: "7天使用量",
          PanelApiEquivalentValue: "等额 API 价值",
          TaskbarWidgetPreviewSpeed: "输出速度",
          OutputSpeedGenerating: "生成中…",
        },
        "chinese",
      ),
    );
    tauriMocks.openProviderDashboard.mockResolvedValue(undefined);
    tauriMocks.openProviderStatusPage.mockResolvedValue(undefined);
  });

  it("renders the .quota-inventory-line in detailed mode with icon, count badge, and expiry", async () => {
    const bridge = codexBridgeWithResetCredits();
    const core = fromBridge(bridge);
    const { container } = renderCard(core, "detailed");

    await waitFor(() => {
      if (!container.querySelector(".tray-card")) throw new Error("card not rendered");
    });

    const line = container.querySelector(".quota-inventory-line");
    expect(line).not.toBeNull();

    const icon = line?.querySelector(".quota-inventory-line__icon svg");
    expect(icon).not.toBeNull();

    const badge = line?.querySelector(".quota-inventory-line__badge");
    expect(badge?.textContent).toContain("剩余");
    expect(badge?.textContent).toContain("2");
    expect(badge?.textContent).toContain("次");

    const expire = line?.querySelector(".quota-inventory-line__expire");
    expect(expire?.textContent).toMatch(/到期/);
  });

  it("renders the .quota-inventory-line in compact mode", async () => {
    const bridge = codexBridgeWithResetCredits();
    const core = fromBridge(bridge);
    const { container } = renderCard(core, "compact");

    await waitFor(() => {
      if (!container.querySelector(".tray-card")) throw new Error("card not rendered");
    });

    const line = container.querySelector(".quota-inventory-line");
    expect(line).not.toBeNull();
    const badge = line?.querySelector(".quota-inventory-line__badge");
    expect(badge?.textContent).toContain("2");
  });

  it("renders the reset credits chip in minimal mode", async () => {
    const bridge = codexBridgeWithResetCredits();
    const core = fromBridge(bridge);
    const { container } = renderCard(core, "minimal");

    await waitFor(() => {
      if (!container.querySelector(".tray-card")) throw new Error("card not rendered");
    });

    const badge = container.querySelector(".minimal-streamlined__badges .soft-badge--neutral");
    expect(badge?.textContent).toContain("2次重置");
  });

  it("displays all credits and reset times in a tooltip when hovering over .quota-inventory-line", async () => {
    const bridge = codexBridgeWithResetCredits();
    const core = fromBridge(bridge);
    const { container } = renderCard(core, "detailed");

    await waitFor(() => {
      if (!container.querySelector(".tray-card")) throw new Error("card not rendered");
    });

    const line = container.querySelector(".quota-inventory-line");
    expect(line).not.toBeNull();

    fireEvent.mouseEnter(line!);
    const tip = document.body.querySelector(".tray-tip");
    expect(tip).not.toBeNull();
    expect(tip?.textContent).toContain("额外重置次数 (剩余 2 次)");
    expect(tip?.textContent).toContain("第 1 次");
    expect(tip?.textContent).toContain("第 2 次");
    expect(tip?.textContent).toMatch(/到期/);

    fireEvent.mouseLeave(line!);
    expect(document.body.querySelector(".tray-tip")).toBeNull();
  });

  it("omits the .quota-inventory-line when provider has no reset credits", async () => {
    const bridge = codexBridgeWithResetCredits();
    bridge.extraRateWindows = [];
    const core = fromBridge(bridge);
    const { container } = renderCard(core, "detailed");

    await waitFor(() => {
      if (!container.querySelector(".tray-card")) throw new Error("card not rendered");
    });

    const line = container.querySelector(".quota-inventory-line");
    expect(line).toBeNull();
  });

  it("renders a tooltip for the API equivalent value in meta-well__note when overlong", async () => {
    const bridge = codexBridgeWithResetCredits();
    bridge.capabilities = {
      outputSpeed: false,
      providerDashboard: false,
      statusPage: false,
      login: false,
      ...bridge.capabilities,
      localUsage: true,
    };
    const core = fromBridge(bridge);
    const chartData = {
      localUsage: {
        sevenDayTokens: 140000000,
        sevenDayCost: 239.64,
        sevenDayTopModel: "grok-4.0-super-long-model-name-overflowing",
      },
    } as any;
    const { container } = renderCard(core, "detailed", chartData);

    await waitFor(() => {
      if (!container.querySelector(".tray-card")) throw new Error("card not rendered");
    });

    const note = container.querySelector(".meta-well__note");
    expect(note).not.toBeNull();
    expect(note?.textContent).toContain("grok-4.0");

    // Mock scrollWidth > clientWidth to simulate text overflow
    Object.defineProperty(note, "scrollWidth", { configurable: true, value: 380 });
    Object.defineProperty(note, "clientWidth", { configurable: true, value: 260 });

    fireEvent.mouseEnter(note!);
    const tip = document.body.querySelector(".tray-tip");
    expect(tip).not.toBeNull();
    expect(tip?.textContent).toContain("$239.64");
    expect(tip?.textContent).toContain("grok-4.0-super-long-model-name-overflowing");

    fireEvent.mouseLeave(note!);
    expect(document.body.querySelector(".tray-tip")).toBeNull();
  });
});
