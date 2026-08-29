#!/usr/bin/env node
/**
 * Guard the unified core read-model contract (TASK-V5-CORE-REPAIR-045).
 *
 * Surfaces must read one UsageStore, go through SurfaceRegistry/PluginHost
 * and dispatch nested SurfaceAction through `surface_action`. Fail the
 * build when a second store, a dispatcher bypass, or a dead host returns.
 */
import { mkdirSync, mkdtempSync, readFileSync, readdirSync, rmSync, statSync, writeFileSync } from "node:fs";
import { dirname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";
import { tmpdir } from "node:os";

const here = dirname(fileURLToPath(import.meta.url));
const srcRoot = resolve(here, "../src");

const STORE_FACTORIES = [
  "createUsageStore(",
  "createRefreshCoordinator(",
  "createEnrichmentScheduler(",
];

const FACTORY_ALLOWLIST = {
  "core/usageStore.ts": { owner: "core", reason: "store definition", remove: "never" },
  "core/refreshCoordinator.ts": { owner: "core", reason: "coordinator definition / optional store", remove: "never" },
  "core/enrichmentScheduler.ts": { owner: "core", reason: "scheduler definition", remove: "never" },
  "appRuntime.ts": { owner: "045", reason: "single production runtime factory", remove: "never" },
  "core/useCoreBridge.ts": {
    owner: "045",
    reason: "unbooted fallback + createCoreBridgeForTest",
    remove: "when every surface waits for ensureAppRuntimeBooted",
  },
  "surfaces/tray/trayCoreStore.ts": {
    owner: "045",
    reason: "resetForTest isolation; production uses attachTrayCoreRuntime",
    remove: "when TrayPanel tests boot exclusively through appRuntime",
  },
  "floatbar/floatBarStore.ts": {
    owner: "045",
    reason: "test fallback store when core bridge is unset",
    remove: "when FloatBar tests always attach the shared store",
  },
};

const DIRECT_COMMANDS = [
  "get_provider_chart_data",
  "scan_local_cost",
  "getProviderChartData(",
  "getProviderLocalUsageSummary(",
  "openProviderDashboard(",
  "openProviderStatusPage(",
  "openSettingsWindow(",
  "quitApp(",
  "triggerProviderLogin(",
  "refreshProviders(",
];

const DIRECT_COMMAND_ALLOWLIST = {
  "lib/tauri.ts": { owner: "bridge", reason: "command wrappers", remove: "never" },
  "appRuntime.ts": {
    owner: "045",
    reason: "fetcher/enrichment/seed still talk to the backend cache",
    remove: "never",
  },
  "core/chartAccess.ts": {
    owner: "045",
    reason: "single core chart loader used by enrichment/previews",
    remove: "never",
  },
  "hooks/useProviders.ts": {
    owner: "045",
    reason: "legacy adapter; production settings pass refreshOnMount:false",
    remove: "when ProvidersPage/Tab drop the hook",
  },
  "core/actionDispatcher.ts": { owner: "045", reason: "may document invoke names", remove: "never" },
  "surfaces/settings/providers/ProviderDetailPane.tsx": {
    owner: "045",
    reason: "detail pane still mixes credential commands; dashboard/login should move to dispatcher",
    remove: "when ProviderDetailPane dispatches SurfaceAction for login/usage/status/refresh",
  },
  "surfaces/settings/pages/ProvidersPage.tsx": {
    owner: "045",
    reason: "auth-body login still uses triggerProviderLogin during credential capture",
    remove: "when AuthBody uses dispatcher.triggerLogin",
  },
  "surfaces/settings/providers/sections/credentials/GeminiCliCreds.tsx": {
    owner: "045",
    reason: "bespoke CLI credential control opens the provider console",
    remove: "when CLI creds dispatch openExternalUsage",
  },
  "surfaces/settings/providers/sections/credentials/JetBrainsCreds.tsx": {
    owner: "045",
    reason: "bespoke IDE credential refresh",
    remove: "when JetBrains creds dispatch refresh",
  },
  "surfaces/settings/providers/sections/credentials/VertexAiCreds.tsx": {
    owner: "045",
    reason: "bespoke gcloud console link",
    remove: "when Vertex creds dispatch openExternalUsage",
  },
  "surfaces/settings/tokens/TokenAccountsPanel.tsx": {
    owner: "045",
    reason: "token-account login is a credential capture, not a surface action yet",
    remove: "when token accounts dispatch triggerLogin",
  },
};

const LEGACY_HOOKS = ["useProviders"];
const LEGACY_ALLOWLIST = {
  "surfaces/settings/pages/ProvidersPage.tsx": {
    owner: "045",
    reason: "migration adapter; refreshOnMount is false when core is present",
    remove: "when the page reads only the injected UsageStore",
  },
  "surfaces/settings/tabs/ProvidersTab.tsx": {
    owner: "045",
    reason: "migration adapter; refreshOnMount is false",
    remove: "when the tab reads only the injected UsageStore",
  },
};

function* walk(dir) {
  for (const entry of readdirSync(dir)) {
    const full = join(dir, entry);
    if (statSync(full).isDirectory()) {
      if (entry === "node_modules" || entry === "dist") continue;
      yield* walk(full);
    } else if (/\.(ts|tsx)$/.test(entry)) {
      yield full;
    }
  }
}

function relativePath(file, root) {
  return file.replace(/\\/g, "/").replace(root.replace(/\\/g, "/") + "/", "");
}

function isTestFile(rel) {
  return /\.(test|spec)\./.test(rel) || /\/test\//.test(rel);
}

export function checkCoreConsumption(root = srcRoot) {
  const failures = [];
  let sawSurfaceActionInvoke = false;
  let sawActivate = false;
  let sawActiveSurfaces = false;
  let sawFloatBarEntries = false;
  let sawKindFromWindowLabel = false;

  for (const file of walk(root)) {
    const rel = relativePath(file, root);
    const text = readFileSync(file, "utf8");
    const lines = text.split(/\r?\n/);

    if (text.includes("invokeSurfaceAction(") || text.includes('"surface_action"')) {
      sawSurfaceActionInvoke = true;
    }
    if (rel === "App.tsx" || rel === "appRuntime.ts") {
      if (text.includes("activateSurface") || text.includes("r.activate(") || text.includes(".activate(")) {
        sawActivate = true;
      }
      if (text.includes("activeSurfaces") || text.includes("listActiveSurfaces")) {
        sawActiveSurfaces = true;
      }
      if (text.includes("kindFromWindowLabel")) sawKindFromWindowLabel = true;
    }
    if (rel === "floatbar/FloatBar.tsx" && /floatBarEntries/.test(text)) {
      sawFloatBarEntries = true;
    }

    if (isTestFile(rel)) continue;

    for (const token of STORE_FACTORIES) {
      if (!text.includes(token)) continue;
      if (!FACTORY_ALLOWLIST[rel]) {
        failures.push(`${rel}: second runtime factory ${token.trim()} (not in allowlist)`);
      }
    }

    if (!LEGACY_ALLOWLIST[rel]) {
      for (const hook of LEGACY_HOOKS) {
        const re = new RegExp("from [\"'][^\"']*hooks/" + hook + "[\"']");
        if (re.test(text)) failures.push(`${rel}: imports legacy hook ${hook}`);
      }
    }

    for (let i = 0; i < lines.length; i++) {
      const line = lines[i].trim();
      if (line.indexOf("//") === 0 || line.indexOf("*") === 0) continue;
      for (const token of DIRECT_COMMANDS) {
        if (!line.includes(token)) continue;
        if (DIRECT_COMMAND_ALLOWLIST[rel]) continue;
        failures.push(`${rel}:${i + 1}: direct ${token} call`);
      }
    }
  }

  if (!sawSurfaceActionInvoke) {
    failures.push("missing production invokeSurfaceAction / surface_action path");
  }
  if (!sawActivate) {
    failures.push("SurfaceRegistry activateSurface has no production caller");
  }
  if (!sawActiveSurfaces) {
    failures.push("activeSurfaces() has no production caller");
  }
  if (!sawKindFromWindowLabel) {
    failures.push("App/runtime does not resolve surfaces through kindFromWindowLabel");
  }
  if (!sawFloatBarEntries) {
    failures.push("FloatBar.tsx does not consume floatBarEntries");
  }

  return failures;
}

function writeFixture(dir, rel, content) {
  const full = join(dir, rel);
  mkdirSync(dirname(full), { recursive: true });
  writeFileSync(full, content);
}

export function runNegativeFixtures() {
  const dir = mkdtempSync(join(tmpdir(), "core-guard-"));
  try {
    writeFixture(dir, "App.tsx", "export default function App() { return null }\n");
    writeFixture(dir, "core/usageStore.ts", "export function createUsageStore() {}\n");
    writeFixture(dir, "rogueStore.ts", "import { createUsageStore } from './core/usageStore'; createUsageStore();\n");
    writeFixture(dir, "surfaces/Bad.tsx", 'import { openSettingsWindow } from "../lib/tauri"; openSettingsWindow("general");\n');
    writeFixture(dir, "floatbar/FloatBar.tsx", "export default function FloatBar() { return null }\n");
    const failures = checkCoreConsumption(dir);
    const joined = failures.join("\n");
    const required = [
      "second runtime factory",
      "direct openSettingsWindow(",
      "missing production invokeSurfaceAction",
      "activateSurface has no production caller",
      "does not consume floatBarEntries",
    ];
    const missing = required.filter((needle) => !joined.includes(needle));
    if (missing.length > 0) {
      throw new Error("negative fixtures did not fail as expected: " + missing.join(", ") + "\n" + joined);
    }
  } finally {
    rmSync(dir, { recursive: true, force: true });
  }
}

const isMain = process.argv[1] && resolve(process.argv[1]) === fileURLToPath(import.meta.url);
if (isMain) {
  const failures = checkCoreConsumption(srcRoot);
  if (failures.length > 0) {
    console.error("[check-core-consumption] FAILED:");
    for (const f of failures) console.error("  - " + f);
    process.exit(1);
  }
  try {
    runNegativeFixtures();
  } catch (error) {
    console.error("[check-core-consumption] negative fixtures FAILED:");
    console.error(String(error instanceof Error ? error.message : error));
    process.exit(1);
  }
  console.log("[check-core-consumption] OK - single runtime, host, dispatcher wire, no surface bypass");
}
