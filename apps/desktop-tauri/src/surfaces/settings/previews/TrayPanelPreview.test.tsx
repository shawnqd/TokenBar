import { beforeEach, describe, expect, it, vi } from "vitest";
import { render, waitFor } from "@testing-library/react";
import { fromBridge } from "../../../core";
import { trayCoreStore } from "../../../surfaces/tray/trayCoreStore";
import type {
  ProviderUsageSnapshot,
  RateWindowSnapshot,
  SettingsSnapshot,
} from "../../../types/bridge";

const tauriMocks = vi.hoisted(() => ({
  getLocaleStrings: vi.fn(),
  setUiLanguage: vi.fn(),
  getProviderChartData: vi.fn(),
}));

vi.mock("../../../lib/tauri", () => tauriMocks);

import { LocaleProvider } from "../../../i18n/LocaleProvider";
import { buildBundle } from "../../../test/localeHarness";
import TrayPanelPreview from "./TrayPanelPreview";

function rateWindow(used: number): RateWindowSnapshot {
  return {
    usedPercent: used,
    remainingPercent: 100 - used,
    kind: null,
    windowMinutes: null,
    resetsAt: null,
    resetDescription: null,
    isExhausted: false,
    reservePercent: null,
    reserveDescription: null,
  };
}

function provider(id: string, displayName: string, used = 20): ProviderUsageSnapshot {
  return {
    providerId: id,
    displayName,
    primary: rateWindow(used),
    primaryLabel: "Monthly",
    secondary: null,
    modelSpecific: null,
    tertiary: null,
    extraRateWindows: [],
    cost: null,
    planName: null,
    accountEmail: null,
    sourceLabel: "auto",
    updatedAt: "2026-05-24T00:00:00Z",
    error: null,
    pace: null,
    accountOrganization: null,
    trayStatusLabel: null,
    fetchDurationMs: null,
  };
}

const settings = {
  menuBarDisplayMode: "detailed",
  dashboardShowAsUsed: true,
  dashboardResetTimeRelative: true,
  highUsageThreshold: 70,
  criticalUsageThreshold: 90,
  outputSpeedEnabled: true,
  trayScalePercent: 100,
  switcherShowsIcons: true,
  localUsagePeriod: "today",
} as unknown as SettingsSnapshot;

function renderPreview() {
  return render(
    <LocaleProvider>
      <TrayPanelPreview settings={settings} />
    </LocaleProvider>,
  );
}

describe("TrayPanelPreview (core read model)", () => {
  beforeEach(() => {
    vi.clearAllMocks();
    trayCoreStore.resetForTest();
    tauriMocks.getLocaleStrings.mockResolvedValue(buildBundle({}));
    tauriMocks.getProviderChartData.mockResolvedValue({
      providerId: "claude",
      costHistory: [],
      creditsHistory: [],
      usageBreakdown: [],
      localUsage: null,
    });
  });

  it("falls back to the catalog fixture preview when the core store is empty", async () => {
    const { container } = renderPreview();
    await waitFor(() => {
      expect(container.querySelector(".settings-tray-preview")).not.toBeNull();
    });
    // pickPreviewSnapshot([]) falls back to the Claude fixture (78%).
    expect(container.textContent).toContain("Claude");
    expect(container.textContent).toContain("78%");
  });

  it("reads the preview provider from the tray core store (same source as the flyout)", async () => {
    const deepseek = {
      ...provider("deepseek", "DeepSeek", 0),
      primary: {
        ...rateWindow(0),
        resetDescription: "¥69.21 (Paid: ¥69.21 / Granted: ¥0.00)",
      },
    };
    trayCoreStore.seed(fromBridge(deepseek));
    const { container } = renderPreview();
    await waitFor(() => {
      expect(container.querySelector(".settings-tray-preview")).not.toBeNull();
    });
    // The seeded provider is the only record, so it wins over the fixture.
    expect(container.textContent).toContain("DeepSeek");
    expect(container.textContent).toContain("¥69.21");
  });

  it("prefers claude/codex/cursor over other live records, matching the flyout ordering helper", async () => {
    trayCoreStore.seed(fromBridge(provider("cursor", "Cursor", 85)));
    trayCoreStore.seed(
      fromBridge({
        ...provider("gemini", "Gemini", 10),
        primary: { ...rateWindow(10), kind: "monthly", windowMinutes: 720 * 60 },
      }),
    );
    const { container } = renderPreview();
    await waitFor(() => {
      expect(container.textContent).toContain("Cursor");
    });
    expect(container.textContent).not.toContain("Gemini");
  });
});