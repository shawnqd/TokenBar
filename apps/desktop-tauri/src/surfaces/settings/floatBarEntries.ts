import { TASKBAR_PROVIDER_AUTO } from "../../types/bridge";
import type { SettingsSnapshot, TaskbarEntry } from "../../types/bridge";

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

/**
 * Runtime resolver: floatBarEntries is the source of truth when present.
 * Legacy floatBarProviderIds is only read here for migration.
 * Returns the effective entry list; empty persisted entries fall back to
 * the auto follow-enabled placeholder so the bar still shows something.
 */
export function resolveFloatBarEntries(
  settings: Pick<SettingsSnapshot, "floatBarEntries" | "floatBarProviderIds">,
): TaskbarEntry[] {
  const direct = settings.floatBarEntries;
  if (Array.isArray(direct)) {
    if (direct.length > 0) return direct;
    // explicit empty array (e.g. after reset) → follow-enabled default
    return floatBarEntriesFromIds([]);
  }
  return floatBarEntriesFromIds(settings.floatBarProviderIds);
}

/**
 * Expand auto entries positionally to enabled providers.
 * Each `auto` slot consumes the next enabled provider in order.
 * Specific entries are kept only when the provider is enabled.
 */
export function expandFloatBarEntries(
  entries: TaskbarEntry[],
  enabledProviders: string[],
): TaskbarEntry[] {
  if (!entries || entries.length === 0) return [];
  const enabledSet = new Set(enabledProviders);
  const expanded: TaskbarEntry[] = [];
  let autoIdx = 0;
  for (const entry of entries) {
    if (entry.providerId === TASKBAR_PROVIDER_AUTO) {
      const providerId = enabledProviders[autoIdx];
      if (providerId && enabledSet.has(providerId)) {
        expanded.push({ providerId, window: entry.window });
        autoIdx += 1;
      }
    } else if (enabledSet.has(entry.providerId)) {
      expanded.push(entry);
    }
  }
  return expanded;
}
