import { render, screen } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";

vi.mock("../../../hooks/useLocale", () => ({
  useLocale: () => ({ t: (key: string) => key, language: "english" }),
}));

import DisplayTab from "./DisplayTab";
import type { SettingsSnapshot } from "../../../types/bridge";

const baseSettings = {
  theme: "dark",
  menuBarDisplayMode: "detailed",
  windowScalePercent: 100,
} as unknown as SettingsSnapshot;

describe("DisplayTab", () => {
  it("keeps the app-wide theme control", () => {
    render(
      <DisplayTab settings={baseSettings} set={() => {}} saving={false} />,
    );
    expect(screen.getByText("ThemeLabel")).toBeInTheDocument();
  });

  /**
   * Item H: a single global control that quietly overrides three components is
   * exactly what the split exists to prevent. Display must own app-level
   * appearance only — component settings belong to their own pages.
   */
  it("owns no component-specific settings", () => {
    const { container } = render(
      <DisplayTab settings={baseSettings} set={() => {}} saving={false} />,
    );
    expect(screen.queryByText("ShowAsUsedLabel")).not.toBeInTheDocument();
    expect(screen.queryByText("ResetTimeRelative")).not.toBeInTheDocument();
    expect(screen.queryByText("DisplayModeLabel")).not.toBeInTheDocument();
    expect(container.querySelector('input[type="range"]')).toBeNull();
  });
});
