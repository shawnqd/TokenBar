import { act, renderHook, waitFor } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";

const tauriMocks = vi.hoisted(() => ({
  reanchorTrayPanel: vi.fn(),
  revealTrayPanelWindow: vi.fn(),
}));

vi.mock("../lib/tauri", () => tauriMocks);

import { useTrayPanelLayout } from "./useTrayPanelLayout";

describe("useTrayPanelLayout first-frame handshake", () => {
  let frames: FrameRequestCallback[];
  let resolveFonts: () => void;

  beforeEach(() => {
    vi.clearAllMocks();
    tauriMocks.reanchorTrayPanel.mockResolvedValue(undefined);
    tauriMocks.revealTrayPanelWindow.mockResolvedValue(undefined);
    frames = [];
    vi.stubGlobal("requestAnimationFrame", (callback: FrameRequestCallback) => {
      frames.push(callback);
      return frames.length;
    });
    const fontsReady = new Promise<void>((resolve) => {
      resolveFonts = resolve;
    });
    Object.defineProperty(document, "fonts", {
      configurable: true,
      value: { ready: fontsReady },
    });
  });

  it("reveals only after fonts and two composited frame boundaries", async () => {
    // `null` restores the unbounded font wait so this test exercises the
    // fonts-gated path deterministically instead of racing the ceiling.
    const { result } = renderHook(() =>
      useTrayPanelLayout({ canMeasure: true, fontReadyTimeoutMs: null }),
    );

    await waitFor(() =>
      expect(tauriMocks.reanchorTrayPanel).toHaveBeenCalledTimes(1),
    );
    expect(tauriMocks.revealTrayPanelWindow).not.toHaveBeenCalled();

    await act(async () => resolveFonts());
    await waitFor(() => expect(result.current.layoutReady).toBe(true));
    expect(frames).toHaveLength(1);
    expect(tauriMocks.revealTrayPanelWindow).not.toHaveBeenCalled();

    await act(async () => frames.shift()?.(0));
    expect(frames).toHaveLength(1);
    expect(tauriMocks.revealTrayPanelWindow).not.toHaveBeenCalled();

    await act(async () => frames.shift()?.(16));
    await waitFor(() =>
      expect(tauriMocks.revealTrayPanelWindow).toHaveBeenCalledTimes(1),
    );
  });

  it("does not reveal after the surface unmounts during first-frame work", async () => {
    const { unmount } = renderHook(() =>
      useTrayPanelLayout({ canMeasure: true, fontReadyTimeoutMs: null }),
    );
    await waitFor(() =>
      expect(tauriMocks.reanchorTrayPanel).toHaveBeenCalledTimes(1),
    );

    unmount();
    await act(async () => resolveFonts());
    expect(frames).toHaveLength(0);
    expect(tauriMocks.revealTrayPanelWindow).not.toHaveBeenCalled();
  });

  it("reveals without fonts once the wait ceiling elapses", async () => {
    // A 20MB preferred web font must never hold the panel hostage: with the
    // ceiling at 0 the handshake proceeds and `font-display: swap` paints the
    // fallback family until the real font lands.
    const { result } = renderHook(() =>
      useTrayPanelLayout({ canMeasure: true, fontReadyTimeoutMs: 0 }),
    );

    await waitFor(() => expect(result.current.layoutReady).toBe(true));
    expect(tauriMocks.revealTrayPanelWindow).not.toHaveBeenCalled();

    await act(async () => frames.shift()?.(0));
    await act(async () => frames.shift()?.(16));
    await waitFor(() =>
      expect(tauriMocks.revealTrayPanelWindow).toHaveBeenCalledTimes(1),
    );

    // Resolving fonts afterwards must not trigger a second reveal.
    await act(async () => resolveFonts());
    expect(tauriMocks.revealTrayPanelWindow).toHaveBeenCalledTimes(1);
  });
});
