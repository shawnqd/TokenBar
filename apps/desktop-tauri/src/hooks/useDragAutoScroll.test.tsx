import { act, renderHook } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { useDragAutoScroll } from "./useDragAutoScroll";

function domRect(top: number, height: number): DOMRect {
  return {
    top,
    bottom: top + height,
    left: 0,
    right: 200,
    width: 200,
    height,
    x: 0,
    y: top,
    toJSON: () => ({}),
  } as DOMRect;
}

describe("useDragAutoScroll", () => {
  let pending: Map<number, FrameRequestCallback>;
  let nextId: number;
  let container: HTMLElement;
  let ref: { current: HTMLElement | null };
  let scrollTop: number;

  beforeEach(() => {
    pending = new Map();
    nextId = 1;
    vi.stubGlobal("requestAnimationFrame", (callback: FrameRequestCallback) => {
      const id = nextId++;
      pending.set(id, callback);
      return id;
    });
    vi.stubGlobal("cancelAnimationFrame", (id: number) => {
      pending.delete(id);
    });

    // Fixed layout box spanning y 100..500 so both edge zones are testable.
    scrollTop = 100;
    container = document.createElement("div");
    Object.defineProperty(container, "getBoundingClientRect", {
      configurable: true,
      value: () => domRect(100, 400),
    });
    Object.defineProperty(container, "scrollTop", {
      configurable: true,
      get: () => scrollTop,
      set: (value: number) => {
        scrollTop = value;
      },
    });
    document.body.appendChild(container);
    ref = { current: container };
  });

  afterEach(() => {
    container.remove();
    vi.unstubAllGlobals();
  });

  const dragover = (clientY: number) => {
    window.dispatchEvent(new MouseEvent("dragover", { clientY, bubbles: true }));
  };

  const flush = async (count: number) => {
    for (let i = 0; i < count; i += 1) {
      const entry = pending.entries().next();
      if (entry.done) break;
      const [id, frame] = entry.value;
      pending.delete(id);
      await act(async () => {
        frame(0);
      });
    }
  };

  it("scrolls up while the pointer is inside the top edge zone", async () => {
    renderHook(() => useDragAutoScroll(ref, true));
    await flush(1);

    dragover(120);
    await flush(1);
    expect(scrollTop).toBe(100 - Math.ceil(((148 - 120) / 48) * 16));
  });

  it("keeps scrolling at full speed when the pointer is above the container", async () => {
    renderHook(() => useDragAutoScroll(ref, true));
    await flush(1);

    // Sortable's built-in auto-scroll clears its timers exactly here — the
    // pointer is over the search box above the list, with no scrollable
    // ancestor. This is the regression: dragging toward the top must keep
    // scrolling instead of dying.
    dragover(50);
    await flush(1);
    expect(scrollTop).toBe(100 - 16);
  });

  it("scrolls down when the pointer is inside the bottom edge zone", async () => {
    renderHook(() => useDragAutoScroll(ref, true));
    await flush(1);

    dragover(470);
    await flush(1);
    expect(scrollTop).toBe(100 + Math.ceil(((470 - 452) / 48) * 16));
  });

  it("does not scroll while the pointer is mid-list", async () => {
    renderHook(() => useDragAutoScroll(ref, true));
    await flush(1);

    dragover(300);
    await flush(3);
    expect(scrollTop).toBe(100);
  });

  it("stops following a stale pointer after dragend", async () => {
    renderHook(() => useDragAutoScroll(ref, true));
    await flush(1);

    dragover(50);
    await flush(1);
    expect(scrollTop).toBe(84);

    window.dispatchEvent(new Event("dragend"));
    await flush(3);
    expect(scrollTop).toBe(84);
  });

  it("stops scrolling entirely once the drag deactivates", async () => {
    const { rerender } = renderHook(
      ({ active }: { active: boolean }) => useDragAutoScroll(ref, active),
      { initialProps: { active: true } },
    );
    await flush(1);
    dragover(50);
    await flush(1);
    expect(scrollTop).toBe(84);

    rerender({ active: false });
    dragover(50);
    await flush(4);
    expect(scrollTop).toBe(84);
  });
});
