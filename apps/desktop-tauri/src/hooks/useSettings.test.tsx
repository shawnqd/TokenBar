import { act, renderHook, waitFor } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";

const eventMocks = vi.hoisted(() => {
  const listeners: Record<string, () => void> = {};
  return {
    listeners,
    listen: vi.fn((name: string, cb: () => void) => {
      listeners[name] = cb;
      return Promise.resolve(() => {
        delete listeners[name];
      });
    }),
  };
});

const tauriMocks = vi.hoisted(() => ({
  getSettingsSnapshot: vi.fn(),
  updateSettings: vi.fn(),
}));

vi.mock("@tauri-apps/api/event", () => eventMocks);
vi.mock("../lib/tauri", () => tauriMocks);

import { useSettings } from "./useSettings";
import type { SettingsSnapshot } from "../types/bridge";

const snapshot = (dashboardShowAsUsed: boolean) =>
  ({ dashboardShowAsUsed }) as unknown as SettingsSnapshot;

describe("useSettings live sync", () => {
  beforeEach(() => {
    vi.clearAllMocks();
  });

  it("re-fetches the snapshot when settings-changed fires from another window", async () => {
    tauriMocks.getSettingsSnapshot.mockResolvedValue(snapshot(true));
    // Stable identity: the hook's bootstrap effect keys on `initial`, so a new
    // object each render would loop forever.
    const initial = snapshot(true);
    const { result } = renderHook(() => useSettings(initial));

    // The hook registers a "settings-changed" listener.
    await waitFor(() =>
      expect(eventMocks.listeners["settings-changed"]).toBeTypeOf("function"),
    );

    // A change persisted by the detached Settings window flips the flag.
    tauriMocks.getSettingsSnapshot.mockResolvedValue(snapshot(false));
    await act(async () => {
      eventMocks.listeners["settings-changed"]();
    });

    await waitFor(() =>
      expect(result.current.settings.dashboardShowAsUsed).toBe(false),
    );
  });

  it("unsubscribes the listener on unmount", async () => {
    const unlisten = vi.fn();
    eventMocks.listen.mockResolvedValueOnce(unlisten);
    tauriMocks.getSettingsSnapshot.mockResolvedValue(snapshot(true));

    const initial = snapshot(true);
    const { unmount } = renderHook(() => useSettings(initial));
    await waitFor(() => expect(eventMocks.listen).toHaveBeenCalled());

    unmount();
    await waitFor(() => expect(unlisten).toHaveBeenCalledTimes(1));
  });
});