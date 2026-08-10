/**
 * Chart palette helpers — map provider ids and service names to the
 * CSS custom properties declared in `styles.css`. Keeping the mapping
 * centralised means Phase 12 (theming) can flip the tokens in one
 * place rather than touching every chart component.
 *
 * Provider token mapping mirrors `native_ui/theme.rs::provider_color`
 * and service mapping mirrors `native_ui/charts.rs::color_for_service`.
 */

const PROVIDER_TOKEN: Record<string, string> = {
  claude: "--chart-claude",
  codex: "--chart-codex",
  gemini: "--chart-gemini",
  cursor: "--chart-cursor",
  copilot: "--chart-copilot",
  jetbrains: "--chart-jetbrains",
  "jetbrains ai": "--chart-jetbrains",
  antigravity: "--chart-antigravity",
  augment: "--chart-augment",
  amp: "--chart-amp",
  factory: "--chart-factory",
  droid: "--chart-droid",
  kimi: "--chart-kimi",
  kimik2: "--chart-kimik2",
  "kimi k2": "--chart-kimik2",
  kiro: "--chart-kiro",
  opencode: "--chart-opencode",
  minimax: "--chart-minimax",
  vertexai: "--chart-vertexai",
  "vertex ai": "--chart-vertexai",
  zai: "--chart-zai",
  "z.ai": "--chart-zai",
  alibaba: "--chart-alibaba",
  tongyi: "--chart-alibaba",
  nanogpt: "--chart-nanogpt",
  mistral: "--chart-mistral",
  codebuff: "--chart-codebuff",
  manicode: "--chart-codebuff",
  deepseek: "--chart-deepseek",
  "deep seek": "--chart-deepseek",
  windsurf: "--chart-windsurf",
  codeium: "--chart-windsurf",
  elevenlabs: "--chart-elevenlabs",
  "eleven labs": "--chart-elevenlabs",
  deepgram: "--chart-deepgram",
  grok: "--chart-grok",
  xai: "--chart-grok",
  supergrok: "--chart-grok",
  groq: "--chart-groq",
  groqcloud: "--chart-groq",
  llmproxy: "--chart-llmproxy",
  "llm proxy": "--chart-llmproxy",
};

/** CSS color expression for a provider's cost-series bars. */
export function providerCostColor(providerId: string): string {
  const token = PROVIDER_TOKEN[providerId.toLowerCase()];
  return token ? `var(${token}, var(--chart-cost))` : "var(--chart-cost)";
}

/** CSS color expression for a provider's credits-series line. */
export function providerCreditsColor(providerId: string): string {
  const token = PROVIDER_TOKEN[providerId.toLowerCase()];
  return token ? `var(${token}, var(--chart-credits))` : "var(--chart-credits)";
}

/**
 * Concrete RGB for each chart brand token — kept in lockstep with the
 * `--chart-*` values in `styles.css`. Needed because a CSS `var()` cannot
 * be luminance-tested from JS; the index badge has to know whether a brand
 * is near-black (grok) or near-white so it can invert for contrast.
 */
const PROVIDER_RGB: Record<string, readonly [number, number, number]> = {
  claude: [204, 124, 94],
  codex: [73, 163, 176],
  gemini: [171, 135, 234],
  cursor: [0, 191, 165],
  copilot: [168, 85, 247],
  jetbrains: [255, 51, 153],
  antigravity: [96, 186, 126],
  augment: [99, 102, 241],
  amp: [220, 38, 38],
  factory: [255, 107, 53],
  droid: [255, 107, 53],
  kimi: [254, 96, 60],
  kimik2: [76, 0, 255],
  kiro: [255, 153, 0],
  opencode: [59, 130, 246],
  minimax: [254, 96, 60],
  vertexai: [66, 133, 244],
  zai: [232, 90, 106],
  alibaba: [255, 106, 0],
  nanogpt: [104, 127, 161],
  mistral: [255, 80, 15],
  codebuff: [68, 255, 0],
  deepseek: [82, 125, 240],
  windsurf: [34, 197, 94],
  elevenlabs: [17, 24, 39],
  deepgram: [19, 239, 147],
  grok: [0, 0, 0],
  groq: [245, 80, 54],
  llmproxy: [79, 70, 229],
};

/** WCAG relative luminance of an sRGB triple (0–255 channels). */
export function relativeLuminance([r, g, b]: readonly [number, number, number]): number {
  const lin = (c: number) => {
    const s = c / 255;
    return s <= 0.03928 ? s / 12.92 : ((s + 0.055) / 1.055) ** 2.4;
  };
  return 0.2126 * lin(r) + 0.7152 * lin(g) + 0.0722 * lin(b);
}

/**
 * How the index badge should paint a brand colour:
 * - `normal` — soft tint background + brand-coloured digit (most brands)
 * - `dark`   — brand is near-black; solid fill + inverted (light) digit on
 *              light surfaces; on dark surfaces CSS lifts to a light face
 * - `light`  — brand is near-white; bordered soft fill + dark digit
 */
export type ProviderBadgeTone = "normal" | "dark" | "light";

export interface ProviderIndexBadge {
  /** CSS colour for `--entry-idx-color` (var() or rgb()). */
  color: string;
  tone: ProviderBadgeTone;
}

/**
 * Resolve the coloured index badge for a taskbar entry's provider.
 *
 * Uses the shared brand palette. Extreme luminances (grok pure black,
 * theoretically pure white) flip to an inverted treatment so the digit
 * stays readable on both light and dark settings themes.
 *
 * `auto` / unknown providers fall back to the accent-cost token with a
 * normal tone — no brand to invert against.
 */
export function providerIndexBadge(providerId: string): ProviderIndexBadge {
  const id = (providerId || "").toLowerCase().trim();
  if (!id || id === "auto") {
    return { color: "var(--accent)", tone: "normal" };
  }

  const token = PROVIDER_TOKEN[id];
  const rgb = token
    ? PROVIDER_RGB[token.replace(/^--chart-/, "")] ?? null
    : PROVIDER_RGB[id] ?? null;

  // Prefer the concrete RGB when we have it so luminance is exact; still
  // expose a CSS var as the painted colour so theme overrides (if any)
  // keep working for the normal path.
  if (rgb) {
    const L = relativeLuminance(rgb);
    const color = token
      ? `var(${token}, rgb(${rgb[0]}, ${rgb[1]}, ${rgb[2]}))`
      : `rgb(${rgb[0]}, ${rgb[1]}, ${rgb[2]})`;
    if (L < 0.12) return { color, tone: "dark" };
    if (L > 0.85) return { color, tone: "light" };
    return { color, tone: "normal" };
  }

  return {
    color: providerCostColor(providerId),
    tone: "normal",
  };
}

/**
 * Resolve a UsageBreakdownChart service name to a palette token. `ordered`
 * is the sorted list of distinct services in the visible data so that
 * unrelated services receive different colors.
 */
export function serviceColorVar(service: string, ordered: string[]): string {
  const lower = service.toLowerCase();
  if (lower === "cli") return "var(--chart-service-cli)";
  if (lower.includes("github") && lower.includes("review")) {
    return "var(--chart-service-review)";
  }
  if (lower.includes("api")) return "var(--chart-service-api)";

  // Deterministic palette spread — prefer the position within the
  // sorted unique-services list so the colors are stable across
  // renders, but wrap into the 5-slot extras palette.
  const idx = Math.max(0, ordered.indexOf(service));
  const slot = (idx % 5) + 1;
  return `var(--chart-service-${slot})`;
}
