import { render, screen } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";
import type { SettingsSnapshot } from "../types/bridge";
import FloatBarSettingsSection from "./SettingsSection";

vi.mock("../hooks/useLocale", () => ({
  useLocale: () => ({ t: (key: string) => key }),
}));

const settings = {
  floatBarEnabled: true,
  floatBarOpacity: 90,
  floatBarScale: 100,
  floatBarOrientation: "horizontal",
  floatBarStyle: "floating",
  floatBarShowCost: false,
  floatBarShowResetInline: false,
  taskbarWidgetEnabled: false,
  taskbarWidgetPosition: "notification",
  taskbarWidgetFontWeight: 400,
  taskbarWidgetContent: "usage",
  taskbarWidgetEntries: [
    { providerId: "auto", window: "session" },
    { providerId: "auto", window: "weekly" },
  ],
  taskbarWidgetFontFamily: "Microsoft YaHei UI",
  taskbarWidgetFontSize: 12,
  taskbarWidgetWidth: 132,
  taskbarWidgetTextAlign: "left",
  floatBarShowAsUsed: true,
  floatBarResetTimeRelative: true,
  dashboardShowAsUsed: true,
  dashboardResetTimeRelative: true,
  taskbarShowAsUsed: true,
  floatBarDarkText: false,
  floatBarClickThrough: false,
} as unknown as SettingsSnapshot;

describe("FloatBar settings", () => {
  it("renders one cost toggle", () => {
    render(
      <FloatBarSettingsSection settings={settings} saving={false} set={vi.fn()} />,
    );

    expect(screen.getAllByText("FloatBarShowCost")).toHaveLength(1);
  });
});
