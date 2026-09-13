import { fireEvent, render, screen, cleanup } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { MultiSelect, Select } from "./FormControls";

const originalInnerHeight = window.innerHeight;

function mockGeometry() {
  vi.spyOn(HTMLElement.prototype, "getBoundingClientRect").mockImplementation(
    function (this: HTMLElement) {
      if (this.classList.contains("dropdown__trigger")) {
        return {
          top: 340,
          bottom: 368,
          left: 20,
          right: 220,
          width: 200,
          height: 28,
        } as DOMRect;
      }
      if (this.classList.contains("dropdown__panel")) {
        return {
          top: 0,
          bottom: 220,
          left: 20,
          right: 220,
          width: 200,
          height: 220,
        } as DOMRect;
      }
      return {
        top: 0,
        bottom: 0,
        left: 0,
        right: 0,
        width: 0,
        height: 0,
      } as DOMRect;
    },
  );
}

describe("themed dropdown placement", () => {
  beforeEach(() => {
    Object.defineProperty(window, "innerHeight", {
      configurable: true,
      value: 400,
    });
    mockGeometry();
  });

  afterEach(() => {
    cleanup();
    vi.restoreAllMocks();
    Object.defineProperty(window, "innerHeight", {
      configurable: true,
      value: originalInnerHeight,
    });
  });

  it("flips a Select above a bottom-edge trigger", () => {
    render(
      <Select
        value="current"
        options={[{ value: "current", label: "Current" }, ...Array.from({ length: 5 }, (_, i) => ({
          value: `option-${i}`,
          label: `Option ${i + 1}`,
        }))]}
        onChange={vi.fn()}
      />,
    );

    fireEvent.click(screen.getByRole("button", { name: "Current" }));

    const panel = screen.getByRole("listbox");
    expect(panel).toHaveClass("dropdown__panel--above");
    expect(panel).toHaveStyle({ bottom: "64px" });
  });

  it("uses the same flipped placement for MultiSelect", () => {
    render(
      <MultiSelect
        values={["current"]}
        options={[{ value: "current", label: "Current" }, { value: "other", label: "Other" }]}
        summary="Current"
        onChange={vi.fn()}
      />,
    );

    fireEvent.click(screen.getByRole("button", { name: "Current" }));

    expect(screen.getByRole("listbox")).toHaveClass("dropdown__panel--above");
  });
});
