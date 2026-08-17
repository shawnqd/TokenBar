import { render, screen } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";

vi.mock("../../../hooks/useLocale", () => ({
  useLocale: () => ({ t: (key: string) => key }),
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
  it("hosts privacy, keychain and Codex dirs with unwired cache actions", () => {
    render(<PrivacyTab settings={settings} set={vi.fn()} saving={false} />);
    expect(screen.getByText("HidePersonalInfo")).toBeInTheDocument();
    expect(screen.getByText("DisableAllKeychainLabel")).toBeInTheDocument();
    expect(screen.getByText("CodexLogPathsLabel")).toBeInTheDocument();
    expect(screen.getAllByText("SettingsNotWired").length).toBeGreaterThan(0);
    expect(screen.queryByText("GlobalShortcutFieldLabel")).not.toBeInTheDocument();
    expect(screen.queryByText("UpdateChannelChoice")).not.toBeInTheDocument();
  });
});
