import { render } from "@testing-library/react";
import { describe, expect, it } from "vitest";
import { BarChart } from "./BarChart";

const series = (count: number) =>
  Array.from({ length: count }, (_, i) => ({ label: `d${i}`, value: i + 1 }));

function viewBoxOf(container: HTMLElement): string {
  const svg = container.querySelector("svg");
  if (!svg) throw new Error("the chart drew no svg");
  return svg.getAttribute("viewBox") ?? "";
}

describe("BarChart", () => {
  /**
   * The whole drawing is scaled by the viewBox, because `.chart__svg` is
   * `width: 100%; height: auto`. A viewBox narrowed to fit the bars therefore
   * magnifies *everything*, height included.
   *
   * That is not hypothetical: capping the bar width without pinning the viewBox
   * shipped once and turned a ten-point chart into two blocks overflowing their
   * panel. The scale must not depend on how much history a provider happens to
   * have.
   */
  it("scales identically no matter how many points it is given", () => {
    const few = render(<BarChart data={series(4)} height={90} ariaLabel="few" />);
    const many = render(<BarChart data={series(40)} height={90} ariaLabel="many" />);
    expect(viewBoxOf(few.container)).toBe(viewBoxOf(many.container));
  });

  /** Bars stay a readable width instead of stretching to fill the plot. */
  it("caps bar width, and centres a short series rather than stretching it", () => {
    const { container } = render(
      <BarChart data={series(3)} height={90} ariaLabel="short" />,
    );
    const bars = [...container.querySelectorAll("rect.chart__bar")];
    expect(bars.length).toBe(3);

    const widths = bars.map((bar) => Number(bar.getAttribute("width")));
    expect(Math.max(...widths)).toBeLessThanOrEqual(14);

    // Centred: the gap left of the first bar matches the gap right of the last.
    const [viewBoxWidth] = viewBoxOf(container).split(" ").slice(2).map(Number);
    const left = Number(bars[0].getAttribute("x"));
    const lastX = Number(bars[2].getAttribute("x"));
    const right = viewBoxWidth - (lastX + widths[2]);
    expect(Math.abs(left - right)).toBeLessThanOrEqual(1);
  });

  it("renders nothing but the empty message when it has no data", () => {
    const { container, getByText } = render(
      <BarChart data={[]} height={90} ariaLabel="none" emptyMessage="没有数据" />,
    );
    expect(container.querySelector("svg")).toBeNull();
    getByText("没有数据");
  });
});
