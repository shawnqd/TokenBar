import { useEffect, useMemo, useRef, useState } from "react";
import Sortable from "sortablejs";
import type { QuotaPercentContext } from "../lib/quotaDisplay";
import type { ProviderUsageSnapshot } from "../types/bridge";
import { ProviderIcon } from "./providers/ProviderIcon";
import { useLocale } from "../hooks/useLocale";
import { TokenBarIcon } from "./TokenBarIcon";

export default function ProviderGrid({
  providers,
  selectedProviderId,
  display,
  showProviderIcons = true,
  expanded,
  onExpandedChange,
  onSelect,
  onReorder,
  onGestureStart,
  onGestureEnd,
}: {
  providers: ProviderUsageSnapshot[];
  selectedProviderId: string | null;
  /** The owning surface's quota presentation choice. */
  display: QuotaPercentContext;
  showProviderIcons?: boolean;
  expanded?: boolean;
  onExpandedChange?: (expanded: boolean) => void;
  onSelect: (providerId: string | null) => void;
  /** Persist a new provider order (list of provider IDs) after a drag-reorder. */
  onReorder?: (orderedIds: string[]) => void;
  /** Called when an actual HTML5 drag starts (not for an ordinary click). */
  onGestureStart?: () => void;
  /** Called when an HTML5 drag ends or is canceled. */
  onGestureEnd?: () => void;
}) {
  const { t } = useLocale();
  const [uncontrolledExpanded, setUncontrolledExpanded] = useState(false);
  const canReorder = typeof onReorder === "function";
  const gridRef = useRef<HTMLDivElement>(null);

  useEffect(() => {
    if (!canReorder) return;
    const el = gridRef.current;
    if (!el) return;
    // 用成熟排序库 sortablejs（项目既有依赖），不自研动画：
    // 1:1 跟手 + 让位过渡 + 松手归位都由库实现。
    const sortable = Sortable.create(el, {
      animation: 150,
      draggable: ".provider-grid__item[data-provider-id]",
      handle: ".provider-grid__item[data-provider-id]",
      ghostClass: "provider-grid__item--ghost",
      chosenClass: "provider-grid__item--dragging",
      onStart: () => onGestureStart?.(),
      onEnd: () => onGestureEnd?.(),
      onUpdate: () => {
        const ordered = Array.from(
          el.querySelectorAll(':scope > .provider-grid__item[data-provider-id]'),
        ).map((node) => (node as HTMLElement).getAttribute("data-provider-id") ?? "");
        const nextVisible = ordered.filter(Boolean);
        if (!onReorder) return;
        const full = providers.map((p) => p.providerId);
        if (full.length === nextVisible.length) {
          onReorder(nextVisible);
          return;
        }
        const visibleSet = new Set(nextVisible);
        const merged: string[] = [];
        let vi = 0;
        for (const id of full) {
          if (visibleSet.has(id)) merged.push(nextVisible[vi++] ?? id);
          else merged.push(id);
        }
        onReorder(merged);
      },
    });
    return () => sortable.destroy();
  }, [canReorder, onGestureStart, onGestureEnd, providers, onReorder]);
  const isExpanded = expanded ?? uncontrolledExpanded;
  const setExpanded = (next: boolean) => {
    if (expanded === undefined) setUncontrolledExpanded(next);
    onExpandedChange?.(next);
  };
  // Display context is for cards below the grid; the icon strip no longer
  // paints per-tile quota bars (removed at user request).
  void display;
  const totalItems = providers.length + 1;
  const shouldCollapse = totalItems > 32;
  const collapsedProviders = useMemo(
    () => prioritizeProviders(providers, selectedProviderId),
    [providers, selectedProviderId],
  );
  const visibleProviders =
    shouldCollapse && !isExpanded
      ? collapsedProviders.slice(0, 18)
      : providers;
  const hiddenCount = Math.max(0, providers.length - visibleProviders.length);
  const densityClass =
    totalItems <= 6
      ? " provider-grid--sparse"
      : shouldCollapse
        ? " provider-grid--compact"
        : "";
  const labelFor = (name: string) =>
    densityClass.includes("compact") ? compactGridLabel(name) : name;

  return (
    <div
      ref={gridRef}
      className={`provider-grid${densityClass}${showProviderIcons ? "" : " provider-grid--no-icons"}`}
      data-provider-count={totalItems}
      data-expanded={isExpanded ? "true" : "false"}
      data-show-icons={showProviderIcons ? "true" : "false"}
    >
      <button
        type="button"
        className={`provider-grid__item${selectedProviderId === null ? " provider-grid__item--active" : ""}`}
        onClick={() => onSelect(null)}
        aria-label={t("PanelAllProviders")}
      >
        {showProviderIcons && (
          <span className="provider-grid__icon-overview provider-grid__icon-overview--brand">
            <TokenBarIcon size={20} />
          </span>
        )}
        <span className="provider-grid__label">{t("PanelAllProvidersShort")}</span>
      </button>
      {visibleProviders.map((p) => (
        <button
          key={p.providerId}
          type="button"
          data-provider-id={p.providerId}
          className={`provider-grid__item${p.providerId === selectedProviderId ? " provider-grid__item--active" : ""}`}
          onClick={() => onSelect(p.providerId)}
          aria-label={p.displayName}
        >
          {showProviderIcons && <ProviderIcon providerId={p.providerId} size={16} />}
          <span className="provider-grid__label">{labelFor(p.displayName)}</span>
        </button>
      ))}
      {shouldCollapse && (
        <button
          type="button"
          className="provider-grid__item provider-grid__item--more"
          onClick={() => setExpanded(!isExpanded)}
          aria-label={isExpanded ? t("PanelShowFewerProviders") : t("PanelShowAllProviders")}
          aria-expanded={isExpanded}
        >
          {showProviderIcons && (
            <span className="provider-grid__icon-overview" aria-hidden>
              {isExpanded ? "−" : "+"}
            </span>
          )}
          <span className="provider-grid__label">
            {isExpanded ? t("PanelShowFewerProviders") : `+${hiddenCount}`}
          </span>
        </button>
      )}
    </div>
  );
}

export function prioritizeProviders(
  providers: ProviderUsageSnapshot[],
  selectedProviderId: string | null,
): ProviderUsageSnapshot[] {
  if (!selectedProviderId) return providers;
  const selectedIndex = providers.findIndex((provider) => provider.providerId === selectedProviderId);
  if (selectedIndex < 0 || selectedIndex < 18) return providers;
  const selected = providers[selectedIndex];
  return [selected, ...providers.slice(0, selectedIndex), ...providers.slice(selectedIndex + 1)];
}

function compactGridLabel(displayName: string): string {
  const clean = displayName.replace(/[._-]+/g, " ").replace(/\s+/g, " ").trim();
  if (clean.length <= 5) return clean;

  const words = clean.split(" ").filter(Boolean);
  const first = words[0] ?? clean;
  if (words.length > 1) {
    if (first.length <= 3 && /\d|^[A-Z]+$/.test(first)) return first;
    const initials = words
      .slice(0, 2)
      .map((word) => word[0]?.toUpperCase() ?? "")
      .join("");
    if (initials.length >= 2) return initials;
  }

  const capitals = clean.match(/[A-Z0-9]/g);
  if (capitals && capitals.length >= 2 && capitals.length <= 4) {
    return capitals.join("");
  }

  return clean.slice(0, 4);
}
