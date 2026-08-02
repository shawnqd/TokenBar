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
import { getCurrentWindow } from "@tauri-apps/api/window";
import { useFormattedResetTime } from "../hooks/useFormattedResetTime";
import {
  quotaDisplayContext,
  quotaPercentDisplay,
  type QuotaDisplayContext,
} from "../lib/quotaDisplay";
import { useLocale } from "../hooks/useLocale";
import { useProviders } from "../hooks/useProviders";
import {
  getProviderLocalUsageSummary,
  getSettingsSnapshot,
  refreshProvidersIfStale,
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
} from "../types/bridge";
import { windowByKind } from "../lib/quotaWindows";
import { FLOAT_BAR_CONFIG_CHANGED_EVENT, resizeFloatBar } from "./api";
import "./FloatBar.css";

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

/**
 * Short names for the reset cycles.
 *
 * The `TaskbarWindow*` strings are plain cycle names ("5h", "weekly"), not
 * taskbar-specific wording, so both surfaces name the same cycle identically —
 * which is the point of letting the user pick a cycle by name at all.
 */
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

function providerCostKey(provider: ProviderUsageSnapshot): string {
  return `${provider.providerId}:${provider.accountEmail ?? ""}`;
}

function hasLocalCost(summary: ProviderLocalUsageSummary | null): summary is ProviderLocalUsageSummary {
  return summary?.todayCost != null || summary?.thirtyDayCost != null;
}

function formatUsd(value: number | null): string | null {
  if (value == null || !Number.isFinite(value)) return null;
  return `$${value.toFixed(2)}`;
}

function CostPill({
  summary,
  scale,
  todayLabel,
  thirtyDayLabel,
}: {
  summary: FloatBarCostSummary;
  scale: number;
  todayLabel: string;
  thirtyDayLabel: string;
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
      title={`${summary.displayName}: ${title}`}
      data-tauri-drag-region
      style={{ "--brand": brand } as CSSProperties}
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
    </div>
  );
}
/**
 * One reset readout inside a pill.
 *
 * Its own component because the formatting is a hook: the pill can show several
 * resets now, and hooks cannot be called in a loop whose length varies.
 */
function ResetChip({
  window,
  label,
  relative,
  iconSize,
}: {
  window: RateWindowSnapshot;
  /** Which cycle this is, shown only when the pill carries more than one. */
  label: string | null;
  relative: boolean;
  iconSize: number;
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
      title={title}
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

/** The reset windows this provider can actually answer for, in configured order. */
function resolveResetWindows(
  provider: ProviderUsageSnapshot,
  kinds: FloatBarResetWindow[],
  windowLabel: (kind: FloatBarResetWindow) => string,
): Array<{ key: string; window: RateWindowSnapshot; label: string }> {
  const seen = new Set<string>();
  const out: Array<{ key: string; window: RateWindowSnapshot; label: string }> =
    [];
  for (const kind of kinds) {
    const window = windowByKind(provider, kind);
    if (!window) continue;
    // "primary" and a named cycle routinely resolve to the same window — Codex's
    // primary IS its weekly. Printing it twice would look like two deadlines.
    const identity = `${window.windowMinutes ?? "?"}:${window.resetsAt ?? ""}:${
      window.resetDescription ?? ""
    }`;
    if (seen.has(identity)) continue;
    seen.add(identity);
    out.push({ key: kind, window, label: windowLabel(kind) });
  }
  return out;
}

/**
 * The capacity pill shown for a single provider.
 *
 * Color follows usage: green default, amber when remaining drops below the
 * high-usage threshold, red when remaining is below the critical threshold
 * or the provider is exhausted.
 */
function ProviderPill({
  provider,
  display,
  scale,
  showResetInline,
  resetWindows,
  windowLabel,
  usedSuffix,
  remainingSuffix,
}: {
  provider: ProviderUsageSnapshot;
  /** The floating bar's own presentation choice — never the dashboard's. */
  display: QuotaDisplayContext;
  scale: number;
  showResetInline: boolean;
  resetWindows: FloatBarResetWindow[];
  windowLabel: (kind: FloatBarResetWindow) => string;
  usedSuffix: string;
  remainingSuffix: string;
}) {
  const percent = quotaPercentDisplay(provider.primary, display);
  const displaySuffix = percent.semantics === "used" ? usedSuffix : remainingSuffix;
  // A failed fetch is as actionable as an exhausted quota, so it shares the
  // critical tone. Everything else comes from the shared grading.
  const tone: "ok" | "warn" | "crit" = provider.error
    ? "crit"
    : percent.level === "exhausted" || percent.level === "critical"
      ? "crit"
      : percent.level === "high"
        ? "warn"
        : "ok";

  const brand = getProviderIcon(provider.providerId).brandColor;
  const label = provider.error ? "—" : `${percent.rounded}%`;
  const resets = showResetInline
    ? resolveResetWindows(provider, resetWindows, windowLabel)
    : [];
  // The tooltip always carries the provider's own next reset, whether or not
  // the inline chips are switched on — hovering is how you check a deadline
  // without giving the bar the width to print one.
  const primaryReset = useFormattedResetTime(
    provider.primary.resetsAt,
    provider.primary.resetDescription,
    display.resetTimeRelative,
  );
  // Icon sizes are forced even so the SVG lands on whole device pixels. At
  // 11px an odd box left the artwork on a half-pixel and it read as tilted.
  const iconSize = Math.max(10, Math.round((11 * scale) / 2) * 2);
  const resetIconSize = Math.max(8, Math.round((10 * scale) / 2) * 2);

  return (
    <div
      className={`floatbar__pill floatbar__pill--${tone}`}
      title={`${provider.displayName}: ${label} ${displaySuffix}${
        primaryReset ? `\n${primaryReset}` : ""
      }`}
      data-tauri-drag-region
      style={{ "--brand": brand } as CSSProperties}
    >
      <span
        className="floatbar__provider-icon"
        data-tauri-drag-region
        style={{ width: iconSize, height: iconSize }}
      >
        <ProviderIcon providerId={provider.providerId} size={iconSize} />
      </span>
      <span className="floatbar__text" data-tauri-drag-region>
        <span className="floatbar__pct" data-tauri-drag-region>
          {label}
        </span>
        {resets.map((reset) => (
          <ResetChip
            key={reset.key}
            window={reset.window}
            /* A single reset needs no name — it is the pill's own quota. Two or
               more do, otherwise the pill shows unlabelled rival deadlines. */
            label={resets.length > 1 ? reset.label : null}
            relative={display.resetTimeRelative}
            iconSize={resetIconSize}
          />
        ))}
      </span>
    </div>
  );
}

/**
 * The always-on-top floating capacity bar.
 *
 * Renders a tiny strip of provider pills. Listens to the same provider
 * refresh cycle as the rest of the app via `useProviders`, and reacts to
 * setting changes (filter list, orientation) live without a reload.
 */
export default function FloatBar({ state }: { state: BootstrapState }) {
  const { t } = useLocale();
  const { providers } = useProviders({
    refreshOnMount: false,
  });
  const startDrag = useCallback((event: MouseEvent<HTMLElement>) => {
    if (event.button !== 0) return;
    void getCurrentWindow().startDragging().catch(() => {});
  }, []);

  // Mark the body so our CSS can strip the dark theme background — the
  // floatbar window is meant to be fully transparent around the pills.
  useEffect(() => {
    document.body.classList.add("floatbar-window");
    return () => {
      document.body.classList.remove("floatbar-window");
    };
  }, []);

  // The floatbar window is detached, so it doesn't share React state
  // with the Settings tab. Listen for the Rust-side config-changed event
  // and re-pull the snapshot when fired.
  const [settings, setSettings] = useState<SettingsSnapshot>(state.settings);
  const [localCosts, setLocalCosts] = useState<Record<string, FloatBarCostSummary>>({});

  // The detached floatbar should keep usage fresh, but it must not open or
  // focus any other surface. Refresh data only; provider-updated events feed
  // this window when the backend completes.
  useEffect(() => {
    const intervalMs = Math.max(60_000, settings.refreshIntervalSecs * 1000);
    const tick = () => {
      void refreshProvidersIfStale().catch(() => {});
    };
    tick();
    const id = setInterval(tick, intervalMs);
    return () => clearInterval(id);
  }, [settings.refreshIntervalSecs]);

  useEffect(() => {
    const unlisten = listen(FLOAT_BAR_CONFIG_CHANGED_EVENT, () => {
      void getSettingsSnapshot().then(setSettings).catch(() => {});
    });
    return () => {
      void unlisten.then((fn) => fn());
    };
  }, []);

  // Orientation flips re-lay-out the bar without recreating the window.
  const orientation: "horizontal" | "vertical" =
    settings.floatBarOrientation === "vertical" ? "vertical" : "horizontal";
  const style = settings.floatBarStyle === "taskbar" ? "taskbar" : "floating";
  const filterIds = settings.floatBarProviderIds;
  const scale = Math.max(0.75, Math.min(2, settings.floatBarScale / 100));
  const showResetInline = settings.floatBarShowResetInline;
  // Default to the pre-setting behaviour when the key is absent, so an older
  // settings file keeps showing exactly the reset it showed yesterday.
  const resetWindows = settings.floatBarResetWindows ?? ["primary"];
  const windowLabel = useCallback(
    (kind: FloatBarResetWindow) => t(RESET_WINDOW_LABEL_KEYS[kind]),
    [t],
  );
  const showCost = settings.floatBarShowCost;
  // The floating bar's own used/remaining and reset-format choice. Changing the
  // dashboard's or the taskbar strip's preference must not move this bar.
  const display = useMemo(
    () => quotaDisplayContext(settings, "floatBar"),
    [settings],
  );
  const visible = useMemo(() => {
    const enabled = new Set(settings.enabledProviders);
    let list = providers.filter((p) => enabled.has(p.providerId));
    if (filterIds && filterIds.length > 0) {
      const wanted = new Set(filterIds);
      list = list.filter((p) => wanted.has(p.providerId));
    }
    return [...list].sort((a, b) => b.primary.usedPercent - a.primary.usedPercent);
  }, [providers, settings.enabledProviders, filterIds]);

  const visibleCostTargetKey = visible
    .map((p) => `${providerCostKey(p)}:${p.providerId}:${p.displayName}`)
    .join("|");
  const visibleCostTargets = useMemo<FloatBarCostTarget[]>(
    () =>
      showCost
        ? visible.map((provider) => ({
            key: providerCostKey(provider),
            providerId: provider.providerId,
            displayName: provider.displayName,
          }))
        : [],
    [showCost, visibleCostTargetKey],
  );

  useEffect(() => {
    let cancelled = false;
    const targets = visibleCostTargets;

    if (targets.length === 0) {
      setLocalCosts({});
      return () => {
        cancelled = true;
      };
    }

    Promise.allSettled(
      targets.map(async (target) => {
        const localUsage = await getProviderLocalUsageSummary(target.providerId);
        if (!hasLocalCost(localUsage)) return null;
        return {
          key: target.key,
          providerId: target.providerId,
          displayName: target.displayName,
          todayCost: localUsage.todayCost,
          thirtyDayCost: localUsage.thirtyDayCost,
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
  }, [visibleCostTargets]);

  const visibleCosts = visible
    .map((provider) => localCosts[providerCostKey(provider)])
    .filter((summary): summary is FloatBarCostSummary => Boolean(summary));
  const visibleCostValuesKey = visibleCosts
    .map((summary) => `${summary.key}:${summary.todayCost ?? ""}:${summary.thirtyDayCost ?? ""}`)
    .join("|");
  // Keep the native floatbar window fitted when late data/fonts/icons change layout.
  const lastResizeRef = useRef<{ w: number; h: number } | null>(null);
  const resizeRafRef = useRef<number | null>(null);
  const resizeToContent = useCallback(() => {
    const el = document.querySelector<HTMLElement>(".floatbar");
    if (!el) return;
    if (resizeRafRef.current !== null) {
      cancelAnimationFrame(resizeRafRef.current);
    }
    resizeRafRef.current = requestAnimationFrame(() => {
      resizeRafRef.current = null;
      const rect = el.getBoundingClientRect();
      const padding = 8;
      const w = Math.ceil(rect.width + padding);
      const h = Math.ceil(rect.height + padding);
      const last = lastResizeRef.current;
      if (last && Math.abs(last.w - w) <= 1 && Math.abs(last.h - h) <= 1) return;
      lastResizeRef.current = { w, h };
      void resizeFloatBar(w, h).catch(() => {});
    });
  }, []);

  useEffect(() => {
    resizeToContent();
  }, [
    resizeToContent,
    visible.length,
    visibleCostValuesKey,
    orientation,
    style,
    scale,
    showResetInline,
    resetWindows.join(","),
    display.resetTimeRelative,
  ]);

  useEffect(() => {
    const el = document.querySelector<HTMLElement>(".floatbar");
    if (!el || typeof ResizeObserver === "undefined") return;
    const observer = new ResizeObserver(resizeToContent);
    observer.observe(el);
    return () => observer.disconnect();
  }, [resizeToContent]);

  useEffect(
    () => () => {
      if (resizeRafRef.current !== null) {
        cancelAnimationFrame(resizeRafRef.current);
      }
    },
    [],
  );

  const opacityFraction = Math.max(0.3, Math.min(1, settings.floatBarOpacity / 100));

  return (
    <div
      className={`floatbar floatbar--${orientation} floatbar--${style}${settings.floatBarDarkText ? " floatbar--light-bg" : ""}`}
      data-tauri-drag-region
      onMouseDown={startDrag}
      style={
        {
          opacity: opacityFraction,
          "--floatbar-scale": scale,
        } as CSSProperties
      }
    >
      <div className="floatbar__handle" data-tauri-drag-region aria-hidden />
      {visible.length === 0 ? (
        <div className="floatbar__empty" data-tauri-drag-region>
          {t("FloatBarNoProviders")}
        </div>
      ) : (
        <>
          {visible.map((p) => (
            <ProviderPill
              key={providerCostKey(p)}
              provider={p}
              display={display}
              scale={scale}
              showResetInline={showResetInline}
              resetWindows={resetWindows}
              windowLabel={windowLabel}
              usedSuffix={t("PanelUsedSuffix")}
              remainingSuffix={t("FloatBarRemainingSuffix")}
            />
          ))}
          {visibleCosts.map((summary) => (
            <CostPill
              key={`cost:${summary.key}`}
              summary={summary}
              scale={scale}
              todayLabel={t("PanelToday")}
              thirtyDayLabel={t("FloatBarThirtyDayShort")}
            />
          ))}
        </>
      )}
    </div>
  );
}
