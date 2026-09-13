import { render } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";
import { V5Seg } from "./v5Controls";

describe("V5Seg", () => {
  it("renders one sliding indicator with the selected option index", () => {
    const { container } = render(
      <V5Seg
        value="remaining"
        options={[
          { value: "follow", label: "跟随" },
          { value: "used", label: "已用" },
          { value: "remaining", label: "剩余" },
        ]}
        onChange={vi.fn()}
      />,
    );

    const segment = container.querySelector<HTMLElement>(".s5-seg");
    const indicator = container.querySelector<HTMLElement>(".s5-seg__indicator");
    expect(segment).toBeInTheDocument();
    expect(indicator).toBeInTheDocument();
    expect(segment?.style.getPropertyValue("--s5-seg-count")).toBe("3");
    expect(segment?.style.getPropertyValue("--s5-seg-index")).toBe("2");
  });
});
