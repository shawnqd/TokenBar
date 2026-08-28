/**
 * Dashboard quota-window filter — kept as a READ-ONLY compatibility helper.
 *
 * The dashboard surface entry has been removed from the app: no surface in
 * this codebase passes a cycle filter today, and nothing here may use a
 * dashboard setting as a render input. This helper survives only because the
 * shared MenuCard still accepts an optional `quotaWindows` filter (its own
 * public API contract), so callers can keep compiling and future dashboard
 * surfaces have a single place to resolve the filter.
 *
 * It performs no I/O, copies no credentials and never decides what the tray
 * panel or any other live surface renders.
 */
export function dashboardShowsQuotaWindow(
  kind: string | null | undefined,
  filter: string[] | undefined | null,
): boolean {
  if (!filter || filter.length === 0) return true;
  if (kind == null) return true;
  return filter.includes(kind);
}