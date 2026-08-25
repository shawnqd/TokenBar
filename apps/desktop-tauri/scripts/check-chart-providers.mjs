#!/usr/bin/env node
/**
 * Guard the chart/usage data path against silent drift.
 *
 * The frontend now treats every provider uniformly: it always calls
 * `get_provider_chart_data`, and the backend (`commands/chart.rs`) returns real
 * data where it has a parser and empty/zero otherwise. There is no longer an
 * allow-list to keep in sync — the old per-provider gate was exactly how Grok's
 * scanner became invisible (backend had it, frontend never asked).
 *
 * What still matters:
 *   1. `providerSupportsChartData` must still exist in providerCharts.ts (the
 *      surfaces import it as the single uniform capability descriptor).
 *   2. The Rust data sources must still exist and still carry the providers
 *      they parse (`scan_local_cost` arms, `load_openai_dashboard_chart_data`
 *      guard), so a future backend change cannot silently drop a parser while
 *      the frontend keeps asking.
 */
import { readFileSync } from "node:fs";
import { dirname, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const here = dirname(fileURLToPath(import.meta.url));
const rustPath = resolve(here, "../src-tauri/src/commands/chart.rs");
const tsPath = resolve(here, "../src/lib/providerCharts.ts");

const rust = readFileSync(rustPath, "utf8");
const ts = readFileSync(tsPath, "utf8");

const { length: providerIds } = ts.match(/providerSupportsChartData/g) ?? [];
if (!/export function providerSupportsChartData/.test(ts) || providerIds === 0) {
  console.error(
    "[check-chart-providers] providerSupportsChartData is missing or not exported " +
      "from providerCharts.ts — the uniform capability descriptor broke.",
  );
  process.exit(1);
}

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

const parsed = new Set();

const scanBody = functionBody(rust, "scan_local_cost");
for (const [, id] of scanBody.matchAll(/"([a-z0-9_-]+)"\s*=>/g)) {
  parsed.add(id);
}

const dashboardBody = functionBody(rust, "load_openai_dashboard_chart_data");
for (const [, id] of dashboardBody.matchAll(/provider_id\s*!=\s*"([a-z0-9_-]+)"/g)) {
  parsed.add(id);
}

if (parsed.size === 0) {
  console.error(
    "[check-chart-providers] parsed no provider ids from chart.rs — the shape of " +
      "scan_local_cost or load_openai_dashboard_chart_data changed, and nothing is " +
      "checked any more. Fix the parser above.",
  );
  process.exit(1);
}

console.log(
  `[check-chart-providers] OK — frontend asks uniformly; Rust parses chart/usage ` +
    `data for: ${[...parsed].sort().join(", ")}`,
);
