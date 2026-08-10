/**
 * Decide whether a quota row should be rendered when a surface supplies an
 * explicit cycle filter (for example, the floating bar or taskbar).
 *
 * The dashboard deliberately passes no filter: it adapts to the windows each
 * provider actually returns. Keeping this generic helper separate prevents a
 * dashboard-only setting from becoming another source of truth.
 */
export function dashboardShowsQuotaWindow(
  kind: string | null | undefined,
  filter: string[] | undefined | null,
): boolean {
  if (!filter || filter.length === 0) return true;
  if (kind == null) return true;
  return filter.includes(kind);
}
