import { useCallback, useEffect, useMemo, useState, useSyncExternalStore } from "react";
import type {
  ProviderCatalogEntry,
  SettingsUpdate,
} from "../../../types/bridge";
import type { BootstrapState } from "../../../types/bridge";
import { useLocale } from "../../../hooks/useLocale";
import type { LocaleKey } from "../../../i18n/keys";
import {
  ProvidersSidebar,
  type ProviderSidebarRow,
  type ProviderSidebarStatus,
} from "../providers/ProvidersSidebar";
import { ProviderDetailPane } from "../providers/ProviderDetailPane";
import { CookieFileImport, COOKIE_IMPORT_ID } from "../providers/CookieFileImport";
import { quotaDisplayContext } from "../../../lib/quotaDisplay";
import { formatRelativeUpdated } from "../../../lib/relativeTime";

import type { ProviderSnapshot } from "../../../core/snapshot";
import { projectSurface } from "../../../core/projection";
import type { UsageStore } from "../../../core/usageStore";
import { useActionDispatcher } from "../../../core/useCoreBridge";
import type { ActionDispatcher } from "../../../core/actionDispatcher";

interface ProvidersTabProps {
  settings: BootstrapState["settings"];
  providers: ProviderCatalogEntry[];
  set: (patch: SettingsUpdate) => void;
  saving: boolean;
  /** Injected core store. Empty/missing store renders loading, not fabricated values. */
  coreStore?: UsageStore | null;
  /** Injected dispatcher for `openProviderDetail`. Selection stays local; the
   *  dispatcher is a side-effect so other surfaces can observe the intent. */
  dispatcher?: ActionDispatcher | null;
}

const EMPTY_STORE_STATE_TAB: { version: number; records: Record<string, unknown> } = {
  version: 0,
  records: {},
};
function useCoreSnapshotList(store?: UsageStore | null): ProviderSnapshot[] {
  const subscribe = useCallback(
    (cb: () => void) => (store ? store.subscribe(cb) : () => {}),
    [store],
  );
  const getSnapshot = useCallback(() => {
    if (!store) return EMPTY_STORE_STATE_TAB;
    return store.getSnapshot();
  }, [store]);
  const getServerSnapshot = useCallback(() => getSnapshot(), [getSnapshot]);
  // useSyncExternalStore requires stable subscribe/getSnapshot; empty store is stable.
  const state = useSyncExternalStore(subscribe, getSnapshot as () => never, getServerSnapshot as () => never) as unknown as { records: Record<string, { snapshot: ProviderSnapshot | null }> };
  return useMemo(() => {
    const vals = Object.values(state.records);
    const out: ProviderSnapshot[] = [];
    for (const rec of vals) if (rec.snapshot) out.push(rec.snapshot);
    return out;
  }, [state]);
}

export default function ProvidersTab({
  settings,
  providers,
  set,
  saving,
  coreStore,
  dispatcher,
}: ProvidersTabProps) {
  const { t } = useLocale();
  const coreSnapshots = useCoreSnapshotList(coreStore);
  const coreMap = useMemo(() => new Map(coreSnapshots.map((s) => [s.providerId, s])), [coreSnapshots]);

  const actionDispatcher = useActionDispatcher(dispatcher ?? undefined);
  const [selectedId, setSelectedId] = useState<string | null>(
    providers[0]?.id ?? null,
  );
  const handleSelect = useCallback(
    (id: string) => {
      // Dispatch openProviderDetail for cross-surface observability; the local
      // selected state remains the source of truth for the detail pane so the
      // UI works without a round-trip through Tauri.
      const d = actionDispatcher as unknown as ActionDispatcher;
      if (d && typeof d.dispatch === "function") {
        void d.dispatch({ type: "openProviderDetail", target: { kind: "provider", providerId: id } } as unknown as Parameters<typeof d.dispatch>[0]).catch(() => {});
      }
      setSelectedId(id);
    },
    [actionDispatcher],
  );
  // Locally-owned catalog order so drag-reorder feels instant before the
  // backend `reorder_providers` round-trip settles.
  const [orderedProviders, setOrderedProviders] =
    useState<ProviderCatalogEntry[]>(providers);
  const [searchText, setSearchText] = useState("");

  useEffect(() => {
    setOrderedProviders(providers);
  }, [providers]);

  const enabled = useMemo(
    () => new Set(settings.enabledProviders),
    [settings.enabledProviders],
  );

  const toggle = (id: string, on: boolean) => {
    const next = new Set(enabled);
    if (on) next.add(id);
    else next.delete(id);
    set({
      enabledProviders: orderedProviders
        .map((provider) => provider.id)
        .filter((providerId) => next.has(providerId)),
    });
  };

  const rows: ProviderSidebarRow[] = useMemo(() => {
    return orderedProviders.map((p) => {
      const isOn = enabled.has(p.id);
      const snap = coreMap.get(p.id) ?? null;
      return {
        id: p.id,
        displayName: p.displayName,
        enabled: isOn,
        status: deriveProviderStatusFromCore(isOn, snap),
        subtitlePrimary: providerSidebarSubtitleFromCore(p.id, isOn, snap, t),
        subtitleSecondary: providerSidebarMetricFromCore(
          snap,
          settings.dashboardShowAsUsed !== false,
        ),
      };
    });
  }, [enabled, orderedProviders, t, coreMap, settings.dashboardShowAsUsed]);

  const normalizedSearch = searchText.trim().toLowerCase();
  const visibleRows = useMemo(
    () =>
      normalizedSearch
        ? rows.filter((row) => {
            const name = row.displayName.toLowerCase();
            const id = row.id.toLowerCase();
            return name.includes(normalizedSearch) || id.includes(normalizedSearch);
          })
        : rows,
    [normalizedSearch, rows],
  );

  useEffect(() => {
    // The pinned "批量导入 Cookie" row is a sentinel selection, not a real
    // (filterable) provider row — never auto-replace it with a provider.
    if (selectedId === COOKIE_IMPORT_ID) return;
    if (visibleRows.length === 0) {
      if (selectedId !== null) setSelectedId(null);
      return;
    }
    if (!selectedId || !visibleRows.some((row) => row.id === selectedId)) {
      setSelectedId(visibleRows[0].id);
    }
  }, [selectedId, visibleRows]);

  const handleReorder = (ids: string[]) => {
    const byId = new Map(orderedProviders.map((p) => [p.id, p]));
    const nextIds = normalizedSearch
      ? mergeFilteredOrder(
          orderedProviders.map((p) => p.id),
          new Set(visibleRows.map((row) => row.id)),
          ids,
        )
      : ids;
    const next = nextIds
      .map((id) => byId.get(id))
      .filter((p): p is ProviderCatalogEntry => Boolean(p));
    setOrderedProviders(next);
    void (actionDispatcher as ActionDispatcher)
      .dispatch({
        type: "reorderProviders",
        target: { kind: "summary" },
        providerIds: nextIds,
      })
      .catch(() => {
        setOrderedProviders(providers);
      });
  };

  // During bootstrap the catalog can arrive one render before the selection
  // effect runs. Use the first visible row as the transient detail target so
  // the right workspace never flashes an empty pane on first open.
  const detailProviderId = selectedId ?? visibleRows[0]?.id ?? null;
  const selectedEntry =
    orderedProviders.find((p) => p.id === detailProviderId) ?? null;
  const selectedCoreSnapshot = coreMap.get(detailProviderId ?? "") ?? null;

  return (
    <div className="providers-tab-content">
      <div className="provider-split">
        <ProvidersSidebar
          providers={visibleRows}
          selectedId={selectedId ?? detailProviderId}
          searchText={searchText}
          onSearchTextChange={setSearchText}
          onSelect={handleSelect}
          onReorder={handleReorder}
          onToggleEnabled={toggle}
          disabled={saving}
        />
        {selectedId === COOKIE_IMPORT_ID ? (
          <CookieFileImport />
        ) : (
          <ProviderDetailPane
            key={detailProviderId ?? "no-provider"}
            providerId={detailProviderId}
            providerSnapshot={null}
            coreSnapshot={selectedCoreSnapshot}
            cookieDomain={selectedEntry?.cookieDomain ?? null}
            display={quotaDisplayContext(settings, "dashboard")}
            localUsagePeriod={settings.localUsagePeriod ?? "7d"}
            outputSpeedEnabled={settings.outputSpeedEnabled !== false}
            providerMetrics={settings.providerMetrics}
            settingsDisabled={saving}
            onSettingsChange={set}
          />
        )}
      </div>
    </div>
  );
}

function mergeFilteredOrder(
  fullOrder: string[],
  visibleIds: Set<string>,
  reorderedVisibleIds: string[],
): string[] {
  const nextVisible = [...reorderedVisibleIds];
  return fullOrder.map((id) =>
    visibleIds.has(id) ? (nextVisible.shift() ?? id) : id,
  );
}

// ── Provider sidebar subtitle helpers (port of
//    rust/src/native_ui/preferences.rs::provider_sidebar_subtitle). ─────

function deriveProviderStatusFromCore(
  isEnabled: boolean,
  snap: ProviderSnapshot | null,
): ProviderSidebarStatus {
  if (!isEnabled) return "disabled";
  if (!snap) return "loading";
  if (snap.displayState === "error" || snap.displayState === "authRequired") return "error";
  if (snap.displayState === "stale") return "stale";
  if (snap.displayState === "loading" || snap.displayState === "unknown") return "loading";
  return "ok";
}

function providerSidebarSubtitleFromCore(
  _providerId: string,
  isEnabled: boolean,
  snap: ProviderSnapshot | null,
  t: (key: LocaleKey) => string,
): string {
  if (!isEnabled) return t("ProviderDisabled");
  if (!snap || snap.error) return "未配置";
  // Core snapshot carries sourceLabel (auto/browser/manual etc) as sourceLabel
  const updatedMs = snap.updatedAt ? Date.parse(snap.updatedAt) : NaN;
  const time = Number.isFinite(updatedMs)
    ? formatRelativeUpdated(updatedMs, t)
    : t("UpdatedJustNow");
  return `已登录 · ${time}`;
}

function providerSidebarMetricFromCore(
  snap: ProviderSnapshot | null,
  showAsUsed: boolean,
): string | undefined {
  if (!snap) return undefined;
  const proj = projectSurface(snap, { showAsUsed });
  if (proj.primary) {
    if (proj.primary.fillPercent != null) return `${Math.round(proj.primary.fillPercent)}%`;
  }
  if (proj.balance) return proj.balance.amountText;
  return undefined;
}
