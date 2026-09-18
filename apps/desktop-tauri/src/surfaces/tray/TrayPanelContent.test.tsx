import { render, waitFor } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";

const tauriMocks = vi.hoisted(() => ({
  getLocaleStrings: vi.fn(),
  openProviderDashboard: vi.fn(),
  openProviderStatusPage: vi.fn(),
  invokeSurfaceAction: vi.fn(async () => "ok"),
}));

vi.mock("../../lib/tauri", () => tauriMocks);

import { FIXTURE_SNAPSHOTS } from "../../core/fixtures";
import { coreSnapshotToBridge } from "../../lib/trayProviders";
import { LocaleProvider } from "../../i18n/LocaleProvider";
import { buildBundle } from "../../test/localeHarness";
import TrayPanelContent from "./TrayPanelContent";

const DISPLAY = {
  showAsUsed: true,
  resetTimeRelative: true,
  highUsageThreshold: 75,
  criticalUsageThreshold: 95,
} as const;

describe("TrayPanelContent canonical data boundary", () => {
  beforeEach(() => {
    tauriMocks.getLocaleStrings.mockResolvedValue(
      buildBundle(
        {
          ProviderSessionLabel: "5 小时额度",
          ProviderWeeklyLabel: "周额度",
          ProviderMonthlyLabel: "月额度",
          QuotaPaceOnPace: "按节奏",
          QuotaPaceInReserve: "结余",
          QuotaPaceInDeficit: "透支",
          PanelExpected: "预计",
        },
        "chinese",
      ),
    );
  });

  it("keeps core-only telemetry when the grid still uses its bridge view", async () => {
    const core = FIXTURE_SNAPSHOTS.telemetryOnly;
    const bridge = coreSnapshotToBridge(core);
    const { container } = render(
      <LocaleProvider>
        <TrayPanelContent
          gridProviders={[bridge]}
          visibleProviders={[bridge]}
          selectedProviderId={null}
          display={DISPLAY}
          densityMode="detailed"
          coreById={new Map([[core.providerId, core]])}
          onSelect={() => {}}
        />
      </LocaleProvider>,
    );

    await waitFor(() => {
      expect(container.querySelector(".tray-card")).not.toBeNull();
    });
    expect(container.querySelector(".telemetry-card")).not.toBeNull();
    expect(core.telemetry).not.toBeNull();
  });
});
