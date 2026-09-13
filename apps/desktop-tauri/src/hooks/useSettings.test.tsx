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
  invokeSurfaceAction: vi.fn(),
}));

vi.mock("@tauri-apps/api/event", () => eventMocks);
vi.mock("../lib/tauri", () => tauriMocks);

import { useSettings, SAVE_TIMEOUT_ERROR, SAVE_WATCHDOG_MS } from "./useSettings";
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
    tauriMocks.invokeSurfaceAction
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

    // Resolve the SECOND (latest) persist first — then the hook re-reads.
    tauriMocks.getSettingsSnapshot.mockResolvedValue({
      ...snapshot(false),
      taskbarWidgetIconGapPx: 8,
    } as unknown as SettingsSnapshot);
    await act(async () => {
      resolveSecond("ok" as unknown as SettingsSnapshot);
    });
    await waitFor(() => {
      expect(result.current.settings).toHaveProperty(
        "taskbarWidgetIconGapPx",
        8,
      );
    });

    // Now resolve the FIRST (stale) call — its response must be discarded
    await act(async () => {
      resolveFirst("ok" as unknown as SettingsSnapshot);
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

  /// The backend command can wedge (AppState contention with a background
  /// refresh has left it pending for minutes) — the "保存中…" pill and the
  /// disabled controls must not be pinned forever, and a hung command that
  /// settles late must not flash a saved toast over the timeout error.
  it("releases the saving pill when the backend command never settles", async () => {
    // Real timers with a shortened watchdog: fake timers here fight the test
    // library's own async plumbing (worker OOM).
    const watchdog = SAVE_WATCHDOG_MS;
    const original = watchdog.value;
    watchdog.value = 60;
    try {
      let settle!: (v: unknown) => void;
      tauriMocks.getSettingsSnapshot.mockResolvedValue(snapshot(true));
      tauriMocks.invokeSurfaceAction.mockImplementationOnce(
        () => new Promise((resolve) => { settle = resolve; }),
      );

      // Stable identity: the bootstrap effect keys on `initial`, so a new
      // object per render would loop forever (see the first test).
      const initial = snapshot(true);
      const { result } = renderHook(() => useSettings(initial));
      await act(async () => {
        void result.current.update({
          taskbarWidgetIconGapPx: 5,
        } as SettingsUpdate);
      });

      await new Promise((r) => setTimeout(r, 300));
      expect(result.current.error).toBe(SAVE_TIMEOUT_ERROR);
      expect(result.current.saving).toBe(false);
    } finally {
      watchdog.value = original;
    }
  });

  it("keeps the local completion when a cross-window event arrives", async () => {
    let resolvePersist!: (value: unknown) => void;
    const persist = new Promise((resolve) => {
      resolvePersist = resolve;
    });
    tauriMocks.getSettingsSnapshot.mockResolvedValue(snapshot(true));
    tauriMocks.invokeSurfaceAction.mockReturnValueOnce(persist);

    const initial = snapshot(true);
    const { result } = renderHook(() => useSettings(initial));
    const onSaved = vi.fn();
    window.addEventListener("codexbar:settings-updated", onSaved);
    await waitFor(() =>
      expect(eventMocks.listeners["settings-changed"]).toBeTypeOf("function"),
    );

    let savePromise!: Promise<void>;
    await act(async () => {
      savePromise = result.current.update({
        taskbarWidgetIconGapPx: 5,
      } as SettingsUpdate);
    });
    await act(async () => {
      await new Promise((resolve) => setTimeout(resolve, 260));
    });
    expect(result.current.saving).toBe(true);

    // Rust broadcasts this event for the persisted update. It refreshes the
    // snapshot but must not cancel the local save completion event.
    await act(async () => {
      eventMocks.listeners["settings-changed"]();
      await new Promise((resolve) => setTimeout(resolve, 0));
    });
    resolvePersist("ok");
    await act(async () => {
      await savePromise;
    });

    expect(result.current.saving).toBe(false);
    expect(onSaved).toHaveBeenCalledTimes(1);
    window.removeEventListener("codexbar:settings-updated", onSaved);
  });

});
