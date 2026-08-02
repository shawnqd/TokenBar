#!/usr/bin/env node
// Provider mark drift check — verifies that the Rust MARKS table in
// apps/desktop-tauri/src-tauri/src/provider_mark.rs matches
// PROVIDER_ICON_REGISTRY in src/components/providers/providerIcons.ts.
//
// The Windows taskbar strip prints a provider's one-glyph brand mark instead of
// its name, because the name is the longest part of a cell and four cells share
// one strip. That glyph and its colour come from the TypeScript registry, which
// is where the rest of the UI reads them, so the Rust copy is generated and has
// to be regenerated whenever a provider is added or recoloured. Without this
// check the drift is silent: an unknown id simply falls back to printing the
// provider's name, so the strip looks fine and just quietly loses the mark.
//
// Exit codes:
//   0 — tables match
//   1 — mismatch (prints a diff-style report)
//   2 — parse failure (file missing / regex produced zero matches)

import { readFileSync } from "node:fs";
import { fileURLToPath } from "node:url";
import { dirname, resolve } from "node:path";

const here = dirname(fileURLToPath(import.meta.url));
const tsPath = resolve(here, "..", "src", "components", "providers", "providerIcons.ts");
const rustPath = resolve(
  here,
  "..",
  "src-tauri",
  "src",
  "provider_mark.rs",
);

function die(code, msg) {
  console.error(`[check-provider-marks] ${msg}`);
  process.exit(code);
}

let tsSrc;
let rustSrc;
try {
  tsSrc = readFileSync(tsPath, "utf8");
  rustSrc = readFileSync(rustPath, "utf8");
} catch (err) {
  die(2, `failed to read source files: ${err.message}`);
}

const registryMatch = tsSrc.match(
  /export const PROVIDER_ICON_REGISTRY[\s\S]*?=\s*\{([\s\S]*?)\n\};/,
);
if (!registryMatch) {
  die(2, "could not locate `PROVIDER_ICON_REGISTRY` in providerIcons.ts");
}
const tsEntryRe =
  /^\s*([A-Za-z0-9_]+):\s*\{\s*id:\s*"([^"]+)",\s*brandColor:\s*"#([0-9a-fA-F]{6})",\s*fallbackLetter:\s*"([^"]+)"/gm;
const ts = new Map();
let m;
while ((m = tsEntryRe.exec(registryMatch[1])) !== null) {
  const [, key, id, color, letter] = m;
  if (key !== id) die(1, `registry key "${key}" does not match its id "${id}"`);
  ts.set(id, `${letter} #${color.toLowerCase()}`);
}
if (ts.size === 0) die(2, "parsed zero entries from PROVIDER_ICON_REGISTRY");

const marksMatch = rustSrc.match(
  /const MARKS: &\[\(&str, char, u32\)\] = &\[([\s\S]*?)\n\];/,
);
if (!marksMatch) {
  die(2, "could not locate `MARKS` in provider_mark.rs");
}
const rustEntryRe = /^\s*\("([^"]+)",\s*'(.)',\s*0x([0-9a-f]{6})\),\s*$/gm;
const rust = new Map();
while ((m = rustEntryRe.exec(marksMatch[1])) !== null) {
  const [, id, letter, color] = m;
  rust.set(id, `${letter} #${color}`);
}
if (rust.size === 0) die(2, "parsed zero entries from MARKS");

const problems = [];
for (const [id, value] of ts) {
  if (!rust.has(id)) problems.push(`  missing in Rust: ${id} (${value})`);
  else if (rust.get(id) !== value) {
    problems.push(`  differs: ${id} — ts "${value}" vs rust "${rust.get(id)}"`);
  }
}
for (const id of rust.keys()) {
  if (!ts.has(id)) problems.push(`  stale in Rust: ${id}`);
}

if (problems.length) {
  console.error(
    `[check-provider-marks] DRIFT DETECTED  ts=${ts.size} rust=${rust.size}`,
  );
  for (const line of problems) console.error(line);
  process.exit(1);
}

console.log(
  `[check-provider-marks] OK — ${ts.size} provider marks match between TS and Rust`,
);
