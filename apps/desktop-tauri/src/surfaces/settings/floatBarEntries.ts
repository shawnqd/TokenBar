import { TASKBAR_PROVIDER_AUTO } from "../../types/bridge";
import type { TaskbarEntry } from "../../types/bridge";

/**
 * Frontend-local adapter: the persisted model is still `floatBarProviderIds`.
 * The settings list shows the same provider+window rows as the taskbar. Window
 * kinds are UI-only until a `floatBarEntries` key exists.
 */
export function floatBarEntriesFromIds(
  ids: string[] | undefined,
): TaskbarEntry[] {
  if (!ids || ids.length === 0) {
    return [
      { providerId: TASKBAR_PROVIDER_AUTO, window: "session" },
      { providerId: TASKBAR_PROVIDER_AUTO, window: "weekly" },
    ];
  }
  return ids.map((id) => ({
    providerId: id,
    window: "primary",
  }));
}

export function floatBarIdsFromEntries(entries: TaskbarEntry[]): string[] {
  const specific = entries
    .map((entry) => entry.providerId)
    .filter((id) => id && id !== TASKBAR_PROVIDER_AUTO);
  return [...new Set(specific)];
}
