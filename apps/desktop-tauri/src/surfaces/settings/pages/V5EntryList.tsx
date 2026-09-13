import { TASKBAR_PROVIDER_AUTO } from "../../../types/bridge";
import type { TaskbarEntry, TaskbarWindowKind } from "../../../types/bridge";
import {
  CONFIGURABLE_TASKBAR_WINDOWS,
  TASKBAR_WINDOW_LABELS_ZH,
} from "../taskbarWindowOptions";
import { V5Select } from "./v5Controls";

export interface EntryProviderChoice {
  id: string;
  label: string;
}

export function V5EntryList({
  entries,
  providers,
  followLabel,
  onChange,
  newWindow,
  maxEntries,
  minEntries,
  hiddenFrom,
  addLabel,
  disabled,
  windowOptionsFor,
  windowLabelFor,
}: {
  entries: TaskbarEntry[];
  providers: EntryProviderChoice[];
  followLabel: string;
  onChange: (next: TaskbarEntry[]) => void;
  newWindow: TaskbarWindowKind;
  maxEntries: number;
  minEntries: number;
  hiddenFrom?: number;
  addLabel: string;
  disabled?: boolean;
  /** Provider-aware window list shared with the native taskbar resolver. */
  windowOptionsFor?: (entry: TaskbarEntry) => TaskbarWindowKind[];
  /** Optional localized labels; V5's current copy defaults to Chinese. */
  windowLabelFor?: (kind: TaskbarWindowKind, entry: TaskbarEntry) => string;
}) {
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
    <>
      <div className="s5-entries-head">
        <span>#</span>
        <span>服务商</span>
        <span>窗口</span>
        <span />
        <span>操作</span>
      </div>
      {entries.length === 0 ? (
        <p className="s5-hint">当前为空。可添加最多 {maxEntries} 条。</p>
      ) : (
        entries.map((entry, index) => {
          const hidden = hiddenFrom !== undefined && index >= hiddenFrom;
          const windowKinds = windowOptionsFor
            ? windowOptionsFor(entry)
            : CONFIGURABLE_TASKBAR_WINDOWS;
          // Official plans never list `primary` (they already have 5h/周).
          // One-time grants do, and when the live map offers it the stored
          // value must stay selected instead of being coerced to 余额.
          const selectedWindow = windowKinds.includes(entry.window)
            ? entry.window
            : windowKinds[0] ?? "";
          return (
            <div
              key={`${entry.providerId}-${entry.window}-${index}`}
              className={`s5-entry${hidden ? " ghosted" : ""}`}
            >
              <span className="s5-idx">{index + 1}</span>
              <V5Select
                value={entry.providerId}
                disabled={disabled}
                options={[
                  { value: TASKBAR_PROVIDER_AUTO, label: followLabel },
                  ...providers.map((provider) => ({
                    value: provider.id,
                    label: provider.label,
                  })),
                ]}
                onChange={(value) => replace(index, { providerId: value })}
              />
              <V5Select
                value={selectedWindow}
                disabled={disabled}
                options={windowKinds.map((kind) => ({
                  value: kind,
                  label:
                    windowLabelFor?.(kind, entry) ??
                    TASKBAR_WINDOW_LABELS_ZH[kind] ??
                    kind,
                }))}
                onChange={(value) =>
                  replace(index, {
                    window: value as TaskbarWindowKind,
                  })
                }
              />
              <span>{hidden ? "条带不可见" : ""}</span>
              <span className="s5-entry-actions">
                <button
                  type="button"
                  disabled={disabled || index === 0}
                  onClick={() => move(index, -1)}
                >
                  ↑
                </button>
                <button
                  type="button"
                  disabled={disabled || index === entries.length - 1}
                  onClick={() => move(index, 1)}
                >
                  ↓
                </button>
                <button
                  type="button"
                  disabled={disabled || entries.length <= minEntries}
                  onClick={() =>
                    onChange(entries.filter((_, i) => i !== index))
                  }
                >
                  ✕
                </button>
              </span>
            </div>
          );
        })
      )}
      <div className={`s5-field${disabled ? " is-off" : ""}`}>
        <button
          type="button"
          className="s5-ghost"
          disabled={disabled || entries.length >= maxEntries}
          onClick={() =>
            onChange([
              ...entries,
              { providerId: TASKBAR_PROVIDER_AUTO, window: newWindow },
            ])
          }
        >
          {addLabel}
        </button>
      </div>
    </>
  );
}
