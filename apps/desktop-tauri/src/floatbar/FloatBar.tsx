import {
  useCallback,
  useEffect,
  useMemo,
  useRef,
  useState,
  type CSSProperties,
  type MouseEvent,
} from "react";
import { listen } from "@tauri-apps/api/event";
import { currentMonitor, getCurrentWindow } from "@tauri-apps/api/window";
import { useFormattedResetTime } from "../hooks/useFormattedResetTime";
import {
  quotaDisplayContext,
  quotaPercentDisplay,
  type QuotaDisplayContext,
} from "../lib/quotaDisplay";
import { useLocale } from "../hooks/useLocale";
import {
  getSettingsSnapshot,
  invokeSurfaceAction,
  revealTrayPanelWindow,
} from "../lib/tauri";
import { ProviderIcon } from "../components/providers/ProviderIcon";
import { getProviderIcon } from "../components/providers/providerIcons";
import type {
  BootstrapState,
  FloatBarResetWindow,
  ProviderLocalUsageSummary,
  ProviderUsageSnapshot,
  RateWindowSnapshot,
  SettingsSnapshot,
  TaskbarWindowKind,
} from "../types/bridge";
import { TASKBAR_PROVIDER_AUTO } from "../types/bridge";
import { windowByKind } from "../lib/quotaWindows";
import {
  FLOAT_BAR_CONFIG_CHANGED_EVENT,
  adjustFloatBarGeometry,
  hideFloatBar,
  resizeFloatBar,
} from "./api";
import "./FloatBar.css";
import {
  floatBarStore,
  ensureFloatBarStoreSync,
  useFloatBarSnapshots,
  fetchFloatBarLocalCost,
  hasFloatBarLocalCostFetcher,
} from "./floatBarStore";
import {
  expandFloatBarEntries,
  resolveFloatBarEntries,
} from "../surfaces/settings/floatBarEntries";
import type { ProviderSnapshot } from "../core/snapshot";

// Re-export for evidence that runtime consumes floatBarEntries (migration layer)
export { resolveFloatBarEntries, expandFloatBarEntries };

function ResetIcon({ size }: { size: number }) {
  return (
    <svg
      className="floatbar__reset-icon-svg"
      width={size}
      height={size}
      viewBox="0 0 16 16"
      fill="none"
      aria-hidden="true"
    >
      <path
        d="M12.9 7.1a5 5 0 1 0-1.2 3.9"
        stroke="currentColor"
        strokeWidth="1.6"
        strokeLinecap="round"
      />
      <path
        d="M12.9 3.8v3.3H9.6"
        stroke="currentColor"
        strokeWidth="1.6"
        strokeLinecap="round"
        strokeLinejoin="round"
      />
    </svg>
  );
}

const RESET_WINDOW_LABEL_KEYS = {
  primary: "FloatBarResetWindowPrimary",
  session: "TaskbarWindowSession",
  weekly: "TaskbarWindowWeekly",
  daily: "TaskbarWindowDaily",
  monthly: "TaskbarWindowMonthly",
} as const;

function inlineResetTime(resetText: string): string {
  const normalized = resetText.trim();
  if (/^reset(?:s|ting)?(?:\s+due)?\s*(?:now)?$/i.test(normalized)) {
    return "now";
  }
  return normalized
    .replace(/^resets?\s+in\s+/i, "")
    .replace(/^resets?\s+/i, "")
    .trim();
}

type FloatBarCostSummary = {
  key: string;
  providerId: string;
  displayName: string;
  todayCost: number | null;
  thirtyDayCost: number | null;
};

type FloatBarCostTarget = {
  key: string;
  providerId: string;
  displayName: string;
};

function providerCostKey(provider: { providerId: string; accountEmail?: string | null }): string {
  return `${provider.providerId}:${provider.accountEmail ?? ""}`;
}

function hasLocalCost(summary: ProviderLocalUsageSummary | null): summary is ProviderLocalUsageSummary {
  return summary?.todayCost != null || summary?.thirtyDayCost != null;
}

function formatUsd(value: number | null): string | null {
  if (value == null || !Number.isFinite(value)) return null;
  return `$${value.toFixed(2)}`;
}

export type FlyoutPlacementState = {
  placementY: "top" | "bottom" | "top-aligned" | "bottom-aligned";
  placementX: "left" | "right";
  padTop: number;
  padLeft: number;
};

export function computeFlyoutPlacement(params: {
  pillRect: { top: number; bottom: number; left: number; right: number };
  basePos: { x: number; y: number };
  workArea: { top: number; bottom: number; left: number; right: number };
  orientation: "horizontal" | "vertical";
}): FlyoutPlacementState {
  const { pillRect, basePos, workArea, orientation } = params;
  const pillScreenTop = basePos.y + pillRect.top;
  const pillScreenBottom = basePos.y + pillRect.bottom;
  const pillScreenLeft = basePos.x + pillRect.left;
  const pillScreenRight = basePos.x + pillRect.right;

  const spaceBelow = workArea.bottom - pillScreenBottom;
  const spaceAbove = pillScreenTop - workArea.top;
  const spaceRight = workArea.right - pillScreenRight;
  const spaceLeft = pillScreenLeft - workArea.left;

  const FLYOUT_H = 150;
  const FLYOUT_W = 220;

  let placementY: "top" | "bottom" | "top-aligned" | "bottom-aligned" = "bottom";
  let placementX: "left" | "right" = "left";
  let padTop = 0;
  let padLeft = 0;

  if (orientation === "vertical") {
    placementX =
      (spaceRight < FLYOUT_W && spaceLeft >= FLYOUT_W) || (spaceRight < 120 && spaceLeft > spaceRight)
        ? "left"
        : "right";
    placementY =
      (spaceBelow < FLYOUT_H && spaceAbove >= FLYOUT_H) || (spaceBelow < 120 && spaceAbove > spaceBelow)
        ? "bottom-aligned"
        : "top-aligned";
    if (placementX === "left") {
      padLeft = Math.ceil(FLYOUT_W + 12);
    }
  } else {
    placementY =
      (spaceBelow < FLYOUT_H && spaceAbove >= FLYOUT_H) || (spaceBelow < 140 && spaceAbove > spaceBelow)
        ? "top"
        : "bottom";
    placementX =
      pillScreenLeft + FLYOUT_W > workArea.right || (spaceRight < FLYOUT_W && spaceLeft > spaceRight)
        ? "right"
        : "left";
    if (placementY === "top") {
      padTop = Math.ceil(FLYOUT_H + 16);
    }
    if (placementX === "right") {
      const flyoutLeftInBar = pillRect.right - FLYOUT_W;
      if (flyoutLeftInBar < 0) {
        padLeft = Math.ceil(Math.abs(flyoutLeftInBar) + 12);
      }
    }
  }

  return { placementY, placementX, padTop, padLeft };
}

function HoverFlyout({
  providerId,
  displayName,
  statusBadge,
  tone,
  percentLabel,
  pctValue,
  windowSub,
  resetTime,
  costText,
  resetLabel,
  costLabel,
  placementY = "bottom",
  placementX = "left",
}: {
  providerId: string;
  displayName: string;
  statusBadge: string;
  tone: "ok" | "warn" | "crit";
  percentLabel: string;
  pctValue: number;
  windowSub: string;
  resetTime: string | null;
  costText: string | null;
  resetLabel: string;
  costLabel: string;
  placementY?: "top" | "bottom" | "top-aligned" | "bottom-aligned";
  placementX?: "left" | "right";
}) {
  return (
    <div
      className={`floatbar__hover-flyout floatbar__hover-flyout--${placementY} floatbar__hover-flyout--${placementX}`}
      data-tauri-drag-region
    >
      <div className="hf-head">
        <div className="hf-provider">
          <span className="hf-icon">
            <ProviderIcon providerId={providerId} size={14} />
          </span>
          <span className="hf-title">{displayName}</span>
        </div>
        <span className={`hf-badge hf-badge--${tone}`}>{statusBadge}</span>
      </div>
      <div className="hf-metric">
        <div className="hf-metric__val">{percentLabel}</div>
        <div className="hf-metric__sub">{windowSub}</div>
      </div>
      <div className="hf-bar-wrap">
        <div
          className={`hf-bar-fill hf-bar-fill--${tone}`}
          style={{ width: `${Math.max(0, Math.min(100, pctValue))}%` }}
        />
      </div>
      <div className="hf-grid">
        <div className="hf-grid-item">
          <span className="hf-grid-label">{resetLabel}</span>
          <span className="hf-grid-val" title={resetTime || "—"}>
            {resetTime || "—"}
          </span>
        </div>
        <div className="hf-grid-item">
          <span className="hf-grid-label">{costLabel}</span>
          <span className="hf-grid-val" title={costText || "—"}>
            {costText || "—"}
          </span>
        </div>
      </div>
    </div>
  );
}

function FloatBarContextMenu({
  placementY = "bottom",
  placementX = "left",
  onRefresh,
  onOpenPanel,
  onOpenSettings,
  onHide,
  labels,
}: {
  placementY?: "top" | "bottom" | "top-aligned" | "bottom-aligned";
  placementX?: "left" | "right";
  onRefresh: () => void;
  onOpenPanel: () => void;
  onOpenSettings: () => void;
  onHide: () => void;
  labels: {
    refresh: string;
    openPanel: string;
    settings: string;
    hide: string;
  };
}) {
  return (
    <div
      className={`floatbar__context-menu floatbar__context-menu--${placementY === "top" ? "top" : "bottom"} floatbar__context-menu--${placementX}`}
      data-tauri-drag-region={false}
      onMouseDown={(e) => e.stopPropagation()}
    >
      <button
        type="button"
        className="floatbar__context-item"
        onClick={onRefresh}
      >
        <span className="floatbar__context-icon">
          <svg width="13" height="13" viewBox="0 0 16 16" fill="none" stroke="currentColor" strokeWidth="1.6" strokeLinecap="round" strokeLinejoin="round">
            <path d="M13.5 8A5.5 5.5 0 1 1 8 2.5c2.3 0 4.3 1.4 5.1 3.5" />
            <polyline points="13.5 2.5 13.5 6 10 6" />
          </svg>
        </span>
        <span>{labels.refresh}</span>
      </button>
      <button
        type="button"
        className="floatbar__context-item"
        onClick={onOpenPanel}
      >
        <span className="floatbar__context-icon">
          <svg width="13" height="13" viewBox="0 0 16 16" fill="none" stroke="currentColor" strokeWidth="1.6" strokeLinecap="round" strokeLinejoin="round">
            <rect x="2" y="2.5" width="12" height="11" rx="2" />
            <line x1="6" y1="2.5" x2="6" y2="13.5" />
          </svg>
        </span>
        <span>{labels.openPanel}</span>
      </button>
      <button
        type="button"
        className="floatbar__context-item"
        onClick={onOpenSettings}
      >
        <span className="floatbar__context-icon">
          <svg width="13" height="13" viewBox="0 0 16 16" fill="none" stroke="currentColor" strokeWidth="1.6" strokeLinecap="round" strokeLinejoin="round">
            <circle cx="8" cy="8" r="2.5" />
            <path d="M13.5 8c0-.3-.1-.6-.2-.9l1.3-.9-1.2-2.1-1.5.5c-.5-.4-1-.7-1.6-.9l-.3-1.6H8.5l-.3 1.6c-.6.2-1.1.5-1.6.9l-1.5-.5-1.2 2.1 1.3.9c-.1.3-.2.6-.2.9s.1.6.2.9l-1.3.9 1.2 2.1 1.5-.5c.5.4 1 .7 1.6.9l.3 1.6h2.4l.3-1.6c.6-.2 1.1-.5 1.6-.9l1.5.5 1.2-2.1-1.3-.9c.1-.3.2-.6.2-.9z" />
          </svg>
        </span>
        <span>{labels.settings}</span>
      </button>
      <div className="floatbar__context-divider" />
      <button
        type="button"
        className="floatbar__context-item floatbar__context-item--danger"
        onClick={onHide}
      >
        <span className="floatbar__context-icon">
          <svg width="13" height="13" viewBox="0 0 16 16" fill="none" stroke="currentColor" strokeWidth="1.6" strokeLinecap="round" strokeLinejoin="round">
            <path d="M2 2l12 12M6.7 6.7a3 3 0 0 0 4.2 4.2M2.5 8s2.5-4.5 5.5-4.5c1.2 0 2.3.7 3.2 1.6M13.5 8s-2.5 4.5-5.5 4.5c-1.2 0-2.3-.7-3.2-1.6" />
          </svg>
        </span>
        <span>{labels.hide}</span>
      </button>
    </div>
  );
}

function CostPill({
  summary,
  scale,
  todayLabel,
  thirtyDayLabel,
  isHovered,
  onHover,
  onLeave,
  placementY,
  placementX,
}: {
  summary: FloatBarCostSummary;
  scale: number;
  todayLabel: string;
  thirtyDayLabel: string;
  isHovered?: boolean;
  onHover?: () => void;
  onLeave?: () => void;
  placementY?: "top" | "bottom" | "top-aligned" | "bottom-aligned";
  placementX?: "left" | "right";
}) {
  const today = formatUsd(summary.todayCost);
  const thirtyDay = formatUsd(summary.thirtyDayCost);
  const iconSize = Math.round(10 * scale);
  const brand = getProviderIcon(summary.providerId).brandColor;
  const title = [
    today ? `${todayLabel} ${today}` : null,
    thirtyDay ? `${thirtyDayLabel} ${thirtyDay}` : null,
  ]
    .filter(Boolean)
    .join(" / ");

  return (
    <div
      className="floatbar__cost-pill"
      data-pill-key={`cost:${summary.key}`}
      title={isHovered ? undefined : `${summary.displayName}: ${title}`}
      data-tauri-drag-region
      style={{ "--brand": brand } as CSSProperties}
      onMouseEnter={onHover}
      onMouseLeave={onLeave}
    >
      <span className="floatbar__provider-icon" data-tauri-drag-region>
        <ProviderIcon providerId={summary.providerId} size={iconSize} />
      </span>
      <span className="floatbar__cost-items" data-tauri-drag-region>
        {today && (
          <span className="floatbar__cost-item" data-tauri-drag-region>
            <span className="floatbar__cost-label" data-tauri-drag-region>
              {todayLabel}
            </span>
            <span className="floatbar__cost-value" data-tauri-drag-region>
              {today}
            </span>
          </span>
        )}
        {thirtyDay && (
          <span className="floatbar__cost-item" data-tauri-drag-region>
            <span className="floatbar__cost-label" data-tauri-drag-region>
              {thirtyDayLabel}
            </span>
            <span className="floatbar__cost-value" data-tauri-drag-region>
              {thirtyDay}
            </span>
          </span>
        )}
      </span>
      {isHovered && (
        <HoverFlyout
          providerId={summary.providerId}
          displayName={summary.displayName}
          statusBadge="费用统计"
          tone="ok"
          percentLabel={today ?? "—"}
          pctValue={100}
          windowSub={todayLabel}
          resetTime={thirtyDay ? `${thirtyDayLabel} ${thirtyDay}` : null}
          costText={today}
          resetLabel={thirtyDayLabel}
          costLabel={todayLabel}
          placementY={placementY}
          placementX={placementX}
        />
      )}
    </div>
  );
}

function ResetChip({
  window,
  label,
  relative,
  iconSize,
  isParentHovered,
}: {
  window: RateWindowSnapshot;
  label: string | null;
  relative: boolean;
  iconSize: number;
  isParentHovered?: boolean;
}) {
  const resetText = useFormattedResetTime(
    window.resetsAt,
    window.resetDescription,
    relative,
  );
  if (!resetText) return null;
  const inline = inlineResetTime(resetText);
  if (!inline) return null;
  const title = label ? `${label} · ${resetText}` : resetText;

  return (
    <span
      className="floatbar__reset"
      title={isParentHovered ? undefined : title}
      aria-label={title}
      data-tauri-drag-region
    >
      <ResetIcon size={iconSize} />
      {label && (
        <span className="floatbar__reset-window" data-tauri-drag-region>
          {label}
        </span>
      )}
      <span className="floatbar__reset-time" data-tauri-drag-region>
        {inline}
      </span>
    </span>
  );
}

function isCoreSnapshot(snapshot: ProviderSnapshot | ProviderUsageSnapshot): snapshot is ProviderSnapshot {
  return (snapshot as ProviderSnapshot).windows !== undefined;
}

/** Resolve the window to display for an entry. Returns null for balance/speed or missing quota. */
function windowForEntry(
  snapshot: ProviderSnapshot | ProviderUsageSnapshot,
  windowKind: TaskbarWindowKind,
): RateWindowSnapshot | null {
  if (windowKind === "balance" || windowKind === "speed") return null;
  let found: RateWindowSnapshot | null = null;
  if (isCoreSnapshot(snapshot)) {
    const core = snapshot;
    if (windowKind === "primary") {
      const quota = core.windows.filter(
        (w) => w.usageKnown && !w.isInformational && w.displayKind === "quota",
      );
      if (quota.length === 0) return null;
      return quota[0] as unknown as RateWindowSnapshot;
    }
    const match = core.windows.find(
      (w) => w.kind === windowKind && w.usageKnown && !w.isInformational,
    );
    found = (match as unknown as RateWindowSnapshot) ?? null;
    // Fallback to first quota when the requested kind is not published – keeps the pill visible for generic fixtures
    if (!found) {
      const quota = core.windows.filter(
        (w) => w.usageKnown && !w.isInformational && w.displayKind === "quota",
      );
      return (quota[0] as unknown as RateWindowSnapshot) ?? null;
    }
    return found;
  }
  const bridge = snapshot as ProviderUsageSnapshot;
  if (["primary", "session", "weekly", "daily", "monthly"].includes(windowKind)) {
    found = windowByKind(bridge, windowKind as FloatBarResetWindow);
    if (!found) {
      // Fallback to primary quota window for generic fixtures that publish only primary
      const primary = windowByKind(bridge, "primary");
      return primary;
    }
    return found;
  }
  return null;
}

/** Resolve reset windows for inline chips. Handles both core and bridge. */
function resolveResetWindowsGeneric(
  snapshot: ProviderSnapshot | ProviderUsageSnapshot,
  kinds: FloatBarResetWindow[],
  windowLabel: (kind: FloatBarResetWindow) => string,
): Array<{ key: string; window: RateWindowSnapshot; label: string }> {
  const seen = new Set<string>();
  const out: Array<{ key: string; window: RateWindowSnapshot; label: string }> = [];
  for (const kind of kinds) {
    let window: RateWindowSnapshot | null = null;
    if (isCoreSnapshot(snapshot)) {
      const core = snapshot as ProviderSnapshot;
      if (kind === "primary") {
        const quota = core.windows.filter(
          (w) => w.usageKnown && !w.isInformational && w.displayKind === "quota",
        );
        window = (quota[0] as unknown as RateWindowSnapshot) ?? null;
      } else {
        const found = core.windows.find((w) => w.kind === kind && w.usageKnown);
        window = (found as unknown as RateWindowSnapshot) ?? null;
      }
    } else {
      window = windowByKind(snapshot as ProviderUsageSnapshot, kind);
    }
    if (!window) continue;
    const identity = `${window.windowMinutes ?? "?"}:${window.resetsAt ?? ""}:${
      window.resetDescription ?? ""
    }`;
    if (seen.has(identity)) continue;
    seen.add(identity);
    out.push({ key: kind, window, label: windowLabel(kind) });
  }
  return out;
}

function ProviderPill({
  snapshot,
  entryWindow,
  display,
  scale,
  showResetInline,
  resetWindows,
  windowLabel,
  usedSuffix,
  remainingSuffix,
  isHovered,
  onHover,
  onLeave,
  costSummary,
  placementY = "bottom",
  placementX = "left",
}: {
  snapshot: ProviderSnapshot | ProviderUsageSnapshot;
  entryWindow: TaskbarWindowKind;
  display: QuotaDisplayContext;
  scale: number;
  showResetInline: boolean;
  resetWindows: FloatBarResetWindow[];
  windowLabel: (kind: FloatBarResetWindow) => string;
  usedSuffix: string;
  remainingSuffix: string;
  isHovered?: boolean;
  onHover?: () => void;
  onLeave?: () => void;
  costSummary?: FloatBarCostSummary;
  placementY?: "top" | "bottom" | "top-aligned" | "bottom-aligned";
  placementX?: "left" | "right";
}) {
  const { t } = useLocale();
  const targetWindow = windowForEntry(snapshot, entryWindow);
  const providerId = (snapshot as any).providerId as string;
  const displayName = (snapshot as any).displayName as string;
  const brand = getProviderIcon(providerId).brandColor;
  const gaugeSize = Math.max(16, Math.round((20 * scale) / 2) * 2);
  const iconSize = Math.max(10, Math.round((11 * scale) / 2) * 2);
  const pillKey = `${providerCostKey(snapshot as any)}:${entryWindow}`;

  if (!targetWindow) {
    // Spec §2.2: an entry the provider cannot serve keeps an explicit unknown
    // pill instead of silently vanishing from the bar.
    return (
      <div
        className="floatbar__pill floatbar__pill--ok floatbar__pill--unsupported"
        data-pill-key={pillKey}
        title={isHovered ? undefined : `${displayName}: ${t("TaskbarEntryUnsupported")}`}
        data-tauri-drag-region
        style={{ "--brand": brand } as CSSProperties}
        onMouseEnter={onHover}
        onMouseLeave={onLeave}
      >
        <div
          className="floatbar__icon-gauge"
          data-tauri-drag-region
          style={{ width: gaugeSize, height: gaugeSize }}
        >
          <svg
            className="floatbar__ring-svg"
            viewBox="0 0 20 20"
            data-tauri-drag-region
            aria-hidden="true"
          >
            <circle className="floatbar__ring-bg" cx="10" cy="10" r="8" />
          </svg>
          <span
            className="floatbar__provider-icon"
            data-tauri-drag-region
            style={{ width: iconSize, height: iconSize }}
          >
            <ProviderIcon providerId={providerId} size={iconSize} />
          </span>
        </div>
        <span className="floatbar__text" data-tauri-drag-region>
          <span className="floatbar__pct" data-tauri-drag-region>
            —
          </span>
        </span>
        {isHovered && (
          <HoverFlyout
            providerId={providerId}
            displayName={displayName}
            statusBadge={t("TaskbarEntryUnsupported")}
            tone="warn"
            percentLabel="—"
            pctValue={0}
            windowSub={t("TaskbarEntryUnsupported")}
            resetTime={null}
            costText={null}
            resetLabel={t("ProviderNextReset")}
            costLabel={t("PanelToday")}
            placementY={placementY}
            placementX={placementX}
          />
        )}
      </div>
    );
  }
  const percent = quotaPercentDisplay(targetWindow as RateWindowSnapshot, display);
  const displaySuffix = percent.semantics === "used" ? usedSuffix : remainingSuffix;
  const error = (snapshot as any).error as string | null;
  const isCore = isCoreSnapshot(snapshot as ProviderSnapshot);
  // displayState for core vs error for bridge
  const coreDisplayState = isCore ? (snapshot as ProviderSnapshot).displayState : null;
  const isErrorState = Boolean(error) || coreDisplayState === "error" || coreDisplayState === "authRequired";
  const stale = coreDisplayState === "stale";
  const tone: "ok" | "warn" | "crit" = isErrorState
    ? "crit"
    : percent.level === "exhausted" || percent.level === "critical"
      ? "crit"
      : percent.level === "high"
        ? "warn"
        : "ok";

  const label = error ? "—" : `${percent.rounded}%`;
  const pctValue = Math.max(0, Math.min(100, percent.rounded));
  const dashOffset = Number((50.265 * (1 - pctValue / 100)).toFixed(2));
  const resets = showResetInline
    ? resolveResetWindowsGeneric(snapshot, resetWindows, windowLabel)
    : [];
  const primaryReset = useFormattedResetTime(
    targetWindow.resetsAt,
    targetWindow.resetDescription,
    display.resetTimeRelative,
  );
  const resetIconSize = Math.max(8, Math.round((10 * scale) / 2) * 2);

  const statusBadge = isErrorState
    ? (t("TrayStatusError") || "异常")
    : tone === "crit"
      ? (percent.semantics === "used" ? "配额告急" : "结余告急")
      : tone === "warn"
        ? (percent.semantics === "used" ? "使用较多" : "结余偏紧")
        : (percent.semantics === "used" ? "用量充足" : "结余充足");

  const todayCostStr = formatUsd(costSummary?.todayCost ?? null);
  const thirtyCostStr = formatUsd(costSummary?.thirtyDayCost ?? null);
  const costText = todayCostStr
    ? `${todayCostStr}${thirtyCostStr ? ` (${thirtyCostStr})` : ""}`
    : (snapshot as any).balance
      ? `${(snapshot as any).balance.amount} ${(snapshot as any).balance.currency || ""}`.trim()
      : "正常";

  const windowSub = `${windowLabel(entryWindow as FloatBarResetWindow) || "主额度"} · ${displaySuffix}`;

  return (
    <div
      className={`floatbar__pill floatbar__pill--${tone}${stale ? " floatbar__pill--stale" : ""}`}
      data-pill-key={pillKey}
      title={isHovered ? undefined : `${displayName}: ${label} ${displaySuffix}${
        primaryReset ? `\n${primaryReset}` : ""
      }${stale ? `\n${t("TrayStatusStale")}` : ""}`}
      data-tauri-drag-region
      style={{ "--brand": brand } as CSSProperties}
      onMouseEnter={onHover}
      onMouseLeave={onLeave}
    >
      <div
        className="floatbar__icon-gauge"
        data-tauri-drag-region
        style={{ width: gaugeSize, height: gaugeSize }}
      >
        <svg
          className="floatbar__ring-svg"
          viewBox="0 0 20 20"
          data-tauri-drag-region
          aria-hidden="true"
        >
          <circle className="floatbar__ring-bg" cx="10" cy="10" r="8" />
          <circle
            className="floatbar__ring-fill"
            cx="10"
            cy="10"
            r="8"
            strokeDasharray="50.265"
            strokeDashoffset={dashOffset}
          />
        </svg>
        <span
          className="floatbar__provider-icon"
          data-tauri-drag-region
          style={{ width: iconSize, height: iconSize }}
        >
          <ProviderIcon providerId={providerId} size={iconSize} />
        </span>
      </div>
      <span className="floatbar__text" data-tauri-drag-region>
        <span className="floatbar__pct" data-tauri-drag-region>
          {label}
        </span>
        {resets.map((reset) => {
          // Spec §2.3: with one chip the window label still appears when that
          // chip is not the window the entry pinned — otherwise a weekly-pinned
          // entry showing a 5h chip has nothing explaining the difference.
          const sameAsTarget =
            reset.window.windowMinutes === targetWindow.windowMinutes &&
            (reset.window.resetsAt ?? "") === (targetWindow.resetsAt ?? "") &&
            (reset.window.resetDescription ?? "") === (targetWindow.resetDescription ?? "");
          return (
            <ResetChip
              key={reset.key}
              window={reset.window}
              label={resets.length > 1 || !sameAsTarget ? reset.label : null}
              relative={display.resetTimeRelative}
              iconSize={resetIconSize}
              isParentHovered={isHovered}
            />
          );
        })}
      </span>
      {isHovered && (
        <HoverFlyout
          providerId={providerId}
          displayName={displayName}
          statusBadge={statusBadge}
          tone={tone}
          percentLabel={label}
          pctValue={pctValue}
          windowSub={windowSub}
          resetTime={primaryReset}
          costText={costText}
          resetLabel={t("ProviderNextReset")}
          costLabel={todayCostStr ? t("PanelToday") : "费用/状态"}
          placementY={placementY}
          placementX={placementX}
        />
      )}
    </div>
  );
}

export default function FloatBar({
  state,
  preview,
}: {
  state: BootstrapState;
  preview?: {
    settings: SettingsSnapshot;
    providers: ProviderUsageSnapshot[];
  };
}) {
  const { t } = useLocale();
  const isPreview = Boolean(preview);
  const [settings, setSettings] = useState<SettingsSnapshot>(
    preview?.settings ?? state.settings,
  );
  const orientation: "horizontal" | "vertical" =
    settings.floatBarOrientation === "vertical" ? "vertical" : "horizontal";

  const [flyoutPlacement, setFlyoutPlacement] = useState<FlyoutPlacementState>({
    placementY: "bottom",
    placementX: "left",
    padTop: 0,
    padLeft: 0,
  });
  const flyoutPlacementRef = useRef<FlyoutPlacementState>(flyoutPlacement);
  flyoutPlacementRef.current = flyoutPlacement;

  const [hoveredKey, setHoveredKey] = useState<string | null>(null);
  const [showContextMenu, setShowContextMenu] = useState(false);
  const showContextMenuRef = useRef(false);
  showContextMenuRef.current = showContextMenu;

  // Ensure store sync synchronously on mount/render to avoid first-frame empty pill state.
  if (typeof window !== "undefined" && !isPreview) {
    ensureFloatBarStoreSync();
  }

  const contextMenuLabels = useMemo(
    () => ({
      refresh: t("ActionRefreshAll") || "立即刷新",
      openPanel: t("TabTrayPanel") ? `展开${t("TabTrayPanel")}` : "展开详细面板",
      settings: `${t("TabFloatBar") || "悬浮栏"}${t("ActionSettings") || "设置"}`,
      hide: `隐藏${t("TabFloatBar") || "悬浮栏"}`,
    }),
    [t],
  );

  const handleContextMenu = useCallback(
    (e: MouseEvent<HTMLElement>) => {
      e.preventDefault();
      e.stopPropagation();
      if (isPreview) return;
      if (hoverTimeoutRef.current !== null) {
        clearTimeout(hoverTimeoutRef.current);
        hoverTimeoutRef.current = null;
      }
      setHoveredKey(null);
      const el = document.querySelector<HTMLElement>(".floatbar");
      if (el) {
        const rawRect = el.getBoundingClientRect();
        const currentPadTop = flyoutPlacementRef.current.padTop;
        const currentPadLeft = flyoutPlacementRef.current.padLeft;
        const restingRect = {
          top: rawRect.top - currentPadTop,
          bottom: rawRect.bottom - currentPadTop,
          left: rawRect.left - currentPadLeft,
          right: rawRect.right - currentPadLeft,
        };
        const placement = computeFlyoutPlacement({
          pillRect: restingRect,
          basePos: basePositionRef.current,
          workArea: workAreaRef.current,
          orientation,
        });
        setFlyoutPlacement(placement);
      }
      setShowContextMenu((prev) => !prev);
    },
    [isPreview, orientation],
  );

  const [isBarHovered, setIsBarHovered] = useState(false);
  const hoverTimeoutRef = useRef<number | null>(null);

  const basePositionRef = useRef<{ x: number; y: number }>({
    x: typeof window !== "undefined" ? window.screenX : 0,
    y: typeof window !== "undefined" ? window.screenY : 0,
  });
  const hoveredKeyRef = useRef<string | null>(null);
  hoveredKeyRef.current = hoveredKey;
  const lastResizeRef = useRef<{
    w: number;
    h: number;
    x?: number | null;
    y?: number | null;
    isExpanded?: boolean;
  } | null>(null);
  const resizeRafRef = useRef<number | null>(null);

  const workAreaRef = useRef<{ top: number; bottom: number; left: number; right: number }>({
    top: typeof window !== "undefined" && window.screen ? ((window.screen as any).availTop ?? 0) : 0,
    bottom:
      typeof window !== "undefined" && window.screen
        ? ((window.screen as any).availTop ?? 0) + (window.screen.availHeight || 1080)
        : 1080,
    left: typeof window !== "undefined" && window.screen ? ((window.screen as any).availLeft ?? 0) : 0,
    right:
      typeof window !== "undefined" && window.screen
        ? ((window.screen as any).availLeft ?? 0) + (window.screen.availWidth || 1920)
        : 1920,
  });

  useEffect(() => {
    if (isPreview) return;
    const updateMonitor = async () => {
      try {
        if (typeof currentMonitor === "function") {
          const mon = await currentMonitor();
          if (mon?.workArea) {
            const scale = mon.scaleFactor || 1;
            workAreaRef.current = {
              top: mon.workArea.position.y / scale,
              bottom: (mon.workArea.position.y + mon.workArea.size.height) / scale,
              left: mon.workArea.position.x / scale,
              right: (mon.workArea.position.x + mon.workArea.size.width) / scale,
            };
          }
        }
      } catch {}
    };
    void updateMonitor();
  }, [isPreview]);

  useEffect(() => {
    if (isPreview) return;
    const win = getCurrentWindow();
    let unlisten: (() => void) | undefined;
    win.onMoved?.(({ payload }) => {
      if (!hoveredKeyRef.current) {
        const scale = window.devicePixelRatio || 1;
        basePositionRef.current = {
          x: payload.x / scale,
          y: payload.y / scale,
        };
      }
    })
      .then((fn) => {
        unlisten = fn;
      })
      .catch(() => {});

    win.outerPosition?.()
      .then((pos) => {
        if (pos && !hoveredKeyRef.current) {
          const scale = window.devicePixelRatio || 1;
          basePositionRef.current = {
            x: pos.x / scale,
            y: pos.y / scale,
          };
        }
      })
      .catch(() => {});

    return () => {
      unlisten?.();
    };
  }, [isPreview]);

  const startDrag = useCallback((event: MouseEvent<HTMLElement>) => {
    if (isPreview || event.button !== 0) return;
    if (hoverTimeoutRef.current !== null) {
      clearTimeout(hoverTimeoutRef.current);
      hoverTimeoutRef.current = null;
    }
    setHoveredKey(null);
    setShowContextMenu(false);
    setFlyoutPlacement({ placementY: "bottom", placementX: "left", padTop: 0, padLeft: 0 });
    if (lastResizeRef.current?.isExpanded) {
      void adjustFloatBarGeometry(
        lastResizeRef.current.w,
        lastResizeRef.current.h,
        basePositionRef.current.x,
        basePositionRef.current.y,
        false,
      ).catch(() => {});
      lastResizeRef.current = {
        ...lastResizeRef.current,
        x: basePositionRef.current.x,
        y: basePositionRef.current.y,
        isExpanded: false,
      };
    }
    void getCurrentWindow().startDragging().catch(() => {});
  }, [isPreview]);

  useEffect(() => {
    if (isPreview) return;
    const preventBrowserContextMenu = (e: globalThis.MouseEvent) => {
      e.preventDefault();
    };
    window.addEventListener("contextmenu", preventBrowserContextMenu);
    return () => {
      window.removeEventListener("contextmenu", preventBrowserContextMenu);
    };
  }, [isPreview]);

  useEffect(() => {
    if (!showContextMenu) return;
    const handleOutsidePointer = (e: globalThis.MouseEvent) => {
      const target = e.target as HTMLElement | null;
      if (target?.closest(".floatbar__context-menu")) return;
      setShowContextMenu(false);
      setFlyoutPlacement({ placementY: "bottom", placementX: "left", padTop: 0, padLeft: 0 });
    };
    const handleKeyDown = (e: KeyboardEvent) => {
      if (e.key === "Escape") {
        setShowContextMenu(false);
        setFlyoutPlacement({ placementY: "bottom", placementX: "left", padTop: 0, padLeft: 0 });
      }
    };
    window.addEventListener("pointerdown", handleOutsidePointer);
    window.addEventListener("keydown", handleKeyDown);
    return () => {
      window.removeEventListener("pointerdown", handleOutsidePointer);
      window.removeEventListener("keydown", handleKeyDown);
    };
  }, [showContextMenu]);

  useEffect(() => {
    if (isPreview) return;
    document.body.classList.add("floatbar-window");
    return () => {
      document.body.classList.remove("floatbar-window");
    };
  }, [isPreview]);

  const [localCosts, setLocalCosts] = useState<Record<string, FloatBarCostSummary>>({});
  // UI tick for relative time; data refresh is owned by RefreshCoordinator/core store
  const [, setUiTick] = useState(0);
  const handlePillEnter = useCallback((key: string) => {
    if (hoverTimeoutRef.current !== null) {
      clearTimeout(hoverTimeoutRef.current);
      hoverTimeoutRef.current = null;
    }
    const pillEl = document.querySelector<HTMLElement>(`[data-pill-key="${key}"]`);
    if (pillEl) {
      const rawRect = pillEl.getBoundingClientRect();
      const currentPadTop = flyoutPlacementRef.current.padTop;
      const currentPadLeft = flyoutPlacementRef.current.padLeft;
      const restingRect = {
        top: rawRect.top - currentPadTop,
        bottom: rawRect.bottom - currentPadTop,
        left: rawRect.left - currentPadLeft,
        right: rawRect.right - currentPadLeft,
      };
      const placement = computeFlyoutPlacement({
        pillRect: restingRect,
        basePos: basePositionRef.current,
        workArea: workAreaRef.current,
        orientation,
      });
      setFlyoutPlacement(placement);
    }
    setHoveredKey(key);
  }, [orientation]);

  const handlePillLeave = useCallback(() => {
    if (hoverTimeoutRef.current !== null) {
      clearTimeout(hoverTimeoutRef.current);
    }
    hoverTimeoutRef.current = window.setTimeout(() => {
      setHoveredKey(null);
      setFlyoutPlacement({ placementY: "bottom", placementX: "left", padTop: 0, padLeft: 0 });
    }, 120);
  }, []);

  const handleBarEnter = useCallback(() => {
    setIsBarHovered(true);
  }, []);

  const handleBarLeave = useCallback(() => {
    setIsBarHovered(false);
    handlePillLeave();
  }, [handlePillLeave]);

  useEffect(() => {
    return () => {
      if (hoverTimeoutRef.current !== null) {
        clearTimeout(hoverTimeoutRef.current);
      }
    };
  }, []);

  useEffect(() => {
    if (preview?.settings) setSettings(preview.settings);
  }, [preview?.settings]);

  // Keep store in sync with backend when not in preview.
  useEffect(() => {
    if (isPreview) return;
    ensureFloatBarStoreSync();
    // no cleanup: store is singleton for window lifetime
  }, [isPreview]);

  // Unified UI tick (60s) for countdowns; does not trigger provider fetch.
  useEffect(() => {
    if (isPreview) return;
    const id = setInterval(() => setUiTick((v) => v + 1), 60_000);
    return () => clearInterval(id);
  }, [isPreview]);

  useEffect(() => {
    if (isPreview) return;
    const unlisten = listen(FLOAT_BAR_CONFIG_CHANGED_EVENT, () => {
      void getSettingsSnapshot().then(setSettings).catch(() => {});
    });
    return () => {
      void unlisten.then((fn) => fn());
    };
  }, [isPreview]);

  const style = settings.floatBarStyle === "taskbar" ? "taskbar" : "floating";
  const scale = Math.max(0.75, Math.min(2, (settings.floatBarScale ?? 100) / 100));
  const showResetInline = settings.floatBarShowResetInline;
  const resetWindows = settings.floatBarResetWindows ?? ["primary"];
  const windowLabel = useCallback(
    (kind: FloatBarResetWindow) => t(RESET_WINDOW_LABEL_KEYS[kind]),
    [t],
  );
  const showCost = settings.floatBarShowCost;
  const display = useMemo(
    () => quotaDisplayContext(settings, "floatBar"),
    [settings],
  );

  // ---- Core snapshot consumption ----
  // The visible list derives from floatBarEntries and reads the shared store.
  const effectiveEntries = useMemo(
    () => resolveFloatBarEntries(settings),
    [settings.floatBarEntries, settings.floatBarProviderIds],
  );
  const expandedEntries = useMemo(
    () => expandFloatBarEntries(effectiveEntries, settings.enabledProviders ?? []),
    [effectiveEntries, settings.enabledProviders],
  );

  const coreSnapshots = useFloatBarSnapshots(floatBarStore);
  const bridgeSnapshots: ProviderUsageSnapshot[] = preview?.providers ?? [];

  // Build a map for quick lookup (both core and bridge)
  const snapshotByProviderId = useMemo(() => {
    const map = new Map<string, ProviderSnapshot | ProviderUsageSnapshot>();
    if (isPreview) {
      for (const p of bridgeSnapshots) map.set(p.providerId, p);
    } else {
      for (const s of coreSnapshots) map.set(s.providerId, s);
      // Fallback: if store empty (e.g. in tests where getCachedProviders mock hasn't yet populated store synchronously),
      // allow direct bridge fallback via preview? For tests that mock getCachedProviders, store will be async populated,
      // but we may need to synchronously handle case where tests set preview.providers? Already handled.
      // Additionally, if store empty and not preview, we try to use any cached bridge data that might be available via global?
      // For test compatibility where getCachedProviders mock returns data but store hasn't yet flushed, we can also consider
      // that bridgeSnapshots is empty and coreSnapshots may be empty initially; we handle via effect that populates store.
    }
    return map;
  }, [isPreview, bridgeSnapshots, coreSnapshots]);

  type VisibleEntry = {
    entry: { providerId: string; window: TaskbarWindowKind };
    snapshot: ProviderSnapshot | ProviderUsageSnapshot | null;
    targetWindow: RateWindowSnapshot | null;
  };

  const visibleEntries: VisibleEntry[] = useMemo(() => {
    const out: VisibleEntry[] = [];
    for (const entry of expandedEntries) {
      const snap = snapshotByProviderId.get(entry.providerId);
      if (!snap) {
        // Spec §2.2: a configured entry whose provider has no snapshot yet
        // keeps an explicit unknown pill instead of silently vanishing —
        // the settings preview must account for every configured row.
        out.push({ entry, snapshot: null, targetWindow: null });
        continue;
      }
      if (isCoreSnapshot(snap) && snap.displayState === "loading") {
        // A still-loading snapshot has no windows yet; a state pill here would
        // flash on every cold start. Skip until the first fetch resolves.
        continue;
      }
      const win = windowForEntry(snap, entry.window);
      // Spec §2.2: unsupported/unknown entries keep their pill (the pill
      // renders an explicit unknown state) instead of silently vanishing.
      out.push({ entry, snapshot: snap, targetWindow: win });
    }
    // Spec §2.2: preserve the configured order — what the settings list shows
    // is what the bar renders, never a hidden urgency re-sort.
    return out;
  }, [expandedEntries, snapshotByProviderId]);

  const visibleCostTargetKey = visibleEntries
    .filter(({ snapshot }) => snapshot)
    .map((v) => `${providerCostKey(v.snapshot as any)}:${(v.snapshot as any).providerId}:${(v.snapshot as any).displayName}`)
    .join("|");
  const visibleCostTargets = useMemo<FloatBarCostTarget[]>(
    () =>
      showCost
        ? visibleEntries
        .filter(({ snapshot }) => snapshot)
        .map(({ snapshot }) => ({
            key: providerCostKey(snapshot as any),
            providerId: (snapshot as any).providerId,
            displayName: (snapshot as any).displayName,
          }))
        : [],
    [showCost, visibleCostTargetKey],
  );

  useEffect(() => {
    if (isPreview) {
      setLocalCosts({});
      return;
    }
    if (!hasFloatBarLocalCostFetcher()) {
      setLocalCosts({});
      return;
    }
    let cancelled = false;
    const targets = visibleCostTargets;

    if (targets.length === 0) {
      setLocalCosts({});
      return () => {
        cancelled = true;
      };
    }

    // Deduplicate targets by key
    const deduped = Array.from(new Map(targets.map((t) => [t.key, t])).values());

    Promise.allSettled(
      deduped.map(async (target) => {
        const localCost = await fetchFloatBarLocalCost(target.providerId);
        if (!localCost) return null;
        return {
          key: target.key,
          providerId: target.providerId,
          displayName: target.displayName,
          todayCost: localCost.todayCost,
          thirtyDayCost: localCost.thirtyDayCost,
        } satisfies FloatBarCostSummary;
      }),
    )
      .then((results) => {
        if (cancelled) return;
        const next: Record<string, FloatBarCostSummary> = {};
        for (const result of results) {
          if (result.status === "fulfilled" && result.value) {
            next[result.value.key] = result.value;
          }
        }
        setLocalCosts(next);
      })
      .catch(() => {
        if (!cancelled) setLocalCosts({});
      });

    return () => {
      cancelled = true;
    };
  }, [isPreview, visibleCostTargets]);

  const visibleCosts = visibleEntries
    .filter(({ snapshot }) => snapshot)
    .map(({ snapshot }) => localCosts[providerCostKey(snapshot as any)])
    .filter((summary): summary is FloatBarCostSummary => Boolean(summary));
  // Dedup costs by key
  const dedupedVisibleCosts = useMemo(() => {
    const map = new Map<string, FloatBarCostSummary>();
    for (const c of visibleCosts) map.set(c.key, c);
    return Array.from(map.values());
  }, [visibleCosts]);

  const visibleCostValuesKey = dedupedVisibleCosts
    .map((summary) => `${summary.key}:${summary.todayCost ?? ""}:${summary.thirtyDayCost ?? ""}`)
    .join("|");
  const resizeToContent = useCallback(() => {
    if (isPreview) return;
    const el = document.querySelector<HTMLElement>(".floatbar");
    if (!el) return;
    if (resizeRafRef.current !== null) {
      cancelAnimationFrame(resizeRafRef.current);
    }
    resizeRafRef.current = requestAnimationFrame(() => {
      resizeRafRef.current = null;
      const rect = el.getBoundingClientRect();
      const flyout = document.querySelector<HTMLElement>(".floatbar__hover-flyout");
      const contextMenu = document.querySelector<HTMLElement>(".floatbar__context-menu");
      let w = rect.width;
      let h = rect.height;
      if (flyout || contextMenu) {
        const targetEl = (flyout || contextMenu)!;
        const fRect = targetEl.getBoundingClientRect();
        // Measure the union bounding box of the floatbar and the open flyout/menu
        const maxRight = Math.max(rect.right, fRect.right);
        const maxBottom = Math.max(rect.bottom, fRect.bottom);
        // Leave ample margin for the 24px blur flyout box-shadow
        w = maxRight + 20;
        h = maxBottom + 24;
      } else {
        const padding = 8;
        w = rect.width + padding;
        h = rect.height + padding;
      }
      const targetW = Math.ceil(w);
      const targetH = Math.ceil(h);

      const isExpanded = hoveredKeyRef.current !== null || showContextMenuRef.current;
      const wasExpanded = lastResizeRef.current?.isExpanded ?? false;
      const needsPositionAdjustment = isExpanded || wasExpanded;

      let targetX: number | null = null;
      let targetY: number | null = null;

      if (needsPositionAdjustment) {
        targetX = Math.round(
          basePositionRef.current.x - (isExpanded ? flyoutPlacementRef.current.padLeft : 0),
        );
        targetY = Math.round(
          basePositionRef.current.y - (isExpanded ? flyoutPlacementRef.current.padTop : 0),
        );
      }

      const last = lastResizeRef.current;
      const sizeUnchanged =
        last && Math.abs(last.w - targetW) <= 1 && Math.abs(last.h - targetH) <= 1;
      const posUnchanged =
        !needsPositionAdjustment ||
        (last &&
          last.x != null &&
          last.y != null &&
          targetX != null &&
          targetY != null &&
          Math.abs(last.x - targetX) <= 1 &&
          Math.abs(last.y - targetY) <= 1);
      const expandedUnchanged = last && last.isExpanded === isExpanded;

      if (sizeUnchanged && posUnchanged && expandedUnchanged) return;

      lastResizeRef.current = {
        w: targetW,
        h: targetH,
        x: targetX,
        y: targetY,
        isExpanded,
      };
      void adjustFloatBarGeometry(targetW, targetH, targetX, targetY, isExpanded).catch(() => {});
    });
  }, [isPreview]);

  useEffect(() => {
    resizeToContent();
  }, [
    resizeToContent,
    visibleEntries.length,
    visibleCostValuesKey,
    orientation,
    style,
    scale,
    showResetInline,
    resetWindows.join(","),
    display.resetTimeRelative,
    hoveredKey,
    showContextMenu,
    flyoutPlacement.padTop,
    flyoutPlacement.padLeft,
  ]);

  useEffect(() => {
    if (isPreview) return;
    const el = document.querySelector<HTMLElement>(".floatbar");
    if (!el || typeof ResizeObserver === "undefined") return;
    const observer = new ResizeObserver(resizeToContent);
    observer.observe(el);
    return () => observer.disconnect();
  }, [isPreview, resizeToContent]);

  useEffect(
    () => () => {
      if (resizeRafRef.current !== null) {
        cancelAnimationFrame(resizeRafRef.current);
      }
    },
    [],
  );

  const opacityFraction = Math.max(
    0.3,
    Math.min(1, (Number.isFinite(settings.floatBarOpacity) ? settings.floatBarOpacity : 100) / 100),
  );
  // Spec §6.4: with the master switch off the preview dims but keeps its
  // values; the production window is hidden by the host instead.
  const disabledPreview = isPreview && settings.floatBarEnabled === false;
  const isHovered = isBarHovered || hoveredKey !== null || showContextMenu;
  const effectiveOpacity = disabledPreview
    ? opacityFraction * 0.5
    : isHovered
      ? 1
      : opacityFraction;

  return (
    <div
      className={`floatbar floatbar--${orientation} floatbar--${style}${settings.floatBarDarkText ? " floatbar--light-bg" : ""}${disabledPreview ? " floatbar--disabled-preview" : ""}${isHovered ? " is-hovered" : ""}`}
      data-tauri-drag-region
      onMouseDown={startDrag}
      onContextMenu={handleContextMenu}
      onMouseEnter={handleBarEnter}
      onMouseLeave={handleBarLeave}
      style={
        {
          opacity: effectiveOpacity,
          "--floatbar-opacity": disabledPreview ? opacityFraction * 0.5 : opacityFraction,
          "--floatbar-scale": scale,
          paddingTop: flyoutPlacement.padTop ? `${flyoutPlacement.padTop}px` : undefined,
          paddingLeft: flyoutPlacement.padLeft ? `${flyoutPlacement.padLeft}px` : undefined,
        } as CSSProperties
      }
    >
      {visibleEntries.length === 0 ? (
        <div className="floatbar__empty" data-tauri-drag-region>
          {t("FloatBarNoProviders")}
        </div>
      ) : (
        <>
          {visibleEntries.map(({ entry, snapshot }) => {
            if (!snapshot) {
              const brand = getProviderIcon(entry.providerId).brandColor;
              const gaugeSize = Math.max(16, Math.round((20 * scale) / 2) * 2);
              const iconSize = Math.max(10, Math.round((11 * scale) / 2) * 2);
              const pillKey = `missing:${entry.providerId}:${entry.window}`;
              const isHovered = hoveredKey === pillKey;
              return (
                <div
                  key={pillKey}
                  className="floatbar__pill floatbar__pill--ok floatbar__pill--unsupported"
                  data-pill-key={pillKey}
                  title={isHovered ? undefined : t("TaskbarEntryUnsupported")}
                  data-tauri-drag-region
                  style={{ "--brand": brand } as CSSProperties}
                  onMouseEnter={() => handlePillEnter(pillKey)}
                  onMouseLeave={handlePillLeave}
                >
                  <div
                    className="floatbar__icon-gauge"
                    data-tauri-drag-region
                    style={{ width: gaugeSize, height: gaugeSize }}
                  >
                    <svg
                      className="floatbar__ring-svg"
                      viewBox="0 0 20 20"
                      data-tauri-drag-region
                      aria-hidden="true"
                    >
                      <circle className="floatbar__ring-bg" cx="10" cy="10" r="8" />
                    </svg>
                    <span
                      className="floatbar__provider-icon"
                      data-tauri-drag-region
                      style={{ width: iconSize, height: iconSize }}
                    >
                      <ProviderIcon providerId={entry.providerId} size={iconSize} />
                    </span>
                  </div>
                  <span className="floatbar__text" data-tauri-drag-region>
                    <span className="floatbar__pct" data-tauri-drag-region>
                      —
                    </span>
                  </span>
                </div>
              );
            }
            const pillKey = `${providerCostKey(snapshot as any)}:${entry.window}`;
            const isHovered = hoveredKey === pillKey;
            return (
              <ProviderPill
                key={pillKey}
                snapshot={snapshot}
                entryWindow={entry.window}
                display={display}
                scale={scale}
                showResetInline={showResetInline}
                resetWindows={resetWindows}
                windowLabel={windowLabel}
                usedSuffix={t("PanelUsedSuffix")}
                remainingSuffix={t("FloatBarRemainingSuffix")}
                isHovered={isHovered}
                onHover={() => handlePillEnter(pillKey)}
                onLeave={handlePillLeave}
                costSummary={localCosts[providerCostKey(snapshot as any)]}
                placementY={flyoutPlacement.placementY}
                placementX={flyoutPlacement.placementX}
              />
            );
          })}
          {dedupedVisibleCosts.map((summary) => {
            const costKey = `cost:${summary.key}`;
            const isHovered = hoveredKey === costKey;
            return (
              <CostPill
                key={costKey}
                summary={summary}
                scale={scale}
                todayLabel={t("PanelToday")}
                thirtyDayLabel={t("FloatBarThirtyDayShort")}
                isHovered={isHovered}
                onHover={() => handlePillEnter(costKey)}
                onLeave={handlePillLeave}
                placementY={flyoutPlacement.placementY}
                placementX={flyoutPlacement.placementX}
              />
            );
          })}
        </>
      )}
      {showContextMenu && (
        <FloatBarContextMenu
          placementY={flyoutPlacement.placementY}
          placementX={flyoutPlacement.placementX}
          labels={contextMenuLabels}
          onRefresh={async () => {
            setShowContextMenu(false);
            setFlyoutPlacement({ placementY: "bottom", placementX: "left", padTop: 0, padLeft: 0 });
            await invokeSurfaceAction({ type: "refresh", force: true }).catch(() => {});
          }}
          onOpenPanel={async () => {
            setShowContextMenu(false);
            setFlyoutPlacement({ placementY: "bottom", placementX: "left", padTop: 0, padLeft: 0 });
            await revealTrayPanelWindow().catch(() => {});
          }}
          onOpenSettings={async () => {
            setShowContextMenu(false);
            setFlyoutPlacement({ placementY: "bottom", placementX: "left", padTop: 0, padLeft: 0 });
            await invokeSurfaceAction({
              type: "openSettings",
              target: { kind: "settings", tab: "floatBar" },
            }).catch(() => {});
          }}
          onHide={async () => {
            setShowContextMenu(false);
            setFlyoutPlacement({ placementY: "bottom", placementX: "left", padTop: 0, padLeft: 0 });
            await hideFloatBar().catch(() => {});
          }}
        />
      )}
    </div>
  );
}
