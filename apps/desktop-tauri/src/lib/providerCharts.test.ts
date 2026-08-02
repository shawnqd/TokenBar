import { describe, expect, it } from "vitest";
import { providerSupportsChartData } from "./providerCharts";

describe("providerSupportsChartData", () => {
  it("keeps chart fetches limited to providers with chart/local usage data", () => {
    expect(providerSupportsChartData("codex")).toBe(true);
    expect(providerSupportsChartData("claude")).toBe(true);
    expect(providerSupportsChartData("openai")).toBe(true);
    expect(providerSupportsChartData("OpenAI")).toBe(true);
    // Grok has a local session-log scanner in Rust. It was absent here for a
    // release: the backend produced the data and the card never asked for it.
    expect(providerSupportsChartData("grok")).toBe(true);

    expect(providerSupportsChartData("copilot")).toBe(false);
    expect(providerSupportsChartData("cursor")).toBe(false);
    expect(providerSupportsChartData("deepseek")).toBe(false);
  });
});
