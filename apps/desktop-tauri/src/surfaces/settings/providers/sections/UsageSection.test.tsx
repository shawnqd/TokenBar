import { render, screen } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { LocaleProvider } from "../../../../i18n/LocaleProvider";
import { buildBundle } from "../../../../test/localeHarness";
import type { QuotaDisplayContext } from "../../../../lib/quotaDisplay";
import type { ProviderDetail } from "../../../../types/bridge";
import { UsageSection } from "./UsageSection";

const DISPLAY: QuotaDisplayContext = {
  showAsUsed: true,
  resetTimeRelative: true,
  highUsageThreshold: 70,
  criticalUsageThreshold: 90,
};

const tauriMocks = vi.hoisted(() => ({
  getLocaleStrings: vi.fn(),
  setUiLanguage: vi.fn(),
}));

const eventMocks = vi.hoisted(() => ({
  listen: vi.fn(),
}));

vi.mock("../../../../lib/tauri", async (importOriginal) => ({
  ...(await importOriginal<typeof import("../../../../lib/tauri")>()),
  ...tauriMocks,
}));
vi.mock("@tauri-apps/api/event", () => eventMocks);

function rateWindow(usedPercent: number, resetDescription: string | null = null) {
  return {
    usedPercent,
    remainingPercent: 100 - usedPercent,
kind: null,
        windowMinutes: null,
    resetsAt: null,
    resetDescription,
    isExhausted: false,
    reservePercent: null,
    reserveDescription: null,
  };
}

function provider(): ProviderDetail {
  return {
    id: "copilot",
    displayName: "GitHub Copilot",
    enabled: true,
    email: null,
    plan: null,
    authType: null,
    sourceLabel: null,
    organization: null,
    lastUpdated: null,
    session: rateWindow(20),
    weekly: null,
    modelSpecific: null,
    tertiary: null,
    extraRateWindows: [
      { id: "additional_budget", title: "Additional Budget", window: rateWindow(42) },
    ],
    cost: null,
    pace: null,
    lastError: null,
    dashboardUrl: null,
    statusPageUrl: null,
    buyCreditsUrl: null,
    hasSnapshot: true,
    cookieSource: null,
    region: null,
    usageSource: "auto",
  };
}

describe("UsageSection", () => {
  beforeEach(() => {
    vi.clearAllMocks();
    tauriMocks.getLocaleStrings.mockResolvedValue(buildBundle());
    eventMocks.listen.mockResolvedValue(() => {});
  });

  it("renders extra Copilot budget windows in settings", async () => {
    render(
      <LocaleProvider>
        <UsageSection provider={provider()} display={DISPLAY} t={(key) => key} />
      </LocaleProvider>,
    );

    expect(await screen.findByText("Additional Budget")).toBeInTheDocument();
    expect(screen.getByText("42%")).toBeInTheDocument();
    expect(document.querySelector(".provider-quota")).toBeInTheDocument();
  });

  it.each([
    {
      id: "deepseek",
      session: rateWindow(0, "¥38.81 (Paid: ¥38.81 / Granted: ¥0.00)"),
      weekly: null,
      amount: "¥38.81",
      granted: "含赠送 ¥0.00",
    },
    {
      id: "mimo",
      session: rateWindow(0, "No active MiMo Token Plan"),
      weekly: rateWindow(
        0,
        "34.46 CNY balance (Paid: 34.46 CNY / Granted: 0.00 CNY)",
      ),
      amount: "¥34.46",
      granted: "含赠送 ¥0.00",
    },
  ])("renders $id with the shared balance-only structure", async (example) => {
    const detail = provider();
    detail.id = example.id;
    detail.displayName = example.id;
    detail.session = example.session;
    detail.weekly = example.weekly;
    detail.extraRateWindows = [];

    const { container } = render(
      <LocaleProvider>
        <UsageSection provider={detail} display={DISPLAY} t={(key) => key} />
      </LocaleProvider>,
    );

    expect(await screen.findByText("余额")).toBeInTheDocument();
    expect(screen.getByText(example.amount)).toBeInTheDocument();
    expect(screen.getByText(example.granted)).toBeInTheDocument();
    // The balance title is promoted to the section h4 so balance-only cards
    // open with the same header rhythm as every other detail section.
    expect(container.querySelector(".provider-detail-section > h4")).toHaveTextContent("余额");
    expect(container.querySelector(".provider-balance__title")).not.toBeInTheDocument();
    expect(container.querySelector(".provider-balance__row")).toBeInTheDocument();
    expect(container.querySelector(".provider-quota")).not.toBeInTheDocument();
    expect(screen.queryByText("0%")).not.toBeInTheDocument();
  });
});
