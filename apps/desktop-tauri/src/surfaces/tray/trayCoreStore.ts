/**
 * Tray-domain read model over the unified core runtime.
 *
 * Pattern: store singleton + useSyncExternalStore selectors + manual refresh
 * commands (the floatBarStore pattern for this surface; that file is merged in
 * the main workspace but not in this worktree).
 *
 * The real Tauri fetcher/enrichment runner is injected by the app shell via
 * `setFetcher` / `setEnrichmentRunner`; until then the store starts empty and
 * every slot stays blank — no fake 100% bars, no static speed/balance. Chart
 * and output-speed data only appear after the enrichment scheduler runs the
 * injected runner and the runner commits a result.
 */
import { useCallback, useMemo, useSyncExternalStore } from "react";
import type {
  EnrichmentKind,
  EnrichmentScheduler,
  ProviderSnapshot,
  RefreshCoordinator,
  UsageFetcher,
  UsageRecord,
  UsageStore,
  UsageStoreKey,
} from "../../core";
import {
  usageStoreKey,
} from "../../core";
import type { ProviderChartData, ProviderOutputSpeed } from "../../types/bridge";

/** A committed enrichment result, keyed by kind + provider key. */
export interface TrayEnrichmentEntry {
  kind: EnrichmentKind;
  key: UsageStoreKey;
  at: number;
  ok: boolean;
  chartData?: ProviderChartData | null;
  outputSpeed?: ProviderOutputSpeed | null;
}

export interface TrayCoreStoreState {
  version: number;
  records: Readonly<Record<string, UsageRecord>>;
  enrich: Readonly<Record<string, TrayEnrichmentEntry>>;
}

export type EnrichmentRunner = (
  kind: EnrichmentKind,
  key: UsageStoreKey,
) => Promise<void>;

let store: UsageStore | null = null;
let coordinator: RefreshCoordinator | null = null;
let scheduler: EnrichmentScheduler | null = null;
let storeUnsub: (() => void) | null = null;

let fetcherSlot: UsageFetcher | undefined;
let runnerSlot: EnrichmentRunner | undefined;

const enrichByKey = new Map<string, TrayEnrichmentEntry>();
const listeners = new Set<() => void>();

let state: TrayCoreStoreState = {
  version: 0,
  records: Object.freeze({}),
  enrich: Object.freeze({}),
};

function rebuild(): void {
  state = {
    version: state.version + 1,
    records: coordinator?.getSnapshot().records ?? Object.freeze({}),
    enrich: Object.freeze({ ...Object.fromEntries(enrichByKey) }),
  };
  for (const listener of [...listeners]) {
    try {
      listener();
    } catch {
      // component listeners must not break the store
    }
  }
}

function isTestRuntime(): boolean {
  return (import.meta as { env?: { MODE?: string } }).env?.MODE === "test";
}

function assertTestRuntime(api: string): void {
  if (!isTestRuntime()) {
    throw new Error(`tray core ${api} is test-only`);
  }
}

/** Bind this read-model to the process-wide runtime. Production only. */
export function attachTrayCoreRuntime(deps: {
  store: UsageStore;
  coordinator: RefreshCoordinator;
  scheduler: EnrichmentScheduler;
}): void {
  if (storeUnsub) {
    storeUnsub();
    storeUnsub = null;
  }
  store = deps.store;
  coordinator = deps.coordinator;
  scheduler = deps.scheduler;
  enrichByKey.clear();
  storeUnsub = store.subscribe(rebuild);
  rebuild();
}

function getSnapshot(): TrayCoreStoreState {
  return state;
}

function getServerSnapshot(): TrayCoreStoreState {
  return state;
}

function subscribe(listener: () => void): () => void {
  listeners.add(listener);
  return () => {
    listeners.delete(listener);
  };
}

/** Read the store record for one provider key (coordinator view). */
function getRecord(key: UsageStoreKey): UsageRecord | undefined {
  return coordinator?.get(key);
}

function listRecords(): UsageRecord[] {
  if (!coordinator) return [];
  return Object.values(coordinator.getSnapshot().records);
}

function hasRecords(): boolean {
  return listRecords().some((record) => record.snapshot != null);
}

function refresh(
  key: UsageStoreKey,
  opts?: { manual?: boolean },
): Promise<UsageRecord> {
  if (!coordinator) {
    return Promise.reject(new Error("tray: core runtime not attached"));
  }
  return coordinator.refresh(key, opts);
}

async function refreshAll(
  keys: UsageStoreKey[],
  opts?: { manual?: boolean },
): Promise<void> {
  if (!coordinator || keys.length === 0) return;
  await coordinator.refreshAll(keys, opts);
  rebuild();
}

function setFetcher(fetcher: UsageFetcher | undefined): void {
  assertTestRuntime("setFetcher");
  fetcherSlot = fetcher;
}

function getFetcher(): UsageFetcher | undefined {
  return fetcherSlot;
}

function setEnrichmentRunner(runner: EnrichmentRunner | undefined): void {
  assertTestRuntime("setEnrichmentRunner");
  runnerSlot = runner;
}

function getEnrichmentRunner(): EnrichmentRunner | undefined {
  return runnerSlot;
}

async function triggerEnrichment(
  kind: EnrichmentKind,
  key: UsageStoreKey,
  opts?: { manual?: boolean; mode?: "minimal" | "compact" | "detailed" | "full" },
): Promise<boolean> {
  if (!scheduler) return false;
  const ok = await scheduler.trigger(kind, key, opts);
  rebuild();
  return ok;
}

/** The enriched runner commits its result here so surfaces can read it. */
function commitEnrichment(
  kind: EnrichmentKind,
  key: UsageStoreKey,
  result: { ok: boolean; chartData?: ProviderChartData | null; outputSpeed?: ProviderOutputSpeed | null },
): void {
  assertTestRuntime("commitEnrichment");
  commitRuntimeEnrichment(kind, key, result);
}

/**
 * Production commit path for the app-runtime enrichment runner. Only the
 * runtime wiring (appRuntime) may call this — surfaces stay read-only
 * consumers of the committed entries.
 */
function commitRuntimeEnrichment(
  kind: EnrichmentKind,
  key: UsageStoreKey,
  result: { ok: boolean; chartData?: ProviderChartData | null; outputSpeed?: ProviderOutputSpeed | null },
): void {
  const id = `${kind}::${usageStoreKey(key)}`;
  enrichByKey.set(id, {
    kind,
    key,
    at: Date.now(),
    ok: result.ok,
    chartData: result.chartData,
    outputSpeed: result.outputSpeed,
  });
  rebuild();
}

function getChartData(key: UsageStoreKey): ProviderChartData | null {
  const entry = enrichByKey.get(`chart::${usageStoreKey(key)}`);
  return entry?.ok ? (entry.chartData ?? null) : null;
}

function latestOutputSpeedFor(providerId: string): ProviderOutputSpeed | null {
  let best: TrayEnrichmentEntry | null = null;
  for (const entry of enrichByKey.values()) {
    if (
      entry.kind === "outputSpeed" &&
      entry.ok &&
      entry.outputSpeed != null &&
      entry.key.providerId === providerId
    ) {
      if (best == null || entry.at > best.at) best = entry;
    }
  }
  return best?.outputSpeed ?? null;
}

/** Provider keys whose record declares the output-speed capability. */
function speedCapableKeys(): UsageStoreKey[] {
  return listRecords()
    .filter(
      (record) =>
        record.snapshot?.capabilities.supportsOutputSpeed === true,
    )
    .map((record) => record.key);
}

/** Provider keys whose record declares chart or local-cost capability. */
function chartCapableKeys(): UsageStoreKey[] {
  return listRecords()
    .filter(
      (record) =>
        record.snapshot?.capabilities.supportsCharts === true ||
        record.snapshot?.capabilities.supportsLocalCost === true,
    )
    .map((record) => record.key);
}

/** Push a snapshot directly into the store (tests / cache seeding). */
function seed(snapshot: ProviderSnapshot): UsageRecord {
  assertTestRuntime("seed");
  if (!store) {
    throw new Error("tray: core projection not attached");
  }
  return store.upsert(snapshot);
}

function resetForTest(): void {
  throw new Error("tray: use resetTrayCoreForTest from trayCoreStore.testSupport");
}

export const trayCoreStore = {
  subscribe,
  getSnapshot,
  getServerSnapshot,
  getRecord,
  listRecords,
  hasRecords,
  refresh,
  refreshAll,
  setFetcher,
  getFetcher,
  setEnrichmentRunner,
  getEnrichmentRunner,
  triggerEnrichment,
  commitEnrichment,
  commitRuntimeEnrichment,
  getChartData,
  latestOutputSpeedFor,
  speedCapableKeys,
  chartCapableKeys,
  seed,
  resetForTest,
};


/* ── React selectors / commands ─────────────────────────────────────── */

export function useTrayCoreState(): TrayCoreStoreState {
  return useSyncExternalStore(subscribe, getSnapshot, getServerSnapshot);
}

export function useTrayCoreRecords(): UsageRecord[] {
  const trayState = useTrayCoreState();
  return useMemo(() => Object.values(trayState.records), [trayState.records]);
}

export function useTrayRefreshCommand(): (
  key: UsageStoreKey,
  opts?: { manual?: boolean },
) => Promise<UsageRecord> {
  return useCallback(
    (key: UsageStoreKey, opts?: { manual?: boolean }) =>
      trayCoreStore.refresh(key, opts),
    [],
  );
}

export function useTrayRefreshAllCommand(): (
  keys: UsageStoreKey[],
  opts?: { manual?: boolean },
) => Promise<void> {
  return useCallback(
    (keys: UsageStoreKey[], opts?: { manual?: boolean }) =>
      trayCoreStore.refreshAll(keys, opts),
    [],
  );
}

/** Committed chart/local-usage enrichment for one provider key. */
export function useTrayChartData(
  key: UsageStoreKey | undefined,
): ProviderChartData | null {
  const trayState = useTrayCoreState();
  void trayState;
  return useMemo(
    () => (key ? trayCoreStore.getChartData(key) : null),
    [trayState.version, key],
  );
}
