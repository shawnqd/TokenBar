import { render, screen } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";

vi.mock("../../../hooks/useLocale", () => ({
  useLocale: () => ({ t: (key: string) => key }),
}));

vi.mock("../../../lib/tauri", () => ({
  invokeSurfaceAction: vi.fn().mockResolvedValue("ok"),
}));

import NotificationsTab from "./NotificationsTab";
import type { SettingsSnapshot } from "../../../types/bridge";

const settings = {
  showNotifications: true,
  soundEnabled: true,
  soundVolume: 80,
  highUsageThreshold: 70,
  criticalUsageThreshold: 90,
} as unknown as SettingsSnapshot;

describe("NotificationsTab", () => {
  it("owns notification controls and not language or refresh", () => {
    render(
      <NotificationsTab settings={settings} set={vi.fn()} saving={false} />,
    );
    expect(screen.getByText("ShowNotifications")).toBeInTheDocument();
    expect(screen.getByText("SettingsUsageThresholdsCaption")).toBeInTheDocument();
    expect(screen.queryByText("InterfaceLanguage")).not.toBeInTheDocument();
    expect(screen.queryByText("RefreshIntervalLabel")).not.toBeInTheDocument();
  });
});
