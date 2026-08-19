import { useCallback, useEffect, useState } from "react";
import { useLocale } from "../../../hooks/useLocale";
import {
  Field,
  NumberInput,
  SegmentedControl,
  Toggle,
} from "../../../components/FormControls";
import {
  getTaskbarFontFamilies,
  getTaskbarPreviewLines,
  getTaskbarWindowAvailability,
} from "../../../lib/tauri";
import FontSettingsBlock, {
  normalizeFontWeight,
} from "../FontSettingsBlock";
import { TASKBAR_PROVIDER_AUTO } from "../../../types/bridge";
import type {
  TaskbarEntry,
  TaskbarFontFamily,
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

export default function TaskbarTab({ settings, set, saving }: TabProps) {
  const { t } = useLocale();
  const enabled = settings.taskbarWidgetEnabled;
  const fontSize = settings.taskbarWidgetFontSize ?? 12;
  const width = settings.taskbarWidgetWidth ?? 132;
  const fontWeight = normalizeFontWeight(settings.taskbarWidgetFontWeight ?? 400);
  const [fontWeightDraft, setFontWeightDraft] = useState(fontWeight);
  const textAlign = settings.taskbarWidgetTextAlign ?? "left";
  const fontFamily = settings.taskbarWidgetFontFamily ?? "Microsoft YaHei UI";
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
  /** Best continuous-weight family for "restore appearance" — not the full list. */
  const [defaultVariableFamily, setDefaultVariableFamily] = useState<string | null>(
    null,
  );
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
  useEffect(() => {
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
  }, []);

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

  // Best continuous-weight family for restore-defaults. The full picker list
  // lives inside FontSettingsBlock so this tab does not re-derive curation.
  useEffect(() => {
    let cancelled = false;
    getTaskbarFontFamilies()
      .then((list: TaskbarFontFamily[]) => {
        if (cancelled) return;
        const best =
          list.find((f) => f.recommended && f.variableWeight)?.name ??
          list.find((f) => f.variableWeight)?.name ??
          null;
        setDefaultVariableFamily(best);
      })
      .catch(() => {
        if (!cancelled) setDefaultVariableFamily(null);
      });
    return () => {
      cancelled = true;
    };
  }, []);

  // The preview shows the renderer's OWN lines, fetched from the native strip,
  // rather than an imitation built from sample numbers here.
  //
  // The imitation was wrong in two ways the user could see at a glance, because
  // it had to restate rules that live in Rust: it printed a fixed 18% for every
  // weekly entry while the strip beside it printed the real 47%, and it printed
  // "unsupported" for every balance entry long after balances began rendering.
  // Anything that re-derives what it previews drifts the moment either side
  // changes; reading the buffer makes that impossible instead of unlikely.
  const [previewLines, setPreviewLines] = useState<TaskbarPreviewLine[]>([]);
  useEffect(() => {
    let cancelled = false;
    // `update_settings` refreshes the tray presentation before it resolves, so
    // by the time an edit lands in `settings` the buffer already holds the new
    // lines — no polling, and no window where the preview shows the old ones.
    getTaskbarPreviewLines()
      .then((lines) => {
        if (!cancelled) setPreviewLines(lines);
      })
      .catch(() => {
        if (!cancelled) setPreviewLines([]);
      });
    return () => {
      cancelled = true;
    };
  }, [entries, settings]);

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
      // Prefer the best genuine continuous-weight face this machine has.
      // Static Microsoft faces are no longer the default — the weight slider
      // only does real work on a variable font.
      taskbarWidgetFontFamily:
        defaultVariableFamily ?? "Segoe UI Variable",
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
          lines={previewLines}
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
