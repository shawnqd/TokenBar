import { fireEvent, render, screen } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";

vi.mock("../../../hooks/useLocale", () => ({
  useLocale: () => ({ t: (key: string) => key, language: "english" }),
}));

import DisplayTab from "./DisplayTab";
import type { SettingsSnapshot } from "../../../types/bridge";

vi.mock("../../../lib/tauri", () => ({
  getTaskbarFontFamilies: () => Promise.resolve([]),
}));

const baseSettings = {
  theme: "dark",
  menuBarDisplayMode: "detailed",
  windowScalePercent: 100,
  menuFontSize: 12,
  menuFontFamily: "Microsoft YaHei UI",
  menuFontWeight: 300,
} as unknown as SettingsSnapshot;

/** Keys owned by the three components item H split apart. */
const COMPONENT_KEY_PREFIXES = ["taskbarWidget", "floatBar", "dashboard"];

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
   *
   * This used to assert `input[type="range"]` was absent, because back then the
   * only slider in the app was the taskbar strip's font weight. That proxy
   * stopped matching the intent once the right-click menu got the same font
   * block: the menu is not one of the three components — it has no page of its
   * own, and after M3 it is shared by the strip *and* the tray icon — so a
   * slider here is not automatically a violation. What is a violation is this
   * page writing a component's key, which is now what gets asserted.
   */
  it("owns no component-specific settings", () => {
    const set = vi.fn();
    const { container } = render(
      <DisplayTab settings={baseSettings} set={set} saving={false} />,
    );
    expect(screen.queryByText("ShowAsUsedLabel")).not.toBeInTheDocument();
    expect(screen.queryByText("ResetTimeRelative")).not.toBeInTheDocument();
    expect(screen.queryByText("DisplayModeLabel")).not.toBeInTheDocument();

    // Drive every control the page has and collect what it tries to persist.
    const range = container.querySelector('input[type="range"]');
    if (range) {
      fireEvent.change(range, { target: { value: "700" } });
      fireEvent.blur(range);
    }
    for (const input of container.querySelectorAll('input[type="number"]')) {
      fireEvent.change(input, { target: { value: "14" } });
    }

    const written = set.mock.calls.flatMap((call) => Object.keys(call[0] ?? {}));
    expect(written.length).toBeGreaterThan(0);
    for (const key of written) {
      for (const prefix of COMPONENT_KEY_PREFIXES) {
        expect(key.startsWith(prefix)).toBe(false);
      }
    }
  });
});
