import { beforeEach, describe, expect, it, vi } from "vitest";
import { fireEvent, render, waitFor } from "@testing-library/react";

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

const DISPLAY = {
  showAsUsed: true,
  resetTimeRelative: true,
  highUsageThreshold: 75,
  criticalUsageThreshold: 95,
} as const;

function renderCard(
  provider: ProviderUsageSnapshot,
  densityMode: "detailed" | "compact" | "minimal" = "detailed",
) {
  return render(
    <LocaleProvider>
      <TrayCard provider={provider} display={{ ...DISPLAY }} densityMode={densityMode} />
    </LocaleProvider>,
  );
}

describe("TrayCard Archetype Template Alignment", () => {
  beforeEach(() => {
    tauriMocks.getLocaleStrings.mockResolvedValue(
      buildBundle(
        {
          ProviderSessionLabel: "5 小时额度",
          ProviderWeeklyLabel: "周额度",
          ProviderMonthlyLabel: "月额度",
          QuotaPaceOnPace: "按节奏",
          QuotaPaceInReserve: "结余",
          QuotaPaceInDeficit: "透支",
          PanelExpected: "预计",
          ResetsInDaysHours: "{}天{}小时后",
          ResetsInHoursMinutes: "{}小时{}分钟后",
          QuotaResetExpiredWaiting: "重置中…",
          QuotaResetUnknown: "重置时间未知",
        },
        "chinese",
      ),
    );
    tauriMocks.openProviderDashboard.mockResolvedValue(undefined);
    tauriMocks.openProviderStatusPage.mockResolvedValue(undefined);
  });

  describe("Archetype 2A: Pure Balance (DeepSeek)", () => {
    const deepseekSnapshot: ProviderUsageSnapshot = {
      providerId: "deepseek",
      displayName: "DeepSeek",
      primary: rw(0, null, null, null, "¥69.21 (Paid: ¥69.21 / Granted: ¥0.00)"),
      primaryLabel: undefined,
      secondary: null,
      secondaryLabel: undefined,
      modelSpecific: null,
      tertiary: null,
      extraRateWindows: [],
      cost: null,
      planName: "CNY balance: ¥69.21",
      accountEmail: "user@deepseek.com",
      sourceLabel: "auto",
      updatedAt: "2026-09-05T08:00:00Z",
      error: null,
      pace: null,
      accountOrganization: null,
      trayStatusLabel: null,
      fetchDurationMs: null,
    };

    it("renders single balance line encased in dual-pill-container in Detailed mode", async () => {
      const { container } = renderCard(deepseekSnapshot, "detailed");
      await waitFor(() => {
        expect(container.querySelector(".dual-pill-container")).not.toBeNull();
      });
      expect(container.querySelector(".balance-single-line")).not.toBeNull();
      expect(container.textContent).toContain("¥69.21");
      expect(container.querySelector(".progress-bar")).toBeNull();
    });

    it("renders Scheme B layout with left label and right soft-badge capsule for balance breakdown", async () => {
      const withBreakdown: ProviderUsageSnapshot = {
        ...deepseekSnapshot,
        updatedAt: new Date().toISOString(),
      };
      const { container } = renderCard(withBreakdown, "detailed");
      await waitFor(() => {
        expect(container.querySelector(".balance-single-line")).not.toBeNull();
      });
      const left = container.querySelector(".balance-single-line__left");
      expect(left?.textContent).toContain("账户余额");
      expect(left?.textContent).toContain("¥69.21");
      const rightBadge = container.querySelector(".balance-single-line__right .soft-badge--reserve");
      expect(rightBadge).not.toBeNull();
      expect(rightBadge?.textContent).toBe("赠送 ¥0.00");
    });

    it("renders clean sublabel and balance metric without duplicating name in Minimal mode", async () => {
      const { container } = renderCard(deepseekSnapshot, "minimal");
      await waitFor(() => {
        expect(container.querySelector(".minimal-streamlined")).not.toBeNull();
      });
      const title = container.querySelector(".minimal-streamlined__title");
      expect(title?.textContent).toContain("DeepSeek");
      expect(title?.textContent).not.toContain("DeepSeek DeepSeek");
      const sublabel = container.querySelector(".minimal-streamlined__sublabel");
      expect(sublabel?.textContent).toBe("· 余额");
      const metric = container.querySelector(".minimal-streamlined__metric");
      expect(metric?.textContent).toBe("¥69.21");
      expect(container.querySelector(".progress-bar")).toBeNull();
    });

    it("keeps the error icon and label in the same status wrapper", async () => {
      const { container } = renderCard(
        { ...deepseekSnapshot, error: "provider request failed" },
        "detailed",
      );
      await waitFor(() => {
        expect(container.querySelector(".card-header__updated.is-error")).not.toBeNull();
      });
      const status = container.querySelector(".card-header__updated.is-error");
      expect(status?.querySelector(".card-header__updated-icon")).not.toBeNull();
      expect(status?.querySelector(".card-header__updated-label")).not.toBeNull();
    });

    it("shows an error icon in minimal mode when no quota is available", async () => {
      const { container } = renderCard(
        { ...deepseekSnapshot, error: "provider request failed" },
        "minimal",
      );
      await waitFor(() => {
        expect(container.querySelector(".minimal-streamlined__status-badge")).not.toBeNull();
      });
      expect(container.querySelector(".minimal-streamlined__status-icon")).not.toBeNull();
    });
  });

  describe("Archetype 2B: Pure Status (Azure OpenAI)", () => {
    const azureSnapshot: ProviderUsageSnapshot = {
      providerId: "azure",
      displayName: "Azure OpenAI",
      primary: {
        ...rw(0, null, null, null, null),
        isInformational: true,
      },
      primaryLabel: undefined,
      secondary: null,
      secondaryLabel: undefined,
      modelSpecific: null,
      tertiary: null,
      extraRateWindows: [],
      cost: null,
      planName: null,
      accountEmail: null,
      sourceLabel: "api",
      updatedAt: "2026-09-05T08:00:00Z",
      error: null,
      pace: null,
      accountOrganization: null,
      trayStatusLabel: "部署运行正常 · 按量就绪",
      fetchDurationMs: null,
    };

    it("renders status telemetry card encased in dual-pill-container in Detailed mode", async () => {
      const { container } = renderCard(azureSnapshot, "detailed");
      await waitFor(() => {
        expect(container.querySelector(".dual-pill-container")).not.toBeNull();
      });
      // density-preview.html's Block D shell: the status text splits on "·"
      // into a readiness title and a soft badge.
      const card = container.querySelector(".telemetry-card");
      expect(card).not.toBeNull();
      expect(card?.querySelector(".telemetry-card__status")?.textContent).toContain("部署运行正常");
      expect(card?.querySelector(".soft-badge")?.textContent).toBe("按量就绪");
      expect(container.textContent).toContain("部署运行正常");
    });

    it("renders status indicator and metric without empty slot in Minimal mode", async () => {
      const { container } = renderCard(azureSnapshot, "minimal");
      await waitFor(() => {
        expect(container.querySelector(".minimal-streamlined")).not.toBeNull();
      });
      const sublabel = container.querySelector(".minimal-streamlined__sublabel");
      expect(sublabel?.textContent).toBe("· 状态");
      const metric = container.querySelector(".minimal-streamlined__metric");
      expect(metric?.textContent).toContain("●");
      expect(metric?.textContent).toContain("部署运行正常");
    });
  });

  describe("Archetype 3: Hybrid Dual-Track (Cursor with Fast Requests + Wallet Balance)", () => {
    const cursorSnapshot: ProviderUsageSnapshot = {
      providerId: "cursor",
      displayName: "Cursor",
      primary: rw(85, 43200, "monthly", "2026-09-15T00:00:00Z"),
      primaryLabel: "Fast Requests",
      secondary: null,
      secondaryLabel: undefined,
      modelSpecific: null,
      tertiary: null,
      extraRateWindows: [],
      cost: {
        used: 24.5,
        limit: null,
        remaining: 24.5,
        currencyCode: "USD",
        period: "monthly",
        resetsAt: "2026-09-15T00:00:00Z",
        formattedUsed: "$24.50 USD",
        formattedLimit: null,
      },
      planName: "Pro Plan",
      accountEmail: "user@cursor.com",
      sourceLabel: "auto",
      updatedAt: "2026-09-05T08:00:00Z",
      error: null,
      pace: null,
      accountOrganization: null,
      trayStatusLabel: null,
      fetchDurationMs: null,
    };

    it("renders hero quota in dual-pill-top and balance in dual-pill-bottom in Detailed mode", async () => {
      const { container } = renderCard(cursorSnapshot, "detailed");
      await waitFor(() => {
        expect(container.querySelector(".dual-pill-top")).not.toBeNull();
      });
      expect(container.querySelector(".dual-pill-bottom")).not.toBeNull();
      expect(container.querySelector(".dual-pill-top .quota-row")).not.toBeNull();
      expect(container.querySelector(".dual-pill-bottom")?.textContent).toContain("$24.50 USD");
      expect(container.querySelector(".dual-pill-bottom .balance-single-line__left")?.textContent).toContain("账户余额");
      expect(container.querySelector(".dual-pill-bottom .soft-badge")).toBeNull();
    });

    it("renders quota metric in row 1 and balance in condensedChip in Minimal mode", async () => {
      const { container } = renderCard(cursorSnapshot, "minimal");
      await waitFor(() => {
        expect(container.querySelector(".minimal-streamlined")).not.toBeNull();
      });
      const metric = container.querySelector(".minimal-streamlined__metric");
      expect(metric?.textContent).toBe("85%");
      expect(container.textContent).toContain("余额 $24.50 USD");
    });

    it("keeps the stale quota visible while surfacing an error in minimal mode", async () => {
      const { container } = renderCard(
        { ...cursorSnapshot, error: "provider request failed" },
        "minimal",
      );
      await waitFor(() => {
        expect(container.querySelector(".minimal-streamlined__status-badge")).not.toBeNull();
      });
      expect(container.querySelector(".minimal-streamlined__metric")?.textContent).toBe("85%");
      expect(container.querySelector(".minimal-streamlined__status-icon")).not.toBeNull();
    });
  });

  describe("z.ai unused one-time Start Plan plus wallet", () => {
    const zaiSnapshot: ProviderUsageSnapshot = {
      providerId: "zai",
      displayName: "z.ai",
      primary: {
        ...rw(0, null, null, null, "无生效套餐"),
        isInformational: true,
      },
      primaryLabel: undefined,
      secondary: null,
      secondaryLabel: undefined,
      modelSpecific: null,
      tertiary: null,
      extraRateWindows: [
        {
          id: "zai-zcode-0",
          title: "体验套餐 · GLM-5.3-Flash",
          usageKnown: true,
          window: rw(0, null, null, "2026-09-07T07:00:00Z", "剩余 3.00亿"),
        },
        {
          id: "zai-account-balance",
          title: "Account balance",
          usageKnown: false,
          window: {
            ...rw(0, null, null, null, "¥48.45 (Paid: ¥41.70 / Granted: ¥6.75)"),
            isInformational: true,
          },
        },
      ],
      cost: null,
      planName: "体验套餐",
      accountEmail: null,
      sourceLabel: "oauth",
      updatedAt: "2026-09-05T08:00:00Z",
      error: null,
      pace: null,
      accountOrganization: null,
      trayStatusLabel: null,
      fetchDurationMs: null,
    };

    it("shows the unused one-time grant as quota, not only the wallet", async () => {
      const { container } = renderCard(zaiSnapshot, "detailed");
      await waitFor(() => {
        expect(container.querySelector(".dual-pill-container")).not.toBeNull();
      });
      expect(container.textContent).toContain("体验套餐");
      expect(container.textContent).toContain("0%");
      expect(container.querySelector(".progress-bar")).not.toBeNull();
      expect(container.querySelector(".balance-single-line")?.textContent).toContain("¥48.45");
    });
  });

  describe("Tile hover tip stays minimal", () => {
    const weeklyWithPace: ProviderUsageSnapshot = {
      providerId: "claude",
      displayName: "Claude",
      primary: rw(82, 300, "session", "2026-09-10T13:00:00Z"),
      primaryLabel: undefined,
      secondary: rw(82, 10080, "weekly", "2026-09-12T07:00:00Z"),
      secondaryLabel: undefined,
      modelSpecific: null,
      tertiary: null,
      extraRateWindows: [],
      cost: null,
      planName: null,
      accountEmail: null,
      sourceLabel: "auto",
      updatedAt: "2026-09-10T13:00:00Z",
      error: null,
      pace: {
        stage: "slightly_ahead",
        deltaPercent: 12.4,
        willLastToReset: false,
        etaSeconds: null,
        expectedUsedPercent: 69.6,
        actualUsedPercent: 82,
        speedMultiplierToReset: null,
      },
      accountOrganization: null,
      trayStatusLabel: null,
      fetchDurationMs: null,
    };

    it("does not render any tip on unobscured elements (tile, notch, reset, unobscured label)", async () => {
      const { container } = renderCard(weeklyWithPace, "detailed");
      await waitFor(() => {
        expect(container.querySelector(".quota-tile--full")).not.toBeNull();
      });
      const tile = container.querySelector(".quota-tile--full")!;
      expect(tile.textContent).toContain("重置");

      // 1. Hovering the whole tile does NOT trigger noisy popup
      fireEvent.mouseEnter(tile);
      await new Promise((resolve) => setTimeout(resolve, 600));
      expect(document.body.querySelector(".tray-tip")).toBeNull();
      fireEvent.mouseLeave(tile);

      // 2. Hovering the progress notch does NOT trigger popup (pure visual marker)
      const notch = tile.querySelector(".progress-notch")!;
      fireEvent.mouseEnter(notch);
      await new Promise((resolve) => setTimeout(resolve, 600));
      expect(document.body.querySelector(".tray-tip")).toBeNull();
      fireEvent.mouseLeave(notch);

      // 3. Hovering the reset text does NOT trigger popup (already fully visible)
      const reset = tile.querySelector(".quota-tile__reset")!;
      fireEvent.mouseEnter(reset);
      await new Promise((resolve) => setTimeout(resolve, 600));
      expect(document.body.querySelector(".tray-tip")).toBeNull();
      fireEvent.mouseLeave(reset);

      // 4. Hovering unobscured label does NOT trigger popup
      const label = tile.querySelector(".quota-tile__label")!;
      fireEvent.mouseEnter(label);
      await new Promise((resolve) => setTimeout(resolve, 600));
      expect(document.body.querySelector(".tray-tip")).toBeNull();
      fireEvent.mouseLeave(label);
    });

    it("triggers tip only when text is truncated (scrollWidth > clientWidth)", async () => {
      const { container } = renderCard(weeklyWithPace, "detailed");
      await waitFor(() => {
        expect(container.querySelector(".quota-tile--full")).not.toBeNull();
      });
      const label = container.querySelector(".quota-tile__label")!;
      expect(label.textContent).toBe("周额度");

      // Simulate overflow truncation: scrollWidth > clientWidth
      Object.defineProperty(label, "clientWidth", { configurable: true, value: 40 });
      Object.defineProperty(label, "scrollWidth", { configurable: true, value: 100 });

      fireEvent.mouseEnter(label);
      await waitFor(
        () => {
          const tip = document.body.querySelector(".tray-tip");
          expect(tip).not.toBeNull();
          expect(tip?.textContent).toBe("周额度");
        },
        { timeout: 1000 },
      );

      fireEvent.mouseLeave(label);
      await waitFor(() => {
        expect(document.body.querySelector(".tray-tip")).toBeNull();
      });
    });

    it("renders no tip at all when the window has no pace verdict", async () => {
      const { container } = renderCard(
        {
          ...weeklyWithPace,
          secondary: null,
          pace: null,
          extraRateWindows: [
            {
              id: "untimed",
              title: "额外额度",
              usageKnown: true,
              window: rw(40, null, null, null, null),
            },
          ],
        },
        "detailed",
      );
      await waitFor(() => {
        expect(container.querySelector(".quota-tile--full")).not.toBeNull();
      });
      const tile = container.querySelector(".quota-tile--full")!;
      expect(tile.textContent).toContain("额外额度");
      fireEvent.mouseEnter(tile);
      await new Promise((resolve) => setTimeout(resolve, 600));
      expect(document.body.querySelector(".tray-tip")).toBeNull();
      fireEvent.mouseLeave(tile);
    });

    it("HeroRow reset has no tip and stays completely silent", async () => {
      const heroWithReset: ProviderUsageSnapshot = {
        ...weeklyWithPace,
        primary: rw(60, 300, "session", "2026-09-12T15:00:00Z"),
      };
      const { container } = renderCard(heroWithReset, "detailed");
      await waitFor(() => {
        expect(container.querySelector(".quota-row__reset")).not.toBeNull();
      });
      const heroReset = container.querySelector(".quota-row__reset")!;
      fireEvent.mouseEnter(heroReset);
      await new Promise((resolve) => setTimeout(resolve, 600));
      expect(document.body.querySelector(".tray-tip")).toBeNull();
      fireEvent.mouseLeave(heroReset);
    });
  });
});
