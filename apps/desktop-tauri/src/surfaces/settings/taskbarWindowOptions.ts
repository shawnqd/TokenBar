import { useEffect, useState } from "react";
import { getTaskbarWindowAvailability } from "../../lib/tauri";
import { TASKBAR_PROVIDER_AUTO } from "../../types/bridge";
import type {
  Language,
  TaskbarEntry,
  TaskbarWindowKind,
} from "../../types/bridge";
import type { LocaleKey } from "../../i18n/keys";

/**
 * Window kinds that are meaningful choices for a user to pin.
 *
 * `primary` remains a persisted/runtime compatibility value for older
 * settings, but it is an internal fallback rather than a product concept.
 * The settings UI must expose the actual named cycle (5h/day/week/month), a
 * real balance, or a measured speed instead.
 */
export type ConfigurableTaskbarWindowKind = Exclude<
  TaskbarWindowKind,
  "primary"
>;

export const CONFIGURABLE_TASKBAR_WINDOWS: ConfigurableTaskbarWindowKind[] = [
  "session",
  "weekly",
  "daily",
  "monthly",
  "balance",
  "speed",
];

export const TASKBAR_WINDOW_LABEL_KEYS: Record<
  ConfigurableTaskbarWindowKind,
  LocaleKey
> = {
  session: "TaskbarWindowSession",
  daily: "TaskbarWindowDaily",
  weekly: "TaskbarWindowWeekly",
  monthly: "TaskbarWindowMonthly",
  balance: "TaskbarWindowBalance",
  speed: "TaskbarWindowSpeed",
};

/** Labels used by the V5 HTML-derived settings surface before locale wiring. */
export const TASKBAR_WINDOW_LABELS_ZH: Record<
  ConfigurableTaskbarWindowKind,
  string
> = {
  session: "5h",
  daily: "日",
  weekly: "周",
  monthly: "月",
  balance: "余额",
  speed: "速度",
};

/**
 * The native strip deliberately uses one-character Chinese tags for balance
 * and speed to stay compact. The settings control has room for the real
 * product names, so expand only those abbreviated translations here instead
 * of changing the native strip vocabulary or inventing a second window kind.
 */
const SETTINGS_LABEL_EXPANSIONS: Partial<
  Record<Language, Partial<Record<ConfigurableTaskbarWindowKind, string>>>
> = {
  chinese: { balance: "余额", speed: "速度" },
  chinesetraditional: { balance: "餘額", speed: "速度" },
};

export function taskbarWindowLabelFor(
  kind: TaskbarWindowKind,
  t: (key: LocaleKey) => string,
  language: Language,
): string {
  if (kind === "primary") return kind;
  const expanded = SETTINGS_LABEL_EXPANSIONS[language]?.[kind];
  if (expanded) return expanded;
  const key = TASKBAR_WINDOW_LABEL_KEYS[kind];
  return key ? t(key) : kind;
}

export type TaskbarWindowAvailability = Record<
  string,
  TaskbarWindowKind[]
>;

/**
 * Filter the shared window vocabulary by the windows a provider actually
 * published. A null map means the backend answer is still loading, so retain
 * the vocabulary long enough for an existing setting to remain editable.
 * Callers may disable stale-selection preservation when the UI must show only
 * currently supported choices.
 */
export function taskbarWindowOptionsFor(
  entry: TaskbarEntry,
  availability: TaskbarWindowAvailability | null,
  options: { preserveSelection?: boolean } = {},
): ConfigurableTaskbarWindowKind[] {
  if (!availability) return [...CONFIGURABLE_TASKBAR_WINDOWS];

  const allowed =
    entry.providerId === TASKBAR_PROVIDER_AUTO
      ? new Set(Object.values(availability).flat())
      : new Set(availability[entry.providerId] ?? []);

  const offered = CONFIGURABLE_TASKBAR_WINDOWS.filter((kind) => allowed.has(kind));
  // Keep an already-saved named choice editable while a provider is between
  // refreshes or has just stopped publishing that window. `primary` is not
  // retained here: it is an internal compatibility fallback and must never
  // reappear as a user-facing option.
  if (
    options.preserveSelection !== false &&
    entry.window !== "primary" &&
    CONFIGURABLE_TASKBAR_WINDOWS.includes(entry.window) &&
    !offered.includes(entry.window)
  ) {
    offered.push(entry.window);
  }
  return offered;
}

/** Read the same native availability map used by the running taskbar strip. */
export function useTaskbarWindowAvailability(enabled = true) {
  const [availability, setAvailability] =
    useState<TaskbarWindowAvailability | null>(null);

  useEffect(() => {
    if (!enabled) {
      setAvailability(null);
      return undefined;
    }

    let cancelled = false;
    void getTaskbarWindowAvailability()
      .then((map) => {
        if (!cancelled) setAvailability(map);
      })
      .catch(() => {
        // Keep the loading vocabulary on a transient bridge failure. The
        // runtime still rejects an unsupported selection honestly; this only
        // prevents a settings control from becoming unusable because the
        // availability probe was temporarily unavailable.
        if (!cancelled) setAvailability(null);
      });

    return () => {
      cancelled = true;
    };
  }, [enabled]);

  return availability;
}
