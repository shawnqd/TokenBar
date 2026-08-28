import { useCallback, useEffect, useMemo, useState, useSyncExternalStore } from "react";
import { useLocale } from "../../../hooks/useLocale";
import {
  Field,
  NumberInput,
  SegmentedControl,
  Toggle,
} from "../../../components/FormControls";
import {
  getTaskbarPreviewLines,
  getTaskbarWindowAvailability,
} from "../../../lib/tauri";
import type { ProviderSnapshot } from "../../../core/snapshot";
import { projectSurface } from "../../../core/projection";
import type { UsageStore } from "../../../core/usageStore";
import { useFontPicker } from "../../../hooks/useFontPicker";
import { FONT_WHITELIST_DEFAULT } from "../../../lib/fontWhitelist";
import FontSettingsBlock, {
  normalizeFontWeight,
} from "../FontSettingsBlock";
import { TASKBAR_PROVIDER_AUTO } from "../../../types/bridge";
import type {
  TaskbarEntry,
  TaskbarPreviewLine,
  TaskbarWindowKind,
  TaskbarWidgetPosition,
  TaskbarWidgetTextAlign,
} from "../../../types/bridge";
import type { TabProps } from "../../Settings";
import BinaryChoiceField from "../BinaryChoiceField";
import PreviewFrame from "../PreviewFrame";
import TaskbarStripPreview from "../previews/TaskbarStripPreview";
import TaskbarEntryList from "../TaskbarEntryList";

/** OpenType `wght` axis bounds. */

// `primary` leads: it is the only kind that resolves for every provider,
// so it is the safe pick when the user does not know what a provider
// publishes. The named cycles below it are for pinning a specific one.
const WINDOW_KINDS: TaskbarWindowKind[] = [
  "primary",
  "session",
  "weekly",
  "daily",
  "monthly",
  "balance",
  "speed",
];

const WINDOW_LABEL_KEYS = {
  primary: "TaskbarWindowPrimary",
  session: "TaskbarWindowSession",
  weekly: "TaskbarWindowWeekly",
  daily: "TaskbarWindowDaily",
  monthly: "TaskbarWindowMonthly",
  balance: "TaskbarWindowBalance",
  speed: "TaskbarWindowSpeed",
} as const;

/** Matches `TASKBAR_MAX_ENTRIES` in shared Rust, which rejects anything longer. */
const MAX_ENTRIES = 6;
/**
 * Matches `taskbar_widget::MAX_VISIBLE_ENTRIES`. The strip is two text rows
 * tall; entries 3 and 4 open a second column rather than a third row, so four
 * fit as a 2x2 grid and anything past that is dropped.
 */
const VISIBLE_LINES = 4;

const DEFAULT_ENTRIES: TaskbarEntry[] = [
  { providerId: TASKBAR_PROVIDER_AUTO, window: "session" },
  { providerId: TASKBAR_PROVIDER_AUTO, window: "weekly" },
];

const EMPTY_STORE_STATE_TASKBAR: { version: number; records: Record<string, unknown> } = {
  version: 0,
  records: {},
};
function useCoreList(store?: UsageStore | null): ProviderSnapshot[] {
  const subscribe = useCallback((cb: () => void) => (store ? store.subscribe(cb) : () => {}), [store]);
  const getSnap = useCallback(
    () => (store ? store.getSnapshot() : EMPTY_STORE_STATE_TASKBAR),
    [store],
  );
  const getServerSnap = useCallback(() => getSnap(), [getSnap]);
  const state = useSyncExternalStore(subscribe as never, getSnap as never, getServerSnap as never) as unknown as {
    records: Record<string, { snapshot: ProviderSnapshot | null }>;
  };
  return useMemo(() => Object.values(state.records).map((r) => r.snapshot).filter(Boolean) as ProviderSnapshot[], [state]);
}
function deriveCells(snapshots: ProviderSnapshot[], entries: TaskbarEntry[], showAsUsed: boolean): import("../../../types/bridge").TaskbarStripCell[] {
  if (snapshots.length === 0) return [];
  const byId = new Map(snapshots.map((s) => [s.providerId, s]));
  const fallback = snapshots[0];
  const out: import("../../../types/bridge").TaskbarStripCell[] = [];
  for (const e of entries.slice(0, 4)) {
    const pid = e.providerId === TASKBAR_PROVIDER_AUTO ? fallback.providerId : e.providerId;
    const snap = byId.get(pid) ?? fallback;
    const proj = projectSurface(snap, { showAsUsed, taskbarEntries: [e] });
    const cell = proj.taskbarCells[0] as unknown as import("../../../types/bridge").TaskbarStripCell | undefined;
    if (cell) {
      // Bridge type retains legacy glyph/color/text for older callers; projection
      // is the single source – legacy fields are mirrored for compat.
      const legacy = cell as unknown as { glyph?: string | null; color?: string | null; text?: string };
      if (legacy.glyph === undefined) (legacy as unknown as Record<string, unknown>).glyph = cell.icon?.fallbackGlyph ?? null;
      if (legacy.color === undefined) (legacy as unknown as Record<string, unknown>).color = cell.icon?.brandColor ?? null;
      if (legacy.text === undefined) (legacy as unknown as Record<string, unknown>).text = `${cell.tag} ${cell.value}`.trim();
      out.push(cell as unknown as import("../../../types/bridge").TaskbarStripCell);
    }
  }
  return out;
}

export default function TaskbarTab({ settings, set, saving, coreStore: injectedCoreStore }: TabProps & { coreStore?: UsageStore | null }) {
  const { t } = useLocale();
  const enabled = settings.taskbarWidgetEnabled;
  const fontSize = settings.taskbarWidgetFontSize ?? 12;
  const width = settings.taskbarWidgetWidth ?? 132;
  const fontWeight = normalizeFontWeight(settings.taskbarWidgetFontWeight ?? 400);
  const [fontWeightDraft, setFontWeightDraft] = useState(fontWeight);
  const textAlign = settings.taskbarWidgetTextAlign ?? "left";
  const fontFamily = settings.taskbarWidgetFontFamily ?? FONT_WHITELIST_DEFAULT;
  // Every row must show a provider. `auto` is the real default — it follows
  // whichever provider the tray icon is on — so an entry that arrives without
  // one falls back to it rather than rendering an empty dropdown.
  const entries: TaskbarEntry[] = (
    settings.taskbarWidgetEntries ?? DEFAULT_ENTRIES
  ).map((entry) => ({
    providerId: entry.providerId || TASKBAR_PROVIDER_AUTO,
    window: entry.window,
  }));
  const providerChoices = [
    { id: TASKBAR_PROVIDER_AUTO, label: t("TaskbarEntriesAutoProvider") },
    ...(settings.enabledProviders ?? []).map((id) => ({ id, label: id })),
  ];

  const commitEntries = (next: TaskbarEntry[]) =>
    set({ taskbarWidgetEntries: next });
  const { defaultFamily } = useFontPicker(fontFamily);
  const [availability, setAvailability] = useState<Record<
    string,
    TaskbarWindowKind[]
  > | null>(null);

  // Which windows each provider can actually answer for. Resolved by the same
  // backend code the strip resolves entries with, so the menu and the strip
  // cannot disagree — the alternative is a dropdown that happily offers "周" for
  // a provider that publishes only a monthly window and then prints
  // "不支持" on the taskbar.
  //
  // `null` means "not loaded yet", which is deliberately different from "this
  // provider has nothing": until the answer arrives every kind stays offered,
  // so a slow backend never silently removes the user's current choice.
  const hasCoreInjection = injectedCoreStore !== undefined;
  const coreSnapshots = useCoreList(injectedCoreStore);
  // Core availability single-source: snapshot.capabilities + actual windows,
  // so the dropdown never offers a cycle that would render "不支持" on the
  // strip. When no core store is injected we keep the legacy Tauri call.
  const coreAvailability = useMemo(() => {
    if (!hasCoreInjection || coreSnapshots.length === 0) return null;
    const map: Record<string, TaskbarWindowKind[]> = {};
    for (const snap of coreSnapshots) {
      const kinds: TaskbarWindowKind[] = ["primary"];
      const caps = snap.capabilities;
      // Add named cycles present as real quota windows
      const hasKind = (k: string | null) =>
        snap.windows.some((w) => w.kind === k && w.usageKnown && !w.isInformational && w.displayKind === "quota");
      if (hasKind("session")) kinds.push("session");
      if (hasKind("daily")) kinds.push("daily");
      if (hasKind("weekly")) kinds.push("weekly");
      if (hasKind("monthly")) kinds.push("monthly");
      if (caps.hasBalance || snap.cost) kinds.push("balance");
      if (caps.supportsOutputSpeed) kinds.push("speed");
      map[snap.providerId] = kinds;
    }
    return map;
  }, [hasCoreInjection, coreSnapshots]);
  useEffect(() => {
    if (hasCoreInjection) {
      if (coreAvailability) setAvailability(coreAvailability);
      return;
    }
    let cancelled = false;
    getTaskbarWindowAvailability()
      .then((map) => {
        if (!cancelled) setAvailability(map);
      })
      .catch(() => {
        if (!cancelled) setAvailability(null);
      });
    return () => {
      cancelled = true;
    };
  }, [hasCoreInjection, coreAvailability]);

  /**
   * The window kinds to offer for one entry's provider.
   *
   * `auto` follows whichever provider the tray settles on, which can be any of
   * them, so it offers the union rather than guessing — narrowing it would hide
   * kinds that become correct the moment the tray switches.
   *
   * The currently-selected kind is always kept, even when unavailable. Dropping
   * it would make the control display someone else's value, and the honest
   * reading of "this entry points at something that does not exist" is the
   * "不支持" the strip already prints, not a silently rewritten setting.
   */
  const windowOptionsFor = (entry: TaskbarEntry): TaskbarWindowKind[] => {
    if (!availability) return WINDOW_KINDS;
    const allowed =
      entry.providerId === TASKBAR_PROVIDER_AUTO
        ? new Set(Object.values(availability).flat())
        : new Set(availability[entry.providerId] ?? []);
    const offered = WINDOW_KINDS.filter((kind) => allowed.has(kind));
    if (!offered.includes(entry.window)) offered.push(entry.window);
    return offered;
  };

  // The preview shows the renderer's OWN lines, fetched from the native strip,
  // rather than an imitation built from sample numbers here.
  //
  // The imitation was wrong in two ways the user could see at a glance, because
  // it had to restate rules that live in Rust: it printed a fixed 18% for every
  // weekly entry while the strip beside it printed the real 47%, and it printed
  // "unsupported" for every balance entry long after balances began rendering.
  // Anything that re-derives what it previews drifts the moment either side
  // changes; reading the buffer makes that impossible instead of unlikely.
  const coreCells = useMemo(
    () => (hasCoreInjection ? deriveCells(coreSnapshots, entries, settings.taskbarShowAsUsed ?? true) : []),
    [hasCoreInjection, coreSnapshots, entries, settings.taskbarShowAsUsed],
  );
  const [tauriLines, setTauriLines] = useState<TaskbarPreviewLine[]>([]);
  // Legacy loading state is derived from balance window's `isInformational`; this
  // helper tracks whether legacy strings contained a loading guard.
  useEffect(() => {
    if (hasCoreInjection) return;
    let cancelled = false;
    // `update_settings` refreshes the tray presentation before it resolves, so
    // by the time an edit lands in `settings` the buffer already holds the new
    // lines — no polling, and no window where the preview shows the old ones.
    getTaskbarPreviewLines()
      .then((lines) => {
        if (!cancelled) setTauriLines(lines);
      })
      .catch(() => {
        if (!cancelled) setTauriLines([]);
      });
    return () => {
      cancelled = true;
    };
  }, [hasCoreInjection, entries, settings]);
  // For the unified path the preview's cells ARE the projection taskbarCells;
  // we keep the legacy line shape for the fallback path via normalizeLegacyLine.
  const previewLines = hasCoreInjection ? [] : tauriLines;
  const previewCells = hasCoreInjection ? coreCells : [];

  useEffect(() => {
    setFontWeightDraft(fontWeight);
  }, [fontWeight]);

  const commitFontWeight = useCallback(() => {
    const next = normalizeFontWeight(fontWeightDraft);
    if (next !== fontWeight) {
      set({ taskbarWidgetFontWeight: next });
    }
  }, [fontWeight, fontWeightDraft, set]);

  // S14/S15 are specified but not on SettingsSnapshot / SettingsUpdate.
  // Preview-only until those keys exist; do not invent a persisted schema.
  const [iconSize, setIconSize] = useState(14);
  const [iconStyle, setIconStyle] = useState<"pure" | "badge" | "solid">("pure");

  const resetAppearance = () => {
    setIconSize(14);
    setIconStyle("pure");
    set({
      taskbarWidgetFontSize: 12,
      taskbarWidgetWidth: 132,
      taskbarWidgetFontWeight: 400,
      taskbarWidgetFontFamily: defaultFamily,
      taskbarWidgetTextAlign: "left",
    });
  };

  return (
    <div className="settings-surf-page">
      <div className="settings-surf-split">
      <div className="settings-surf-fields">
      <section className="settings-section">
        <h3 className="settings-section__title">{t("TaskbarWidgetSection")}</h3>
        <div className="settings-section__group">
          <Field label={t("TaskbarWidgetLabel")} description={t("TaskbarWidgetHelper")}>
            <Toggle
              checked={enabled}
              disabled={saving}
              onChange={(value) => set({ taskbarWidgetEnabled: value })}
            />
          </Field>
          {/* The strip's own used/remaining choice. Independent of the dashboard
              and the floating bar by design (item H) — before this page had a
              control, the key could only be set by migration. */}
          <BinaryChoiceField
            label={t("ShowAsUsedLabel")}
            description={t("ShowAsUsedHelper")}
            onLabel={t("QuotaShowUsedOption")}
            offLabel={t("QuotaShowRemainingOption")}
            value={settings.taskbarShowAsUsed}
            disabled={saving || !enabled}
            onChange={(value) => set({ taskbarShowAsUsed: value })}
          />
          {/* New with the context menu's status row, which is the first thing
              in this family to print a reset time at all. */}
          <BinaryChoiceField
            label={t("ResetTimeRelative")}
            description={t("ResetTimeRelativeHelper")}
            onLabel={t("ResetTimeCountdownOption")}
            offLabel={t("ResetTimeAbsoluteOption")}
            value={settings.taskbarResetTimeRelative}
            disabled={saving || !enabled}
            onChange={(value) => set({ taskbarResetTimeRelative: value })}
          />
          <Field
            label={t("TaskbarWidgetPositionLabel")}
            description={t("TaskbarWidgetPositionHelper")}
          >
            <SegmentedControl
              value={settings.taskbarWidgetPosition ?? "notification"}
              disabled={saving || !enabled}
              options={[
                { value: "left", label: t("TaskbarWidgetPositionLeft") },
                {
                  value: "notification",
                  label: t("TaskbarWidgetPositionNotification"),
                },
              ]}
              onChange={(value) =>
                set({ taskbarWidgetPosition: value as TaskbarWidgetPosition })
              }
            />
          </Field>
        </div>
      </section>

      {/* The right-click menu used to be configurable here: four toggles
          picking which of open-panel / refresh / settings / quit appeared. That
          menu is gone — the strip now shows the same actions as the
          notification-area tray menu, built from `build_tray_menu`, so there is
          no per-row list to choose from any more. The control went first and
          `taskbar_context_menu_actions` followed it out of the settings file. */}

      {/* TASK-021 item 9 — hover fields independent of strip painting. */}
      <section className="settings-section">
        <div className="settings-section-heading">
          <h3 className="settings-section__title">{t("TaskbarTooltipSection")}</h3>
          <button
            type="button"
            className="settings-section-heading__action"
            disabled={saving}
            onClick={() => set({ taskbarTooltipEntries: [] })}
          >
            {t("TaskbarEntriesReset")}
          </button>
        </div>
        <p className="settings-section__hint">{t("TaskbarTooltipHelper")}</p>
        <p className="settings-section__hint">{t("TaskbarTooltipUseStripHint")}</p>
        <div className="settings-section__group">
          <TaskbarEntryList
            entries={settings.taskbarTooltipEntries ?? []}
            providerChoices={providerChoices}
            windowOptionsFor={windowOptionsFor}
            windowLabelKeys={WINDOW_LABEL_KEYS}
            newEntryWindow="session"
            maxEntries={MAX_ENTRIES}
            disabled={saving || !enabled}
            onChange={(next) => set({ taskbarTooltipEntries: next })}
          />
        </div>
      </section>

      <section className="settings-section">
        <div className="settings-section-heading">
          <h3 className="settings-section__title">{t("TaskbarEntriesLabel")}</h3>
          <button
            type="button"
            className="settings-section-heading__action"
            disabled={saving}
            onClick={() => commitEntries(DEFAULT_ENTRIES)}
          >
            {t("TaskbarEntriesReset")}
          </button>
        </div>
        <p className="settings-section__hint">{t("TaskbarEntriesHelper")}</p>
        <div className="settings-section__group">
          <TaskbarEntryList
            entries={entries}
            providerChoices={providerChoices}
            windowOptionsFor={windowOptionsFor}
            windowLabelKeys={WINDOW_LABEL_KEYS}
            newEntryWindow="weekly"
            maxEntries={MAX_ENTRIES}
            hiddenFrom={VISIBLE_LINES}
            /* The strip always paints at least one line; an empty list would
               make it disappear with no way back from this page. */
            minEntries={1}
            disabled={saving || !enabled}
            onChange={commitEntries}
          />
        </div>
      </section>

      <section className="settings-section">
        <div className="settings-section-heading">
          <h3 className="settings-section__title">
            {t("TaskbarWidgetTypographySection")}
          </h3>
          <button
            type="button"
            className="settings-section-heading__action"
            disabled={saving}
            onClick={resetAppearance}
          >
            {t("TaskbarWidgetResetAppearance")}
          </button>
        </div>
        <div className="settings-section__group">
          {/* Shared with the right-click menu's block on the 显示 page. Both
              surfaces are drawn by `taskbar_text.rs`, so duplicating these
              controls would mean two copies of the same font-enumeration,
              curation and slider-commit logic drifting apart. */}
          <FontSettingsBlock
            value={{ size: fontSize, family: fontFamily, weight: fontWeight }}
            disabled={saving || !enabled}
            onChange={(patch) =>
              set({
                ...(patch.size !== undefined
                  ? { taskbarWidgetFontSize: patch.size }
                  : {}),
                ...(patch.family !== undefined
                  ? { taskbarWidgetFontFamily: patch.family }
                  : {}),
                ...(patch.weight !== undefined
                  ? { taskbarWidgetFontWeight: patch.weight }
                  : {}),
              })
            }
          />
          <Field
            label={t("TaskbarWidgetWidthLabel")}
            description={t("TaskbarWidgetWidthHelper")}
          >
            <div className="settings-value-with-unit">
              <NumberInput
                value={width}
                min={96}
                max={240}
                step={4}
                disabled={saving || !enabled}
                onChange={(value) => set({ taskbarWidgetWidth: value })}
              />
              <span>px</span>
            </div>
          </Field>
          <Field
            label={t("TaskbarWidgetTextAlignLabel")}
            description={t("TaskbarWidgetTextAlignHelper")}
          >
            <SegmentedControl
              value={textAlign}
              disabled={saving || !enabled}
              options={[
                { value: "left", label: t("TaskbarWidgetTextAlignLeft") },
                { value: "center", label: t("TaskbarWidgetTextAlignCenter") },
                { value: "right", label: t("TaskbarWidgetTextAlignRight") },
              ]}
              onChange={(value) =>
                set({ taskbarWidgetTextAlign: value as TaskbarWidgetTextAlign })
              }
            />
          </Field>
          <Field
            label="图标尺寸"
            description="10 到 18 像素，默认 14"
          >
            {/* TODO(lane-s-i18n): S14 — taskbarWidgetIconSize is not persisted. */}
            <div className="settings-value-with-unit">
              <NumberInput
                value={iconSize}
                min={10}
                max={18}
                step={1}
                disabled={saving || !enabled}
                onChange={setIconSize}
              />
              <span>px</span>
            </div>
          </Field>
          <Field
            label="图标渲染样式"
            description="三种都要做进产品和预览"
          >
            {/* TODO(lane-s-i18n): S15 — taskbarWidgetIconStyle is not persisted. */}
            <SegmentedControl
              value={iconStyle}
              disabled={saving || !enabled}
              options={[
                { value: "pure", label: "纯彩色图标" },
                { value: "badge", label: "微底色胶囊" },
                { value: "solid", label: "实色徽章" },
              ]}
              onChange={(value) =>
                setIconStyle(value as "pure" | "badge" | "solid")
              }
            />
          </Field>
        </div>
        <p className="settings-section__hint">{t("TaskbarWidgetSystemStyleHint")}</p>
      </section>
      </div>

      <PreviewFrame
        kind="taskbar"
        label={t("TaskbarWidgetPreviewLabel")}
        dimmed={!enabled}
      >
        <TaskbarStripPreview
          cells={hasCoreInjection ? previewCells : undefined}
          lines={hasCoreInjection ? undefined : previewLines}
          entries={entries}
          enabled={enabled}
          width={width}
          fontSize={fontSize}
          fontWeight={fontWeightDraft}
          fontFamily={fontFamily}
          textAlign={textAlign}
          iconSize={iconSize}
          iconStyle={iconStyle}
          label={t("TaskbarWidgetPreviewLabel")}
        />
      </PreviewFrame>
      </div>
    </div>
  );
}
