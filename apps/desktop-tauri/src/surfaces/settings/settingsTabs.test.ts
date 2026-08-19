import { describe, expect, it } from "vitest";
import {
  canonicalizeSettingsTab,
  isSettingsTab,
} from "./settingsTabs";

describe("settings tab deep-links", () => {
  it("keeps accepting old ids and resolves them to the V5 pages", () => {
    expect(isSettingsTab("dashboard")).toBe(true);
    expect(isSettingsTab("menuBar")).toBe(true);
    expect(isSettingsTab("menu")).toBe(true);
    expect(isSettingsTab("trayPanel")).toBe(true);
    expect(isSettingsTab("security")).toBe(false);
    expect(canonicalizeSettingsTab("dashboard")).toBe("trayPanel");
    expect(canonicalizeSettingsTab("menuBar")).toBe("taskbarStatus");
    expect(canonicalizeSettingsTab("menu")).toBe("appearance");
    expect(canonicalizeSettingsTab("privacy")).toBe("privacy");
  });
});
