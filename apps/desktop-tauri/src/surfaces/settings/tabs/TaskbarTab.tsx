import { useCallback, useEffect, useMemo, useState } from "react";
import { useLocale } from "../../../hooks/useLocale";
import {
  Field,
  NumberInput,
  SegmentedControl,
  Select,
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
  TrayIconMode,
} from "../../../types/bridge";
import type { TabProps } from "../../Settings";

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
  const replaceEntry = (index: number, patch: Partial<TaskbarEntry>) =>
    commitEntries(
      entries.map((entry, i) => (i === index ? { ...entry, ...patch } : entry)),
    );
  const moveEntry = (index: number, delta: number) => {
    const target = index + delta;
    if (target < 0 || target >= entries.length) return;
    const next = [...entries];
    [next[index], next[target]] = [next[target], next[index]];
    commitEntries(next);
  };
  const [families, setFamilies] = useState<TaskbarFontFamily[]>([]);
  const [showAllFonts, setShowAllFonts] = useState(false);
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

  // Real installed families, straight from DirectWrite. Never a hardcoded list:
  // offering a family this machine lacks would silently fall back to something
  // else at paint time, and the user would have no way to tell.
  useEffect(() => {
    let cancelled = false;
    getTaskbarFontFamilies()
      .then((list) => {
        if (!cancelled) setFamilies(list);
      })
      .catch(() => {
        if (!cancelled) setFamilies([]);
      });
    return () => {
      cancelled = true;
    };
  }, []);

  // Whether the weight slider is genuinely continuous depends on the FONT, not
  // on the renderer. Saying so is the whole point of this task item: a static
  // family makes DirectWrite pick the nearest installed face, so intermediate
  // slider positions would look identical.
  const selectedIsVariable =
    families.find((f) => f.name === fontFamily)?.variableWeight ?? false;

  // A typical Windows install carries ~400 families, nearly all of them symbol,
  // script and per-app faces that have no business in a 12px status strip.
  // Showing that raw list was the complaint; the backend marks a short curated
  // set, and everything else stays one switch away rather than being hidden.
  const recommendedFamilies = useMemo(
    () => families.filter((family) => family.recommended),
    [families],
  );
  const fontOptions = useMemo(() => {
    const listed = showAllFonts ? families : recommendedFamilies;
    return [
      // Keep the persisted family selectable even when it is not installed or
      // not curated, so the control never appears to silently reset it.
      ...(listed.some((family) => family.name === fontFamily)
        ? []
        : [{ value: fontFamily, label: fontFamily }]),
      ...listed.map((family) => {
        const tags = [
          family.variableWeight ? t("TaskbarFontVariableTag") : null,
          family.hasCjk ? null : t("TaskbarFontNoCjkTag"),
        ].filter(Boolean);
        return {
          value: family.name,
          label: tags.length
            ? `${family.name} · ${tags.join(" · ")}`
            : family.name,
        };
      }),
    ];
  }, [families, recommendedFamilies, fontFamily, showAllFonts, t]);

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

  // Column-major, matching `taskbar_widget::cell_rects`: entries 1 and 2 fill
  // the left column, 3 and 4 the right. Filling row-major instead would put
  // entry 2 beside entry 1, so a two-entry strip would preview as one row when
  // the renderer stacks it.
  const previewColumns = useMemo(() => {
    const visible = previewLines.slice(0, VISIBLE_LINES);
    const columns = Math.max(1, Math.ceil(visible.length / 2));
    const rows = Math.max(1, Math.ceil(visible.length / columns));
    return Array.from({ length: columns }, (_, column) =>
      visible.slice(column * rows, column * rows + rows),
    );
  }, [previewLines]);

  useEffect(() => {
    setFontWeightDraft(fontWeight);
  }, [fontWeight]);

  const commitFontWeight = useCallback(() => {
    const next = normalizeFontWeight(fontWeightDraft);
    if (next !== fontWeight) {
      set({ taskbarWidgetFontWeight: next });
    }
  }, [fontWeight, fontWeightDraft, set]);

  const resetAppearance = () => {
    set({
      taskbarWidgetFontSize: 12,
      taskbarWidgetWidth: 132,
      taskbarWidgetFontWeight: 400,
      // The backend sorts best-first — continuous weight and Chinese coverage
      // ahead of everything — so "defaults" means the best font this machine
      // has, not a name hardcoded years ago that may not even be installed.
      taskbarWidgetFontFamily:
        recommendedFamilies[0]?.name ?? "Microsoft YaHei UI",
      taskbarWidgetTextAlign: "left",
    });
  };

  return (
    <>
      <header className="settings-page-head">
        <div>
          <span className="settings-page-head__eyebrow">
            {t("TaskbarSettingsEyebrow")}
          </span>
          <h2 className="settings-page-head__title">{t("TaskbarSettingsTitle")}</h2>
          <p className="settings-page-head__description">
            {t("TaskbarSettingsDescription")}
          </p>
        </div>
        <div className="taskbar-preview" aria-label={t("TaskbarWidgetPreviewLabel")}>
          <span className="taskbar-preview__label">{t("TaskbarWidgetPreviewLabel")}</span>
          <div
            className={`taskbar-preview__strip${enabled ? "" : " is-disabled"}`}
            style={{
              width: `${Math.max(120, Math.min(240, width))}px`,
              fontSize: `${fontSize}px`,
              // `font-variation-settings` drives the `wght` axis directly and is
              // the CSS analogue of the renderer's `SetFontAxisValues`. Plain
              // `font-weight` lets the engine round to the nearest 100, so on a
              // variable font 9 of every 10 slider steps looked identical while
              // dragging. Both are set: the axis for variable families, the
              // keyword for static ones that have no axis to drive.
              fontWeight: fontWeightDraft,
              fontVariationSettings: `"wght" ${fontWeightDraft}`,
              fontFamily,
              textAlign,
            }}
          >
            {previewColumns.map((column, columnIndex) => (
              <div className="taskbar-preview__column" key={columnIndex}>
                {column.map((line, rowIndex) => (
                  <span className="taskbar-preview__line" key={rowIndex}>
                    {line.glyph ? (
                      // The mark is drawn in its own colour by the renderer —
                      // shapes repeat across providers, so the colour is what
                      // identifies one. A monochrome preview would not be one.
                      <span
                        className="taskbar-preview__mark"
                        style={{ color: line.color ?? undefined }}
                      >
                        {line.glyph}
                      </span>
                    ) : null}
                    {line.text}
                  </span>
                ))}
              </div>
            ))}
          </div>
        </div>
      </header>

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
          <Field label={t("ShowAsUsedLabel")} description={t("ShowAsUsedHelper")}>
            <Toggle
              checked={settings.taskbarShowAsUsed}
              disabled={saving || !enabled}
              onChange={(value) => set({ taskbarShowAsUsed: value })}
            />
          </Field>
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
          menu is gone — the strip now shows the same content as the
          notification-area tray menu, built from `build_tray_menu`, so there is
          no per-row list to choose from any more. The control was removed
          rather than left inert; `taskbar_context_menu_actions` is still
          persisted and still normalised, it simply no longer drives anything. */}

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
          <ol className="taskbar-entries">
            {(settings.taskbarTooltipEntries ?? []).map((entry, index) => (
              <li
                key={`tip-${entry.providerId}-${entry.window}-${index}`}
                className="taskbar-entries__row"
              >
                <span className="taskbar-entries__index">{index + 1}</span>
                <Select
                  value={entry.providerId}
                  disabled={saving || !enabled}
                  options={providerChoices.map((choice) => ({
                    value: choice.id,
                    label: choice.label,
                  }))}
                  onChange={(value) => {
                    const next = [...(settings.taskbarTooltipEntries ?? [])];
                    next[index] = { ...next[index], providerId: value };
                    set({ taskbarTooltipEntries: next });
                  }}
                />
                <Select
                  value={entry.window}
                  disabled={saving || !enabled}
                  options={WINDOW_KINDS.map((kind) => ({
                    value: kind,
                    label: t(WINDOW_LABEL_KEYS[kind]),
                  }))}
                  onChange={(value) => {
                    const next = [...(settings.taskbarTooltipEntries ?? [])];
                    next[index] = {
                      ...next[index],
                      window: value as TaskbarWindowKind,
                    };
                    set({ taskbarTooltipEntries: next });
                  }}
                />
                <button
                  type="button"
                  className="is-destructive"
                  aria-label={t("TaskbarEntriesRemove")}
                  disabled={saving || !enabled}
                  onClick={() =>
                    set({
                      taskbarTooltipEntries: (
                        settings.taskbarTooltipEntries ?? []
                      ).filter((_, i) => i !== index),
                    })
                  }
                >
                  ✕
                </button>
              </li>
            ))}
          </ol>
          <button
            type="button"
            disabled={
              saving ||
              !enabled ||
              (settings.taskbarTooltipEntries ?? []).length >= MAX_ENTRIES
            }
            onClick={() =>
              set({
                taskbarTooltipEntries: [
                  ...(settings.taskbarTooltipEntries ?? []),
                  { providerId: TASKBAR_PROVIDER_AUTO, window: "session" },
                ],
              })
            }
          >
            {t("TaskbarEntriesAdd")}
          </button>
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
          <ol className="taskbar-entries">
          {entries.map((entry, index) => (
            <li
              key={`${entry.providerId}-${entry.window}-${index}`}
              className="taskbar-entries__row"
              /* Entries past the strip's height still render here, marked, so
                 the user can see what is configured but not displayed rather
                 than wondering why a line never appears. */
              data-hidden={index >= VISIBLE_LINES ? "true" : undefined}
            >
              <span className="taskbar-entries__index">{index + 1}</span>
              <Select
                value={entry.providerId}
                disabled={saving || !enabled}
                options={providerChoices.map((choice) => ({
                  value: choice.id,
                  label: choice.label,
                }))}
                onChange={(value) => replaceEntry(index, { providerId: value })}
              />
              <Select
                value={entry.window}
                disabled={saving || !enabled}
                options={windowOptionsFor(entry).map((kind) => ({
                  value: kind,
                  label: t(WINDOW_LABEL_KEYS[kind]),
                }))}
                onChange={(value) =>
                  replaceEntry(index, { window: value as TaskbarWindowKind })
                }
              />
              {index >= VISIBLE_LINES && (
                <span className="taskbar-entries__hidden-note">
                  {t("TaskbarEntriesHidden")}
                </span>
              )}
              <span className="taskbar-entries__actions">
                <button
                  type="button"
                  aria-label={t("TaskbarEntriesMoveUp")}
                  disabled={saving || !enabled || index === 0}
                  onClick={() => moveEntry(index, -1)}
                >
                  ↑
                </button>
                <button
                  type="button"
                  aria-label={t("TaskbarEntriesMoveDown")}
                  disabled={saving || !enabled || index === entries.length - 1}
                  onClick={() => moveEntry(index, 1)}
                >
                  ↓
                </button>
                <button
                  type="button"
                  className="is-destructive"
                  aria-label={t("TaskbarEntriesRemove")}
                  disabled={saving || !enabled || entries.length <= 1}
                  onClick={() =>
                    commitEntries(entries.filter((_, i) => i !== index))
                  }
                >
                  ✕
                </button>
              </span>
            </li>
          ))}
          </ol>
          <button
            type="button"
            className="taskbar-entries__add"
            disabled={saving || !enabled || entries.length >= MAX_ENTRIES}
            onClick={() =>
              commitEntries([
                ...entries,
                { providerId: TASKBAR_PROVIDER_AUTO, window: "weekly" },
              ])
            }
          >
            + {t("TaskbarEntriesAdd")}
          </button>
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
        </div>
        <p className="settings-section__hint">{t("TaskbarWidgetSystemStyleHint")}</p>
      </section>

    </>
  );
}
