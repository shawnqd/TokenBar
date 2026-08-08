/**
 * The dashboard's own provider filter (task package item H).
 *
 * `enabledProviders` decides which providers the app polls at all. That is a
 * different question from which ones a given surface should be crowded with,
 * and item H asks each component to answer it for itself: the floating bar has
 * `floatBarProviderIds`, the taskbar strip has its ordered entries, and this is
 * the dashboard's — the tray flyout and the pop-out panel.
 */

/**
 * Resolve which provider ids a dashboard surface should render.
 *
 * `candidateIds` is whatever the surface would have shown without this setting,
 * and differs between the two on purpose: the tray flyout passes
 * `enabledProviders`, the pop-out panel passes the ids it has snapshots for,
 * because that surface has never applied the enabled filter.
 *
 * `filter` is intersected with the candidates rather than used directly, so it
 * can only ever narrow: an id that was filtered *in* but has since been
 * disabled must not come back, and disabling a provider must not require
 * editing this list too.
 *
 * Two cases fall back to every candidate: an empty filter (the default, and
 * what the surfaces did before this key existed) and a filter that survives the
 * intersection with nothing left. The second is the important one — a blank
 * dashboard has no controls on it, so a filter that matches nothing would leave
 * the user staring at an empty window with no way back to the setting from
 * there.
 */
export function resolveDashboardProviderIds(
  candidateIds: string[],
  filter: string[] | undefined | null,
): string[] {
  if (!filter || filter.length === 0) return candidateIds;
  const allowed = new Set(filter);
  const kept = candidateIds.filter((id) => allowed.has(id));
  return kept.length > 0 ? kept : candidateIds;
}
