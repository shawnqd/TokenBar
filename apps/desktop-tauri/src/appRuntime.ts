/**
 * App-level core runtime (CORE-05/07).
 *
 * One read-only projection replica + action dispatcher per WebView. Rust owns
 * the process refresh/enrichment writer and publishes a monotonically-versioned
 * full projection; this shell only subscribes and renders that projection.
 * `useCoreBridge` and `trayCoreStore` consume the same replica in this WebView
 * so tray, float bar, taskbar preview and Settings never create a writer.
 */
import { listen } from "@tauri-apps/api/event";
import {
  type UsageFetcher,
  type UsageStore,
  type UsageStoreKey,
} from "./core/usageStore";
import {
  buildProjectionCoordinator,
} from "./core/projectionRuntime";
import {
  buildProjectionEnrichmentScheduler,
} from "./core/projectionRuntime";
import type { RefreshCoordinator } from "./core/refreshCoordinator";
import type { EnrichmentScheduler } from "./core/enrichmentScheduler";
import {
  createActionDispatcher,
  type ActionDispatcher,
} from "./core/actionDispatcher";
import {
  setCoreBridgeStore,
  setCoreBridgeCoordinator,
  setCoreBridgeDispatcher,
} from "./core/useCoreBridge";
import { buildProjectionStore, type ProjectionStore } from "./core/projectionRuntime";
import { attachTrayCoreRuntime } from "./surfaces/tray/trayCoreStore";
import { attachFloatBarStore } from "./floatbar/floatBarStore";
import type { ProviderSnapshot } from "./core/snapshot";
import type {
  ProviderUsageSnapshot,
  VersionedProviderProjection,
} from "./types/bridge";
import { invokeSurfaceAction, getProviderProjection } from "./lib/tauri";
import {
  activateSurface,
  activeSurfaces as listActiveSurfaces,
  kindFromWindowLabel,
  type SurfaceDescriptor,
  type SurfaceKind,
} from "./core/surfaceRegistry";
import { recordRefreshTrace } from "./core/runtimeDiagnostics";

export interface AppRuntime {
  store: UsageStore;
  coordinator: RefreshCoordinator;
  scheduler: EnrichmentScheduler;
  dispatcher: ActionDispatcher;
  fetchProvider: UsageFetcher;
  seedFromBridge: (snapshots: ProviderUsageSnapshot[]) => void;
  refreshAll: (opts?: { force?: boolean }) => Promise<void>;
  activeSurfaces: (settings?: Record<string, unknown>) => SurfaceDescriptor[];
  activate: (kind: SurfaceKind, settings?: Record<string, unknown>) => Promise<void>;
  kindFromWindowLabel: typeof kindFromWindowLabel;
  dispose: () => void;
}

/**
 * Resolve one provider snapshot for a store key.
 *
 * - The backend cache is authoritative for everything the bridge exposes:
 *   we look up by providerId, then prefer the cached row whose
 *   accountEmail matches the store key; the Rust cache keeps one row
 *   per provider, so exact multi-account isolation is bounded by it.
 * - Replica fill only: never calls refresh_providers. Manual refresh goes
 *   through `surface_action` so Rust remains the process writer.
 * - Unknown data returns the record/error, never a fabricated percentage.
 */
async function fetchForKey(
  store: UsageStore,
  key: UsageStoreKey,
): Promise<ProviderSnapshot> {
  const record = store.get(key);
  if (record?.snapshot) return record.snapshot;
  throw new Error("core projection: no snapshot for " + key.providerId);
}

/**
 * Generation-domain note (review round 2, M5).
 *
 * UsageStore and RefreshCoordinator keep independent generation counters:
 * the coordinator bumps per refresh round, the store per upsert/seed. An
 * in-flight coordinator fetch resolving AFTER a fresh seed /
 * provider-updated event could overwrite the newer snapshot with the
 * older result.
 *
 * Accepted risk, no live repro observed: the fetcher re-reads the same
 * backend cache the seed used, the Rust side coalesces refetches, and
 * surfaces re-render from the store. Coupling the two counters would
 * serialize seeds, so this is documented instead.
 */

export function buildAppRuntime(): AppRuntime {
  const store = buildProjectionStore();
  const scheduler = buildProjectionEnrichmentScheduler();

  const dispatcher = createActionDispatcher(
    {},
    {
      fallback: async (action) => {
        const data = await invokeSurfaceAction(action);
        return { status: "handled", data };
      },
    },
  );

  const coordinator = buildProjectionCoordinator({
    store,
    dispatchRefresh: (opts) =>
      invokeSurfaceAction({ type: "refresh", force: opts?.force }),
    readProjection: getProviderProjection,
  });

  attachTrayCoreRuntime({ store, coordinator, scheduler });
  attachFloatBarStore(store);
  coordinator.on("started", recordRefreshTrace);
  coordinator.on("core-complete", recordRefreshTrace);
  coordinator.on("enrichment-complete", recordRefreshTrace);
  coordinator.on("failed", recordRefreshTrace);

  setCoreBridgeStore(store);
  setCoreBridgeCoordinator(coordinator);
  setCoreBridgeDispatcher(dispatcher);

  const runtime: AppRuntime = {
    store,
    coordinator,
    scheduler,
    dispatcher,
    fetchProvider: (key) => fetchForKey(store, key),
    seedFromBridge: (snapshots) => {
      // Compatibility entry for existing boot fixtures. Production boot and
      // events use the versioned full projection below.
      store.applyProjection({
        version: Math.max(0, store.projectionVersion() + 1),
        snapshots,
      });
    },
    activeSurfaces: () => listActiveSurfaces(),
    activate: (kind, settings) => activateSurface(kind, settings),
    kindFromWindowLabel,
    refreshAll: async () => {
      // `RefreshCoordinator.refreshAll([])` is intentionally a no-op for
      // selector callers. The app-level command has no key list, so dispatch
      // the process-owned refresh directly and then pull the new projection.
      await invokeSurfaceAction({ type: "refresh", force: true });
      try {
        (store as ProjectionStore).applyProjection(await getProviderProjection());
      } catch {
        // The projection event will repair this replica when the backend emits.
      }
    },
    dispose: () => {
      setCoreBridgeStore(null);
      setCoreBridgeCoordinator(null);
      setCoreBridgeDispatcher(null);
    },
  };
  return runtime;
}

let runtime: AppRuntime | null = null;
let stopListeners: Array<() => void> = [];

export function getAppRuntime(): AppRuntime {
  if (!runtime) runtime = buildAppRuntime();
  return runtime;
}

export function hasAppRuntime(): boolean {
  return runtime != null;
}

/** Seed the read-only replica from the process-owned full projection on boot. */
export async function seedAppRuntime(r: AppRuntime): Promise<void> {
  try {
    (r.store as ProjectionStore).applyProjection(await getProviderProjection());
  } catch {
    // backend not ready yet: surfaces render empty/unknown states
  }
}

/** Wire the process-owned full projection event into this WebView replica. */
export function startAppRuntimeWiring(r: AppRuntime): void {
  void listen<VersionedProviderProjection>("provider-projection-updated", (evt) => {
    try {
      (r.store as ProjectionStore).applyProjection(evt.payload);
    } catch {
      // malformed projection: ignore; the next full read repairs the replica
    }
  })
    .then((unlisten) => stopListeners.push(unlisten))
    .catch(() => {
      // running outside Tauri (tests/web): events unavailable, surfaces stay empty
    });
}

export function stopAppRuntimeWiring(): void {
  for (const fn of stopListeners) {
    try {
      fn();
    } catch {
      // ignore
    }
  }
  stopListeners = [];
}

export function disposeAppRuntime(): void {
  stopAppRuntimeWiring();
  if (runtime) {
    runtime.dispose();
    runtime = null;
  }
}

/** Idempotent boot entry used by App.tsx. */
export async function ensureAppRuntimeBooted(): Promise<AppRuntime> {
  const r = getAppRuntime();
  await seedAppRuntime(r).catch(() => {});
  if (stopListeners.length === 0) startAppRuntimeWiring(r);
  return r;
}
