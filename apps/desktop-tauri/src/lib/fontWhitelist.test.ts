import { describe, expect, it } from "vitest";
import {
  buildFontPickerOptions,
  firstInstalledWhitelistFamily,
  fontInstallGuide,
  isFamilyReady,
  isWhitelistedFamily,
} from "./fontWhitelist";
import type { TaskbarFontFamily } from "../types/bridge";

const installed: TaskbarFontFamily[] = [
  { name: "MiSans VF", variableWeight: true, hasCjk: true, recommended: true },
  {
    name: "Segoe UI Variable Text",
    variableWeight: true,
    hasCjk: false,
    recommended: true,
  },
  {
    name: "Bahnschrift",
    variableWeight: true,
    hasCjk: false,
    recommended: true,
  },
  {
    name: "Microsoft YaHei UI",
    variableWeight: false,
    hasCjk: true,
    recommended: false,
  },
];

describe("font whitelist", () => {
  it("never offers system variable faces outside the five approved families", () => {
    const options = buildFontPickerOptions(installed, "MiSans VF");
    const labels = options.map((option) => option.label).join("\n");
    expect(labels).toContain("MiSans VF");
    expect(labels).toContain("Source Han Sans VF");
    expect(labels).toContain("Noto Sans CJK VF");
    expect(labels).toContain("Source Han Serif VF");
    expect(labels).toContain("Noto Serif CJK VF");
    expect(labels).not.toContain("Segoe");
    expect(labels).not.toContain("Bahnschrift");
    expect(labels).not.toContain("Sitka");
  });

  it("keeps a previously saved non-whitelist family selectable", () => {
    const options = buildFontPickerOptions(installed, "Microsoft YaHei UI");
    expect(options[0]).toEqual({
      value: "Microsoft YaHei UI",
      label: "Microsoft YaHei UI",
    });
    expect(isWhitelistedFamily("Microsoft YaHei UI")).toBe(false);
  });

  it("picks the first installed whitelist family as default", () => {
    expect(firstInstalledWhitelistFamily(installed)).toBe("MiSans VF");
  });

  it("treats bundled CN/SC subset names as the confirmed picker families", () => {
    expect(isWhitelistedFamily("Source Han Sans CN VF")).toBe(true);
    expect(isWhitelistedFamily("Noto Sans SC")).toBe(true);
    expect(isWhitelistedFamily("Source Han Serif CN VF")).toBe(true);
    expect(isWhitelistedFamily("Noto Serif SC")).toBe(true);
  });

  it("still lists the five families when nothing matching is installed", () => {
    const options = buildFontPickerOptions([], "Microsoft YaHei UI");
    expect(options.map((option) => option.value)).toEqual([
      "Microsoft YaHei UI",
      "MiSans VF",
      "Source Han Sans VF",
      "Noto Sans CJK VF",
      "Source Han Serif VF",
      "Noto Serif CJK VF",
    ]);
  });

  it("treats only MiSans as ready without a system install", () => {
    expect(isFamilyReady("MiSans VF", [])).toBe(true);
    expect(isFamilyReady("Source Han Sans VF", [])).toBe(false);
    expect(isFamilyReady("Microsoft YaHei UI", [])).toBe(true);
    expect(fontInstallGuide("Source Han Sans VF")?.file).toBe(
      "SourceHanSansCN-VF.ttf",
    );
    expect(fontInstallGuide("MiSans VF")).toBeUndefined();
  });
});
