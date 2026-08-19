import { render, screen } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";

vi.mock("../../../hooks/useLocale", () => ({
  useLocale: () => ({ t: (key: string) => key }),
}));

import AdvancedTab from "./AdvancedTab";
import type { SettingsSnapshot } from "../../../types/bridge";

const settings = {
  updateChannel: "stable",
  autoDownloadUpdates: false,
  installUpdatesOnQuit: false,
} as unknown as SettingsSnapshot;

describe("AdvancedTab", () => {
  it("keeps updates and leaves shortcut/privacy on other pages", () => {
    render(<AdvancedTab settings={settings} set={vi.fn()} saving={false} />);
    expect(screen.getByText("UpdateChannelChoice")).toBeInTheDocument();
    expect(screen.getAllByText("SettingsResetAll").length).toBeGreaterThan(0);
    expect(screen.queryByText("GlobalShortcutFieldLabel")).not.toBeInTheDocument();
    expect(screen.queryByText("HidePersonalInfo")).not.toBeInTheDocument();
    expect(screen.queryByText("CodexLogPathsLabel")).not.toBeInTheDocument();
  });
});
