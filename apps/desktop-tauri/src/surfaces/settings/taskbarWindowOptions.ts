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
 * Named windows the composer always knows how to spell.
 *
 * `primary` is not in this list: official plans already publish 5h/day/week/
 * month, and listing both "主额度" and "周" for the same reading is the
 * confusion the filter exists to prevent. One-time grants with no cycle name
 * (GLM 体验套餐 / ZCode Start Plan) are the exception — the native
 * availability map offers `primary` only then, and the composer follows it.
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

export const TASKBAR_WINDOW_LABEL_KEYS: Record<TaskbarWindowKind, LocaleKey> = {
  primary: "TaskbarWindowPrimary",
  session: "TaskbarWindowSession",
  daily: "TaskbarWindowDaily",
  weekly: "TaskbarWindowWeekly",
  monthly: "TaskbarWindowMonthly",
  balance: "TaskbarWindowBalance",
  speed: "TaskbarWindowSpeed",
};

/** Labels used by the V5 HTML-derived settings surface before locale wiring. */
export const TASKBAR_WINDOW_LABELS_ZH: Record<TaskbarWindowKind, string> = {
  primary: "主额度",
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
  providerId?: string,
): string {
  if (kind === "primary") {
    // Native only offers `primary` when the grant has no 5h/周/日/月 name.
    // For z.ai that grant is the ZCode Start Plan, whose product name the
    // tray already prints; keep the same words in the composer.
    if (providerId === "zai") {
      return language === "chinesetraditional" ? "體驗套餐" : "体验套餐";
    }
    return t("TaskbarWindowPrimary");
  }
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
): TaskbarWindowKind[] {
  if (!availability) {
    // Keep a live unnamed-grant selection visible while the native map loads,
    // otherwise the trigger flashes the first named cycle ("5h") and snaps back.
    const loading: TaskbarWindowKind[] = [...CONFIGURABLE_TASKBAR_WINDOWS];
    if (entry.window === "primary") insertPrimaryBeforeBalance(loading);
    return loading;
  }

  const allowed =
    entry.providerId === TASKBAR_PROVIDER_AUTO
      ? new Set(Object.values(availability).flat())
      : new Set(availability[entry.providerId] ?? []);

  const offered: TaskbarWindowKind[] = CONFIGURABLE_TASKBAR_WINDOWS.filter((kind) =>
    allowed.has(kind),
  );
  if (allowed.has("primary")) insertPrimaryBeforeBalance(offered);
  // Keep an already-saved named choice editable while a provider is between
  // refreshes or has just stopped publishing that window. A stored `primary`
  // is only kept when the live map still offers it — once the grant becomes
  // a named cycle, resurrecting "主额度" next to "周" is the old confusion.
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

function insertPrimaryBeforeBalance(offered: TaskbarWindowKind[]): void {
  if (offered.includes("primary")) return;
  const balanceAt = offered.indexOf("balance");
  if (balanceAt === -1) offered.push("primary");
  else offered.splice(balanceAt, 0, "primary");
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
