import { render, screen } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";

vi.mock("../../../hooks/useLocale", () => ({
  useLocale: () => ({ t: (key: string) => key }),
}));

vi.mock("../../../lib/tauri", () => ({
  getSafeDiagnostics: vi.fn(async () => ({
    appVersion: "0.0.0-test",
    platform: "test",
    schemaVersion: 1,
    enabledProviders: [],
    providerCookieSources: {},
    hasManualCookies: [],
    hasApiKeys: [],
    hidePersonalInfo: false,
    refreshIntervalSecs: 300,
  })),
  invokeSurfaceAction: vi.fn(async () => "ok"),
}));

import PrivacyTab from "./PrivacyTab";
import type { SettingsSnapshot } from "../../../types/bridge";

const settings = {
  hidePersonalInfo: false,
  disableKeychainAccess: false,
  claudeAvoidKeychainPrompts: true,
  codexCustomSessionsDirs: [],
} as unknown as SettingsSnapshot;

describe("PrivacyTab", () => {
  it("hosts privacy, keychain, Codex dirs and live cache actions", () => {
    render(<PrivacyTab settings={settings} set={vi.fn()} saving={false} />);
    expect(screen.getByText("HidePersonalInfo")).toBeInTheDocument();
    expect(screen.getByText("DisableAllKeychainLabel")).toBeInTheDocument();
    expect(screen.getByText("CodexLogPathsLabel")).toBeInTheDocument();
    expect(screen.getAllByText("SettingsClearCache").length).toBeGreaterThan(0);
    expect(screen.getAllByText("SettingsExportDiagnostics").length).toBeGreaterThan(0);
    expect(screen.queryByText("SettingsNotWired")).not.toBeInTheDocument();
    expect(screen.queryByText("GlobalShortcutFieldLabel")).not.toBeInTheDocument();
    expect(screen.queryByText("UpdateChannelChoice")).not.toBeInTheDocument();
  });
});
