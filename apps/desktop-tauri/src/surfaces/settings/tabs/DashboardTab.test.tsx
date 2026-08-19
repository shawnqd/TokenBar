import { render, screen } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";

vi.mock("../../../hooks/useLocale", () => ({
  useLocale: () => ({ t: (key: string) => key, language: "english" }),
}));
vi.mock("../../../hooks/useProviders", () => ({
  useProviders: () => ({
    providers: [],
    isRefreshing: false,
    refresh: () => {},
    lastRefresh: null,
    hasCachedData: false,
    hasLoadedCache: true,
  }),
}));
vi.mock("../../../hooks/useOutputSpeedSnapshot", () => ({
  useOutputSpeedSnapshot: () => null,
}));
vi.mock("../../../lib/tauri", () => ({
  getProviderChartData: vi.fn().mockResolvedValue(null),
}));
vi.mock("@tauri-apps/api/event", () => ({
  listen: vi.fn().mockResolvedValue(() => {}),
}));

import DashboardTab from "./DashboardTab";
import type { SettingsSnapshot } from "../../../types/bridge";

const baseSettings = {
  trayIconMode: "single",
  switcherShowsIcons: false,
  menuBarShowsHighestUsage: false,
  menuBarShowsPercent: false,
  menuBarDisplayMode: "detailed",
  showAsUsed: false,
  showAllTokenAccountsInMenu: false,
  resetTimeRelative: false,
  dashboardShowAsUsed: true,
  dashboardResetTimeRelative: true,
  outputSpeedEnabled: false,
} as unknown as SettingsSnapshot;

describe("DashboardTab", () => {
  it("follows the HTML tray page: content + tray-grid groups and a live preview", () => {
    render(
      <DashboardTab
        settings={baseSettings}
        set={vi.fn() as never}
        saving={false}
      />,
    );
    expect(screen.queryByText("WindowScaleLabel")).not.toBeInTheDocument();
    expect(screen.getByText("卡片内容")).toBeInTheDocument();
    expect(screen.getByText("通知区与网格")).toBeInTheDocument();
    expect(screen.getByText("PanelZoom")).toBeInTheDocument();
    expect(screen.getByLabelText("实时预览")).toBeInTheDocument();
  });
});
