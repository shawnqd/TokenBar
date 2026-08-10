import { describe, it, expect } from "vitest";
import {
  providerCostColor,
  providerCreditsColor,
  providerIndexBadge,
  relativeLuminance,
  serviceColorVar,
} from "./chartPalette";

describe("chartPalette.providerColor", () => {
  it("returns a CSS var() expression referencing a provider token for known ids", () => {
    expect(providerCostColor("claude")).toBe(
      "var(--chart-claude, var(--chart-cost))",
    );
    expect(providerCreditsColor("codex")).toBe(
      "var(--chart-codex, var(--chart-credits))",
    );
  });

  it("is case-insensitive and handles spaced aliases", () => {
    expect(providerCostColor("CURSOR")).toBe(
      "var(--chart-cursor, var(--chart-cost))",
    );
    expect(providerCostColor("Kimi K2")).toBe(
      "var(--chart-kimik2, var(--chart-cost))",
    );
    expect(providerCostColor("Vertex AI")).toBe(
      "var(--chart-vertexai, var(--chart-cost))",
    );
  });

  it("falls back to the generic cost/credits token for unknown providers", () => {
    expect(providerCostColor("unknown-provider-xyz")).toBe("var(--chart-cost)");
    expect(providerCreditsColor("another-ghost")).toBe(
      "var(--chart-credits)",
    );
  });
});

describe("chartPalette.providerIndexBadge", () => {
  it("uses the brand token for known colourful providers with normal tone", () => {
    expect(providerIndexBadge("codex")).toEqual({
      color: "var(--chart-codex, rgb(73, 163, 176))",
      tone: "normal",
    });
    expect(providerIndexBadge("deepseek").tone).toBe("normal");
  });

  it("inverts near-black brands like grok so the digit stays readable", () => {
    const grok = providerIndexBadge("grok");
    expect(grok.tone).toBe("dark");
    expect(grok.color).toContain("--chart-grok");
    // Same brand under aliases.
    expect(providerIndexBadge("xai").tone).toBe("dark");
    expect(providerIndexBadge("supergrok").tone).toBe("dark");
  });

  it("falls back to accent for auto / empty, and cost token for unknowns", () => {
    expect(providerIndexBadge("auto")).toEqual({
      color: "var(--accent)",
      tone: "normal",
    });
    expect(providerIndexBadge("").tone).toBe("normal");
    expect(providerIndexBadge("not-a-real-provider").color).toBe(
      "var(--chart-cost)",
    );
  });

  it("classifies pure black as dark via relative luminance", () => {
    expect(relativeLuminance([0, 0, 0])).toBeLessThan(0.12);
    expect(relativeLuminance([73, 163, 176])).toBeGreaterThan(0.12);
  });
});

describe("chartPalette.serviceColorVar", () => {
  it("routes named service kinds to dedicated tokens", () => {
    expect(serviceColorVar("cli", ["cli"])).toBe("var(--chart-service-cli)");
    expect(serviceColorVar("github code review", ["github code review"])).toBe(
      "var(--chart-service-review)",
    );
    expect(serviceColorVar("api-calls", ["api-calls"])).toBe(
      "var(--chart-service-api)",
    );
  });

  it("assigns deterministic palette slots (1..5) to unknown services", () => {
    const ordered = ["alpha", "beta", "gamma", "delta", "epsilon", "zeta"];
    expect(serviceColorVar("alpha", ordered)).toBe("var(--chart-service-1)");
    expect(serviceColorVar("epsilon", ordered)).toBe("var(--chart-service-5)");
    // slot wraps with modulo 5 → "zeta" is index 5 → slot 1
    expect(serviceColorVar("zeta", ordered)).toBe("var(--chart-service-1)");
  });
});
