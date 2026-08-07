import { fireEvent, render, screen, waitFor } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";

const tauriMocks = vi.hoisted(() => ({
  // Enumerated natively from DirectWrite in the real app; the shape here matches
  // what `get_taskbar_font_families` returns, including which families expose a
  // genuine weight axis.
  // Returned already sorted best-first and flagged, exactly as the backend
  // does: continuous weight ahead of static, curated ahead of the ~400 symbol
  // and script faces a real Windows install carries.
  getTaskbarFontFamilies: vi.fn().mockResolvedValue([
    { name: "Bahnschrift", variableWeight: true, hasCjk: false, recommended: true },
    {
      name: "Microsoft YaHei UI",
      variableWeight: false,
      hasCjk: true,
      recommended: true,
    },
    { name: "Wingdings", variableWeight: false, hasCjk: false, recommended: false },
  ]),
  // Resolved by the same backend code the strip resolves entries with. Grok
  // publishes only its own cycle; deepseek is a prepaid balance with no
  // percentage quota at all.
  getTaskbarWindowAvailability: vi.fn().mockResolvedValue({
    codex: ["session", "weekly", "speed"],
    grok: ["weekly"],
    deepseek: ["balance"],
  }),
  // The native renderer's own line buffer. The preview shows these verbatim
  // rather than composing an imitation, so the mock is shaped like real painted
  // output — a brand mark in its own colour, and a balance printed as money
  // rather than as the "unsupported" the old imitation always showed.
  getTaskbarPreviewLines: vi.fn().mockResolvedValue([
    { glyph: "◆", color: "#49a3b0", text: "周 18%" },
    { glyph: "◈", color: "#cc7c5e", text: "5小时 59%" },
    { glyph: "D", color: "#4d6bfe", text: "¥38.38" },
  ]),
}));

vi.mock("../../../lib/tauri", () => tauriMocks);

vi.mock("../../../hooks/useLocale", () => ({
  useLocale: () => ({ t: (key: string) => key, language: "english" }),
}));

import TaskbarTab from "./TaskbarTab";
import type { SettingsSnapshot } from "../../../types/bridge";

const settings = {
  enabledProviders: ["codex", "grok", "deepseek"],
  taskbarWidgetEnabled: true,
  menuFontWeight: 300,
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
  trayIconMode: "single",
  switcherShowsIcons: true,
  menuBarShowsHighestUsage: false,
  menuBarShowsPercent: true,
} as unknown as SettingsSnapshot;

describe("TaskbarTab", () => {
  /** Labels in the currently open dropdown, in order. */
  const openedOptions = () =>
    Array.from(document.querySelectorAll(".dropdown__option-label")).map(
      (el) => el.textContent ?? "",
    );

  it("shows an immediate preview and applies typography changes", async () => {
    const set = vi.fn();
    render(<TaskbarTab settings={settings} set={set} saving={false} />);

    // Whatever the renderer is painting, verbatim — including a balance printed
    // as money. The preview this replaced hardcoded "unsupported" for every
    // balance entry and invented its percentages, so it contradicted the strip
    // sitting next to it on screen.
    await waitFor(() =>
      expect(
        screen.getByLabelText("TaskbarWidgetPreviewLabel"),
      ).toHaveTextContent("¥38.38"),
    );
    const preview = screen.getByLabelText("TaskbarWidgetPreviewLabel");
    expect(preview).toHaveTextContent("周 18%");
    expect(preview).not.toHaveTextContent("TaskbarEntryUnsupported");

    // The mark keeps the renderer's colour: shapes repeat across providers, so
    // a monochrome preview would not identify anything.
    const marks = Array.from(
      preview.querySelectorAll<HTMLElement>(".taskbar-preview__mark"),
    );
    expect(marks.map((mark) => mark.style.color)).toEqual([
      "rgb(73, 163, 176)",
      "rgb(204, 124, 94)",
      "rgb(77, 107, 254)",
    ]);

    // Three entries fill the left column first, then start a second — the
    // column-major order `taskbar_widget::cell_rects` lays cells out in.
    const columns = Array.from(
      preview.querySelectorAll(".taskbar-preview__column"),
    );
    expect(
      columns.map((c) => c.querySelectorAll(".taskbar-preview__line").length),
    ).toEqual([2, 1]);

    const sizeInput = screen.getAllByRole("spinbutton")[0];
    fireEvent.change(sizeInput, { target: { value: "14" } });
    expect(set).toHaveBeenCalledWith({ taskbarWidgetFontSize: 14 });

    const weightSlider = screen.getByRole("slider", {
      name: "TaskbarWidgetFontWeightAriaLabel",
    });
    // The slider carries the OpenType `wght` axis value directly now, not an
    // index into three hardcoded stops. An intermediate value must survive: the
    // DirectWrite renderer draws 430 differently from 400 (measured in
    // `taskbar_text::tests`), so quantizing here would discard a real choice.
    fireEvent.change(weightSlider, { target: { value: "430" } });
    fireEvent.pointerUp(weightSlider);
    expect(set).toHaveBeenCalledWith({ taskbarWidgetFontWeight: 430 });
  });

  it("offers only font families this machine actually has", async () => {
    const set = vi.fn();
    render(<TaskbarTab settings={settings} set={set} saving={false} />);

    await waitFor(() => {
      expect(tauriMocks.getTaskbarFontFamilies).toHaveBeenCalled();
    });

    // The app deliberately does not use a native <select> (its OS popup cannot
    // be themed), so the picker is a button that opens a portalled list.
    const trigger = await screen.findByRole("button", {
      name: /Microsoft YaHei UI/,
    });
    fireEvent.click(trigger);
    const option = await screen.findByText(/Bahnschrift/);
    fireEvent.click(option);
    expect(set).toHaveBeenCalledWith({ taskbarWidgetFontFamily: "Bahnschrift" });
  });

  /// The complaint that produced the curated list: a raw ~400-family dropdown
  /// buried the handful of fonts that are actually usable in a 12px strip.
  it("hides uncurated families until 'show all' is switched on", async () => {
    const set = vi.fn();
    render(<TaskbarTab settings={settings} set={set} saving={false} />);

    await waitFor(() => {
      expect(tauriMocks.getTaskbarFontFamilies).toHaveBeenCalled();
    });

    const trigger = await screen.findByRole("button", {
      name: /Microsoft YaHei UI/,
    });
    fireEvent.click(trigger);
    expect(screen.queryByText(/Wingdings/)).toBeNull();
    // Continuous-weight families are labelled as such, because whether the
    // weight slider does anything depends entirely on the font.
    expect(screen.getByText(/Bahnschrift · TaskbarFontVariableTag/)).toBeTruthy();
    fireEvent.click(trigger);

    // `Toggle` renders a bare checkbox; the visible name lives in the sibling
    // `Field` label, so the switch is reached through its row.
    const showAllRow = screen
      .getByText("TaskbarFontShowAll")
      .closest(".settings-field");
    fireEvent.click(
      showAllRow!.querySelector('input[type="checkbox"]') as HTMLElement,
    );
    fireEvent.click(trigger);
    expect(await screen.findByText(/Wingdings/)).toBeTruthy();
  });

  it("restores the taskbar appearance defaults as one patch", async () => {
    const set = vi.fn();
    render(<TaskbarTab settings={settings} set={set} saving={false} />);

    await waitFor(() => {
      expect(tauriMocks.getTaskbarFontFamilies).toHaveBeenCalled();
    });
    fireEvent.click(screen.getByRole("button", { name: "TaskbarWidgetResetAppearance" }));

    expect(set).toHaveBeenCalledWith({
      taskbarWidgetFontSize: 12,
      taskbarWidgetWidth: 132,
      taskbarWidgetFontWeight: 400,
      // The best family this machine has, not a name hardcoded in the UI —
      // the backend already ranked continuous weight ahead of static.
      taskbarWidgetFontFamily: "Bahnschrift",
      taskbarWidgetTextAlign: "left",
    });
  });

  /**
   * The dropdown must offer only what the chosen provider can answer for.
   *
   * Before this, every row offered all six kinds regardless of provider, so
   * picking 周 for Grok — which publishes only a monthly window — was a silent
   * trap: the setting saved fine and the taskbar then printed 不支持. The user's
   * words: "不要让我猜".
   */
  it("offers only the windows the selected provider actually publishes", async () => {
    const set = vi.fn();
    render(
      <TaskbarTab
        settings={
          {
            ...settings,
            taskbarWidgetEntries: [{ providerId: "grok", window: "weekly" }],
          } as unknown as SettingsSnapshot
        }
        set={set}
        saving={false}
      />,
    );

    await waitFor(() => {
      expect(tauriMocks.getTaskbarWindowAvailability).toHaveBeenCalled();
    });

    const trigger = await screen.findByRole("button", {
      name: /TaskbarWindowWeekly/,
    });
    fireEvent.click(trigger);
    // Scoped to the option list: the closed trigger also renders the selected
    // label, so a document-wide query would match it and prove nothing.
    expect(openedOptions()).toEqual(["TaskbarWindowWeekly"]);
  });

  /**
   * An entry already pointing at something the provider stopped publishing keeps
   * its value in the list. Dropping it would make the control display a
   * different setting than the one stored — the honest signal for a stale entry
   * is the "不支持" the strip prints, not a silently rewritten choice.
   */
  it("keeps the current selection even when it is no longer available", async () => {
    const set = vi.fn();
    render(
      <TaskbarTab
        settings={
          {
            ...settings,
            taskbarWidgetEntries: [{ providerId: "grok", window: "monthly" }],
          } as unknown as SettingsSnapshot
        }
        set={set}
        saving={false}
      />,
    );

    await waitFor(() => {
      expect(tauriMocks.getTaskbarWindowAvailability).toHaveBeenCalled();
    });

    const trigger = await screen.findByRole("button", {
      name: /TaskbarWindowMonthly/,
    });
    fireEvent.click(trigger);
    expect(openedOptions()).toEqual(["TaskbarWindowWeekly", "TaskbarWindowMonthly"]);
  });
});
