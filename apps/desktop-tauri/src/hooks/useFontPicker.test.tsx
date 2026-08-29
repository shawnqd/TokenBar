import { renderHook, waitFor } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";

vi.mock("../lib/tauri", () => ({
  invokeSurfaceAction: vi.fn(),
  getTaskbarFontFamilies: vi.fn().mockResolvedValue([
    {
      name: "Bahnschrift",
      variableWeight: true,
      hasCjk: false,
      recommended: true,
    },
    {
      name: "Cascadia Code",
      variableWeight: true,
      hasCjk: false,
      recommended: true,
    },
    {
      name: "Segoe UI Variable Text",
      variableWeight: true,
      hasCjk: false,
      recommended: true,
    },
    { name: "MiSans VF", variableWeight: true, hasCjk: true, recommended: true },
    {
      name: "Microsoft YaHei UI",
      variableWeight: false,
      hasCjk: true,
      recommended: false,
    },
  ]),
}));

import { useFontPicker } from "./useFontPicker";

describe("useFontPicker", () => {
  it("never offers system variable faces outside the five approved families", async () => {
    const { result } = renderHook(() => useFontPicker("Microsoft YaHei UI"));

    await waitFor(() => {
      expect(result.current.options.some((option) => option.value === "MiSans VF")).toBe(
        true,
      );
    });

    const values = result.current.options.map((option) => option.value);
    const labels = result.current.options.map((option) => option.label).join("\n");
    expect(values[0]).toBe("Microsoft YaHei UI");
    expect(values).toEqual([
      "Microsoft YaHei UI",
      "MiSans VF",
      "Source Han Sans VF",
      "Noto Sans CJK VF",
      "Source Han Serif VF",
      "Noto Serif CJK VF",
    ]);
    expect(labels).not.toMatch(/Bahnschrift|Cascadia|Segoe|Sitka/);
    expect(result.current.defaultFamily).toBe("MiSans VF");
  });
});
