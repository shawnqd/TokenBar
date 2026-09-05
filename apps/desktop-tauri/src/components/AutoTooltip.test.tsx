import { describe, it, expect, afterEach } from "vitest";
import { render, fireEvent, cleanup } from "@testing-library/react";
import { AutoTooltip } from "./AutoTooltip";

describe("AutoTooltip", () => {
  afterEach(cleanup);

  it("does not show tooltip when text does not overflow", () => {
    const { container } = render(<AutoTooltip text="Short text" className="test-span" />);
    const el = container.querySelector(".test-span") as HTMLElement;
    expect(el).toBeTruthy();

    // Mock scrollWidth == clientWidth (no overflow)
    Object.defineProperty(el, "scrollWidth", { value: 100, configurable: true });
    Object.defineProperty(el, "clientWidth", { value: 100, configurable: true });

    fireEvent.mouseEnter(el);
    expect(el.getAttribute("title")).toBeNull();
  });

  it("shows tooltip when text overflows (scrollWidth > clientWidth)", () => {
    const { container } = render(
      <AutoTooltip text="Very long text that gets truncated by ellipsis" className="test-span" />
    );
    const el = container.querySelector(".test-span") as HTMLElement;
    expect(el).toBeTruthy();

    // Mock scrollWidth > clientWidth (overflow)
    Object.defineProperty(el, "scrollWidth", { value: 250, configurable: true });
    Object.defineProperty(el, "clientWidth", { value: 100, configurable: true });

    fireEvent.mouseEnter(el);
    expect(el.getAttribute("title")).toBe("Very long text that gets truncated by ellipsis");
  });

  it("clears tooltip if container later expands and no longer overflows", () => {
    const { container } = render(
      <AutoTooltip text="Dynamic text" className="test-span" />
    );
    const el = container.querySelector(".test-span") as HTMLElement;

    // First: overflowing
    Object.defineProperty(el, "scrollWidth", { value: 200, configurable: true });
    Object.defineProperty(el, "clientWidth", { value: 100, configurable: true });
    fireEvent.mouseEnter(el);
    expect(el.getAttribute("title")).toBe("Dynamic text");

    // Later: resized wide enough, no longer overflowing
    Object.defineProperty(el, "scrollWidth", { value: 200, configurable: true });
    Object.defineProperty(el, "clientWidth", { value: 300, configurable: true });
    fireEvent.mouseEnter(el);
    expect(el.getAttribute("title")).toBeNull();
  });
});
