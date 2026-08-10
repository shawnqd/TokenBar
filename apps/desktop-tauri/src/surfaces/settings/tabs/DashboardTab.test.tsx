import { render, screen } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";

vi.mock("../../../hooks/useLocale", () => ({
  useLocale: () => ({ t: (key: string) => key, language: "english" }),
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
  it("does not offer a window-scale control", () => {
    render(
      <DashboardTab
        settings={baseSettings}
        set={vi.fn() as never}
        saving={false}
      />,
    );
    expect(screen.queryByRole("slider")).not.toBeInTheDocument();
    expect(screen.queryByText("WindowScaleLabel")).not.toBeInTheDocument();
  });
});
