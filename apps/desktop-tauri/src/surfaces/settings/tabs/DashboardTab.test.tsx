import { fireEvent, render, screen } from "@testing-library/react";
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
  windowScalePercent: 100,
  showAsUsed: false,
  showAllTokenAccountsInMenu: false,
  resetTimeRelative: false,
} as unknown as SettingsSnapshot;

function renderTab(
  set: (patch: Record<string, unknown>) => void,
  settings: SettingsSnapshot = {
    ...baseSettings,
    outputSpeedEnabled: false,
  },
) {
  return render(
    <DashboardTab settings={settings} set={set as never} saving={false} />,
  );
}

describe("DashboardTab window scale", () => {
  it("commits the new window scale on blur", () => {
    const set = vi.fn();
    renderTab(set);
    const slider = screen.getByRole("slider", { name: "WindowScaleAriaLabel" });

    fireEvent.change(slider, { target: { value: "175" } });
    fireEvent.blur(slider);

    expect(set).toHaveBeenCalledWith({ windowScalePercent: 175 });
  });

  it("does not commit when the value is unchanged", () => {
    const set = vi.fn();
    renderTab(set);
    const slider = screen.getByRole("slider", { name: "WindowScaleAriaLabel" });

    fireEvent.change(slider, { target: { value: "100" } });
    fireEvent.blur(slider);

    expect(set).not.toHaveBeenCalled();
  });
});
