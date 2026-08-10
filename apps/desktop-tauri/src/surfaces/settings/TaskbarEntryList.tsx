import type { CSSProperties } from "react";
import { Select } from "../../components/FormControls";
import { providerIndexBadge } from "../../components/charts/chartPalette";
import { useLocale } from "../../hooks/useLocale";
import type { TaskbarEntry, TaskbarWindowKind } from "../../types/bridge";
import type { LocaleKey } from "../../i18n/keys";

interface ProviderChoice {
  id: string;
  label: string;
}

interface Props {
  entries: TaskbarEntry[];
  providerChoices: ProviderChoice[];
  /** Which quota windows this entry's provider can actually answer for. */
  windowOptionsFor: (entry: TaskbarEntry) => TaskbarWindowKind[];
  windowLabelKeys: Record<TaskbarWindowKind, LocaleKey>;
  onChange: (entries: TaskbarEntry[]) => void;
  /** Window used by a freshly added row. */
  newEntryWindow: TaskbarWindowKind;
  maxEntries: number;
  /** Rows at or past this index are marked as configured but not displayed. */
  hiddenFrom?: number;
  /** Below this the remove button is disabled. 0 lets the list be emptied. */
  minEntries?: number;
  disabled?: boolean;
}

/**
 * The ordered "provider + quota window" list, shared by the strip's entries and
 * its hover tooltip's.
 *
 * Layout is the B "table header" direction: a column grid
 * (# / provider / window / note / actions) so the two dropdowns align down the
 * list instead of stretching into empty flex space. Index badges take the
 * provider's brand colour; near-black / near-white brands (grok, …) invert via
 * `data-tone` so the digit stays readable on both themes.
 */
export default function TaskbarEntryList({
  entries,
  providerChoices,
  windowOptionsFor,
  windowLabelKeys,
  onChange,
  newEntryWindow,
  maxEntries,
  hiddenFrom,
  minEntries = 0,
  disabled,
}: Props) {
  const { t } = useLocale();

  const replace = (index: number, patch: Partial<TaskbarEntry>) => {
    const next = [...entries];
    next[index] = { ...next[index], ...patch };
    onChange(next);
  };

  const move = (index: number, delta: number) => {
    const target = index + delta;
    if (target < 0 || target >= entries.length) return;
    const next = [...entries];
    [next[index], next[target]] = [next[target], next[index]];
    onChange(next);
  };

  return (
    <div className="taskbar-entries">
      <div className="taskbar-entries__head" aria-hidden>
        <span className="taskbar-entries__head-cell">#</span>
        <span className="taskbar-entries__head-cell">
          {t("TaskbarEntriesColProvider")}
        </span>
        <span className="taskbar-entries__head-cell">
          {t("TaskbarEntriesColWindow")}
        </span>
        <span className="taskbar-entries__head-cell taskbar-entries__head-cell--actions">
          {t("TaskbarEntriesColActions")}
        </span>
      </div>
      <ol className="taskbar-entries__list">
        {entries.map((entry, index) => {
          const hidden = hiddenFrom !== undefined && index >= hiddenFrom;
          const badge = providerIndexBadge(entry.providerId);
          const indexStyle = {
            "--entry-idx-color": badge.color,
          } as CSSProperties;
          return (
            <li
              key={`${entry.providerId}-${entry.window}-${index}`}
              className="taskbar-entries__row"
              /* Entries past the surface's capacity still render, marked, so
                 the user can see what is configured but not displayed rather
                 than wondering why a line never appears. */
              data-hidden={hidden ? "true" : undefined}
            >
              <span
                className="taskbar-entries__index"
                data-tone={badge.tone === "normal" ? undefined : badge.tone}
                style={indexStyle}
              >
                {index + 1}
              </span>
              <Select
                value={entry.providerId}
                disabled={disabled}
                options={providerChoices.map((choice) => ({
                  value: choice.id,
                  label: choice.label,
                }))}
                onChange={(value) => replace(index, { providerId: value })}
              />
              <Select
                value={entry.window}
                disabled={disabled}
                options={windowOptionsFor(entry).map((kind) => ({
                  value: kind,
                  label: t(windowLabelKeys[kind]),
                }))}
                onChange={(value) =>
                  replace(index, { window: value as TaskbarWindowKind })
                }
              />
              {hidden ? (
                <span className="taskbar-entries__hidden-note">
                  {t("TaskbarEntriesHidden")}
                </span>
              ) : (
                <span className="taskbar-entries__hidden-note taskbar-entries__hidden-note--empty" />
              )}
              <span className="taskbar-entries__actions">
                <button
                  type="button"
                  aria-label={t("TaskbarEntriesMoveUp")}
                  disabled={disabled || index === 0}
                  onClick={() => move(index, -1)}
                >
                  ↑
                </button>
                <button
                  type="button"
                  aria-label={t("TaskbarEntriesMoveDown")}
                  disabled={disabled || index === entries.length - 1}
                  onClick={() => move(index, 1)}
                >
                  ↓
                </button>
                <button
                  type="button"
                  className="is-destructive"
                  aria-label={t("TaskbarEntriesRemove")}
                  disabled={disabled || entries.length <= minEntries}
                  onClick={() =>
                    onChange(entries.filter((_, i) => i !== index))
                  }
                >
                  ✕
                </button>
              </span>
            </li>
          );
        })}
      </ol>
      <button
        type="button"
        className="taskbar-entries__add"
        disabled={disabled || entries.length >= maxEntries}
        onClick={() =>
          onChange([
            ...entries,
            {
              providerId: providerChoices[0]?.id ?? "auto",
              window: newEntryWindow,
            },
          ])
        }
      >
        + {t("TaskbarEntriesAdd")}
      </button>
    </div>
  );
}
