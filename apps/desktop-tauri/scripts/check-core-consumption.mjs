#!/usr/bin/env node
/**
 * Guard the unified core read-model contract.
 *
 * TASK-V5-CORE-INTEGRATION-044: surfaces must read snapshots from the core
 * store; they must not import legacy per-surface data hooks nor invoke
 * chart/usage commands directly. Fails the build when a direct path is
 * reintroduced.
 */
import { readFileSync, readdirSync, statSync } from "node:fs";
import { dirname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const here = dirname(fileURLToPath(import.meta.url));
const srcRoot = resolve(here, "../src");

const SURFACE_DIRS = ["surfaces", "floatbar"];
const LEGACY_HOOKS = ["useProviders"];
const FORBIDDEN_CALLS = ["get_provider_chart_data", "scan_local_cost"];

// Only these files may keep the legacy useProviders migration fallback.
// Remove once the app-core prewarm integration fully replaces it.
const LEGACY_ALLOWLIST = new Set([
  "surfaces/settings/pages/ProvidersPage.tsx",
  "surfaces/settings/tabs/ProvidersTab.tsx",
]);

function* walk(dir) {
  for (const entry of readdirSync(dir)) {
    const full = join(dir, entry);
    if (statSync(full).isDirectory()) {
      if (entry === "node_modules" || entry === "dist") continue;
      yield* walk(full);
    } else if (/.(ts|tsx)$/.test(entry)) {
      yield full;
    }
  }
}

function relativePath(file) {
  return file.replace(/\\/g, "/").replace(srcRoot.replace(/\\/g, "/") + "/", "");
}

const failures = [];
for (const dir of SURFACE_DIRS) {
  const root = resolve(srcRoot, dir);
  if (!statSync(root).isDirectory()) continue;
  for (const file of walk(root)) {
    if (/.(test|spec)./.test(file)) continue;
    const rel = relativePath(file);
    const text = readFileSync(file, "utf8");
    const lines = text.split(/\r?\n/);

    if (!LEGACY_ALLOWLIST.has(rel)) {
      for (const hook of LEGACY_HOOKS) {
        const re = new RegExp("from [\"''][^\"'']*hooks/" + hook + "[\"'']");
        if (re.test(text)) failures.push(rel + ": imports legacy hook " + hook);
      }
    }
    for (let i = 0; i < lines.length; i++) {
      const line = lines[i].trim();
      if (line.indexOf("//") === 0 || line.indexOf("*") === 0) continue;
      for (const token of FORBIDDEN_CALLS) {
        if (line.includes(token)) failures.push(rel + ":" + (i + 1) + ": direct " + token + " call");
      }
    }
  }
}

if (failures.length > 0) {
  console.error("[check-core-consumption] FAILED:");
  for (const f of failures) console.error("  - " + f);
  process.exit(1);
}
console.log("[check-core-consumption] OK - surfaces read core store, no legacy hooks / direct chart calls");
