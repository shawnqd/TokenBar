import { beforeEach, describe, expect, it, vi } from "vitest";
import { render, waitFor } from "@testing-library/react";

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

    it("renders status block encased in dual-pill-container in Detailed mode", async () => {
      const { container } = renderCard(azureSnapshot, "detailed");
      await waitFor(() => {
        expect(container.querySelector(".dual-pill-container")).not.toBeNull();
      });
      expect(container.querySelector(".status-block")).not.toBeNull();
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
});
