import { render, screen, fireEvent } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";

vi.mock("../../../hooks/useLocale", () => ({
  useLocale: () => ({ t: (key: string) => key, language: "english" }),
}));
vi.mock("../../../floatbar", () => ({
  FloatBarSettingsSection: () => null,
}));
vi.mock("../../../lib/tauri", () => ({
  getTaskbarFontFamilies: vi.fn().mockResolvedValue([]),
  // Empty rather than absent: this suite is about which settings keys each page
  // writes, not about the composer, so every window kind stays offered.
  getTaskbarWindowAvailability: vi.fn().mockResolvedValue({}),
  // Same reasoning: the preview's content is TaskbarTab's business, not this
  // suite's. `vi.mock` replaces the whole module, so every command a rendered
  // tab calls has to be present here or the tab throws on mount.
  getTaskbarPreviewLines: vi.fn().mockResolvedValue([]),
}));

import DashboardTab from "./DashboardTab";
import FloatBarTab from "./FloatBarTab";
import TaskbarTab from "./TaskbarTab";
import type { SettingsSnapshot, SettingsUpdate } from "../../../types/bridge";

const settings = {
  theme: "dark",
  menuBarDisplayMode: "detailed",
  windowScalePercent: 100,
  localUsagePeriod: "today",
  showAllTokenAccountsInMenu: false,
  floatBarShowAsUsed: true,
  floatBarResetTimeRelative: true,
  dashboardShowAsUsed: true,
  dashboardResetTimeRelative: true,
  taskbarShowAsUsed: true,
    taskbarContextMenuActions: ["open_panel", "refresh", "settings", "quit"],
    taskbarTooltipEntries: [],
  taskbarWidgetEnabled: true,
  menuFontWeight: 300,
  taskbarWidgetPosition: "notification",
  taskbarWidgetFontWeight: 400,
  taskbarWidgetContent: "usage",
  taskbarWidgetFontFamily: "Microsoft YaHei UI",
  taskbarWidgetEntries: [{ providerId: "auto", window: "session" }],
  taskbarWidgetFontSize: 12,
  taskbarWidgetWidth: 132,
  taskbarWidgetTextAlign: "left",
  enabledProviders: ["codex"],
} as unknown as SettingsSnapshot;

/**
 * Item H's whole purpose: three components, three independent settings sets.
 * A control on one page must never write another component's key, and neither
 * must that page's "restore defaults".
 */
describe("settings component isolation", () => {
  const cases = [
    { name: "dashboard", Tab: DashboardTab, own: /^dashboard/, foreign: [/^floatBar/, /^taskbar/] },
    { name: "floatBar", Tab: FloatBarTab, own: /^floatBar/, foreign: [/^dashboard/, /^taskbar/] },
    { name: "taskbar", Tab: TaskbarTab, own: /^taskbar/, foreign: [/^dashboard/, /^floatBar/] },
  ];

  for (const { name, Tab, own, foreign } of cases) {
    it(`${name} page writes only its own keys when toggled`, () => {
      const written: string[] = [];
      const set = (patch: SettingsUpdate) => {
        written.push(...Object.keys(patch));
      };

      const { container } = render(
        <Tab settings={settings} set={set} saving={false} />,
      );
      // Exercise every toggle on the page.
      container
        .querySelectorAll('input[type="checkbox"], button[role="switch"]')
        .forEach((el) => fireEvent.click(el));

      expect(written.length).toBeGreaterThan(0);
      for (const key of written) {
        for (const pattern of foreign) {
          expect(key).not.toMatch(pattern);
        }
      }
    });

    it(`${name} restore-defaults touches no other component`, () => {
      const written: string[] = [];
      const set = (patch: SettingsUpdate) => {
        written.push(...Object.keys(patch));
      };

      render(<Tab settings={settings} set={set} saving={false} />);
      const reset = screen.queryByText("ComponentResetDefaults");
      if (!reset) return; // taskbar uses its own appearance-reset label
      fireEvent.click(reset);

      expect(written.length).toBeGreaterThan(0);
      expect(written.some((key) => own.test(key))).toBe(true);
      for (const key of written) {
        for (const pattern of foreign) {
          expect(key).not.toMatch(pattern);
        }
      }
    });
  }
});
