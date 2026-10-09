import { render } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import ProviderGrid from "./ProviderGrid";
import type { ProviderUsageSnapshot } from "../types/bridge";

vi.mock("../hooks/useLocale", () => ({
  useLocale: () => ({ t: (key: string) => key }),
}));

function providersOf(count: number): ProviderUsageSnapshot[] {
  return Array.from({ length: count }, (_, index) => ({
    providerId: `p${index}`,
    displayName: `Provider ${index}`,
  })) as unknown as ProviderUsageSnapshot[];
}

/** jsdom does not implement WheelEvent; the handler only reads deltas. */
function wheelEvent(delta: { deltaY?: number; deltaX?: number; deltaMode?: number }) {
  const event = new Event("wheel", { bubbles: true, cancelable: true });
  Object.defineProperty(event, "deltaY", { value: delta.deltaY ?? 0 });
  Object.defineProperty(event, "deltaX", { value: delta.deltaX ?? 0 });
  Object.defineProperty(event, "deltaMode", { value: delta.deltaMode ?? 0 });
  return event;
}

describe("ProviderGrid wheel-to-horizontal scrolling", () => {
  let scrollLeft: number;

  beforeEach(() => {
    scrollLeft = 0;
  });

  afterEach(() => {
    vi.restoreAllMocks();
  });

  function renderStrip(overflow: boolean) {
    const { container, unmount } = render(
      <ProviderGrid
        providers={providersOf(overflow ? 40 : 3)}
        selectedProviderId={null}
        display={{} as never}
        onSelect={() => {}}
      />,
    );
    const grid = container.querySelector<HTMLElement>(".provider-grid");
    if (!grid) throw new Error("provider grid missing");
    Object.defineProperty(grid, "scrollLeft", {
      configurable: true,
      get: () => scrollLeft,
      set: (value: number) => {
        scrollLeft = value;
      },
    });
    Object.defineProperty(grid, "scrollWidth", {
      configurable: true,
      value: overflow ? 800 : 300,
    });
    Object.defineProperty(grid, "clientWidth", {
      configurable: true,
      value: 300,
    });
    return { grid, unmount };
  }

  it("maps vertical wheel deltas onto scrollLeft when the strip overflows", () => {
    const { grid } = renderStrip(true);
    grid.dispatchEvent(wheelEvent({ deltaY: 120 }));
    expect(scrollLeft).toBe(120);

    grid.dispatchEvent(wheelEvent({ deltaY: 90 }));
    expect(scrollLeft).toBe(210);
  });

  it("clamps at both scroll ends instead of going negative or past max", () => {
    const { grid } = renderStrip(true);
    grid.dispatchEvent(wheelEvent({ deltaY: -500 }));
    expect(scrollLeft).toBe(0);

    grid.dispatchEvent(wheelEvent({ deltaY: 5000 }));
    expect(scrollLeft).toBe(500);
  });

  it("consumes the wheel event so the card stack does not scroll vertically", () => {
    const { grid } = renderStrip(true);
    const event = wheelEvent({ deltaY: 120 });
    grid.dispatchEvent(event);
    expect(event.defaultPrevented).toBe(true);
  });

  it("leaves the wheel untouched when the strip fits without overflow", () => {
    const { grid } = renderStrip(false);
    const event = wheelEvent({ deltaY: 120 });
    grid.dispatchEvent(event);
    expect(scrollLeft).toBe(0);
    expect(event.defaultPrevented).toBe(false);
  });

  it("leaves pure horizontal trackpad deltas to native panning", () => {
    const { grid } = renderStrip(true);
    const event = wheelEvent({ deltaX: 120 });
    grid.dispatchEvent(event);
    expect(scrollLeft).toBe(0);
    expect(event.defaultPrevented).toBe(false);
  });

  it("scales line-mode deltas (deltaMode 1) up to pixel distances", () => {
    const { grid } = renderStrip(true);
    grid.dispatchEvent(wheelEvent({ deltaY: 3, deltaMode: 1 }));
    expect(scrollLeft).toBe(120);
  });
});
