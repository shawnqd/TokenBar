import { TASKBAR_PROVIDER_AUTO } from "../../../types/bridge";
import type { TaskbarEntry, TaskbarWindowKind } from "../../../types/bridge";

const WINDOWS: { value: TaskbarWindowKind; label: string }[] = [
  { value: "session", label: "会话" },
  { value: "weekly", label: "周" },
  { value: "daily", label: "日" },
  { value: "monthly", label: "月" },
  { value: "balance", label: "余额" },
  { value: "speed", label: "速度" },
  { value: "primary", label: "主窗口" },
];

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
          return (
            <div
              key={`${entry.providerId}-${entry.window}-${index}`}
              className={`s5-entry${hidden ? " ghosted" : ""}`}
            >
              <span className="s5-idx">{index + 1}</span>
              <select
                className="s5-select"
                value={entry.providerId}
                disabled={disabled}
                onChange={(event) =>
                  replace(index, { providerId: event.target.value })
                }
              >
                <option value={TASKBAR_PROVIDER_AUTO}>{followLabel}</option>
                {providers.map((provider) => (
                  <option key={provider.id} value={provider.id}>
                    {provider.label}
                  </option>
                ))}
              </select>
              <select
                className="s5-select"
                value={entry.window}
                disabled={disabled}
                onChange={(event) =>
                  replace(index, {
                    window: event.target.value as TaskbarWindowKind,
                  })
                }
              >
                {WINDOWS.map((window) => (
                  <option key={window.value} value={window.value}>
                    {window.label}
                  </option>
                ))}
              </select>
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
