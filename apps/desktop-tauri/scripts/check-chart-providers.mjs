#!/usr/bin/env node
/**
 * Guard the frontend's chart-capable provider set against the Rust functions
 * that actually produce the data.
 *
 * The frontend decides whether to CALL `get_provider_chart_data` at all. If it
 * says no, a provider's card stays empty no matter what the backend can do —
 * which is exactly what happened when Grok's local-usage scanner was added in
 * Rust and `providerCharts.ts` was left alone. Nothing failed; the feature was
 * simply invisible. Two hand-maintained lists of the same fact will drift, so
 * this makes the drift a build error instead of a silent one.
 *
 * Sources of truth, both in Rust:
 *   * `scan_local_cost`                  — providers with a local log scanner
 *   * `load_openai_dashboard_chart_data` — providers with hosted chart data
 */
import { readFileSync } from "node:fs";
import { dirname, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const here = dirname(fileURLToPath(import.meta.url));
const rustPath = resolve(here, "../src-tauri/src/commands/chart.rs");
const tsPath = resolve(here, "../src/lib/providerCharts.ts");

const rust = readFileSync(rustPath, "utf8");
const ts = readFileSync(tsPath, "utf8");

/** Body of a `fn name(...) { ... }`, matched by brace depth. */
function functionBody(source, name) {
  const start = source.indexOf(`fn ${name}(`);
  if (start === -1) {
    throw new Error(`[check-chart-providers] fn ${name} not found in chart.rs`);
  }
  let index = source.indexOf("{", start);
  if (index === -1) throw new Error(`[check-chart-providers] no body for ${name}`);
  let depth = 0;
  const bodyStart = index;
  for (; index < source.length; index += 1) {
    if (source[index] === "{") depth += 1;
    else if (source[index] === "}") {
      depth -= 1;
      if (depth === 0) return source.slice(bodyStart, index + 1);
    }
  }
  throw new Error(`[check-chart-providers] unbalanced braces in ${name}`);
}

const expected = new Set();

// Match arms like `"codex" => Some(...)`. The catch-all `_ => None` has no
// string literal, so it is skipped naturally.
const scanBody = functionBody(rust, "scan_local_cost");
for (const [, id] of scanBody.matchAll(/"([a-z0-9_-]+)"\s*=>/g)) {
  expected.add(id);
}

// Guard clause of the shape `provider_id != "codex" && provider_id != "openai"`.
const dashboardBody = functionBody(rust, "load_openai_dashboard_chart_data");
for (const [, id] of dashboardBody.matchAll(/provider_id\s*!=\s*"([a-z0-9_-]+)"/g)) {
  expected.add(id);
}

if (expected.size === 0) {
  console.error(
    "[check-chart-providers] parsed no provider ids from chart.rs — the shape " +
      "of scan_local_cost or load_openai_dashboard_chart_data changed, and this " +
      "check is no longer checking anything. Fix the parser above.",
  );
  process.exit(1);
}

const setLiteral = ts.match(/PROVIDER_CHART_DATA_IDS\s*=\s*new Set\(\[([^\]]*)\]\)/);
if (!setLiteral) {
  console.error("[check-chart-providers] PROVIDER_CHART_DATA_IDS not found in providerCharts.ts");
  process.exit(1);
}
const actual = new Set(
  [...setLiteral[1].matchAll(/"([^"]+)"/g)].map(([, id]) => id.toLowerCase()),
);

const missing = [...expected].filter((id) => !actual.has(id)).sort();
const extra = [...actual].filter((id) => !expected.has(id)).sort();

if (missing.length || extra.length) {
  console.error("[check-chart-providers] providerCharts.ts disagrees with chart.rs");
  if (missing.length) {
    console.error(
      `  Rust produces data for, but the frontend never asks: ${missing.join(", ")}\n` +
        "  -> add them to PROVIDER_CHART_DATA_IDS, or their cards stay empty.",
    );
  }
  if (extra.length) {
    console.error(
      `  The frontend asks for, but Rust has no source: ${extra.join(", ")}\n` +
        "  -> remove them, or every card pays a round trip that returns nothing.",
    );
  }
  process.exit(1);
}

console.log(
  `[check-chart-providers] OK — ${expected.size} chart providers match between Rust and TS`,
);
