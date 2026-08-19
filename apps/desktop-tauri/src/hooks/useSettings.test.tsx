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
import type { SettingsSnapshot, SettingsUpdate } from "../types/bridge";

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

  it("discards stale updateSettings responses that arrive out of order", async () => {
    // Deferred promises to control resolution order — simulate the race
    // where the first IPC request resolves after the second.
    let resolveFirst!: (v: SettingsSnapshot) => void;
    const firstPromise = new Promise<SettingsSnapshot>((resolve) => {
      resolveFirst = resolve;
    });
    let resolveSecond!: (v: SettingsSnapshot) => void;
    const secondPromise = new Promise<SettingsSnapshot>((resolve) => {
      resolveSecond = resolve;
    });

    tauriMocks.getSettingsSnapshot.mockResolvedValue(snapshot(false));
    tauriMocks.updateSettings
      .mockResolvedValueOnce(firstPromise)
      .mockResolvedValueOnce(secondPromise);

    const initial = snapshot(false);
    const { result } = renderHook(() => useSettings(initial));

    await waitFor(() => expect(result.current.settings).toBeDefined());

    // First update: gap=5 (request starts, stays pending)
    await act(async () => {
      result.current.update({
        taskbarWidgetIconGapPx: 5,
      } as SettingsUpdate);
    });

    // Second update: gap=8 (request starts, stays pending)
    await act(async () => {
      result.current.update({
        taskbarWidgetIconGapPx: 8,
      } as SettingsUpdate);
    });

    // Resolve the SECOND (latest) call first — it should win
    await act(async () => {
      resolveSecond({
        ...snapshot(false),
        taskbarWidgetIconGapPx: 8,
      } as unknown as SettingsSnapshot);
    });
    await waitFor(() => {
      expect(result.current.settings).toHaveProperty(
        "taskbarWidgetIconGapPx",
        8,
      );
    });

    // Now resolve the FIRST (stale) call — its response must be discarded
    await act(async () => {
      resolveFirst({
        ...snapshot(false),
        taskbarWidgetIconGapPx: 5,
      } as unknown as SettingsSnapshot);
    });
    // Give React a microtask to flush any state update from the stale response
    await new Promise((r) => setTimeout(r, 50));
    expect(result.current.settings).toHaveProperty(
      "taskbarWidgetIconGapPx",
      8,
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