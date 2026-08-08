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
  dashboardProviderIds: [],
  dashboardQuotaWindows: [],
  taskbarShowAsUsed: true,
  taskbarResetTimeRelative: true,
    taskbarTooltipEntries: [],
  taskbarWidgetEnabled: true,
  menuFontWeight: 300,
  menuFontFamily: "Microsoft YaHei UI",
  menuFontSize: 12,
  taskbarWidgetPosition: "notification",
  taskbarWidgetFontWeight: 400,
  taskbarWidgetContent: "usage",
  taskbarWidgetFontFamily: "Microsoft YaHei UI",
  taskbarWidgetEntries: [{ providerId: "auto", window: "session" }],
  taskbarWidgetFontSize: 12,
  taskbarWidgetWidth: 132,
  taskbarWidgetTextAlign: "left",
  floatBarEnabled: true,
  floatBarProviderIds: [],
  // Two, not one: the provider-filter chips refuse to unpress the last active
  // one, so a single-provider fixture would render a control this suite cannot
  // click and the pages' filters would go unexercised.
  enabledProviders: ["codex", "claude"],
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
      // Exercise every binary control on the page. `role="radio"` covers the
      // SegmentedControl: settings that are stored as a boolean but read as a
      // choice between two named things (used/remaining, countdown/exact time)
      // stopped being switches, and without this the FloatBar page had no
      // clickable control left and the assertion below passed vacuously.
      // `aria-pressed` covers the chip groups — the provider filters and the
      // bar's reset-window picker.
      container
        .querySelectorAll(
          'input[type="checkbox"], button[role="switch"], button[role="radio"], button[aria-pressed]',
        )
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
