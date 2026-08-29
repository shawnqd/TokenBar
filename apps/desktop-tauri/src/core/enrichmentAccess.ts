import { getProviderLocalUsageSummary } from "../lib/tauri";

/**
 * Read-only enrichment adapter. Rust owns the local-usage cache and its
 * process-level scan lock; a WebView may request a value but cannot create or
 * mutate an enrichment store.
 */
export async function readProviderLocalCost(
  providerId: string,
): Promise<{ todayCost: number; thirtyDayCost: number } | null> {
  try {
    const summary = await getProviderLocalUsageSummary(providerId);
    if (
      summary == null ||
      summary.todayCost == null && summary.thirtyDayCost == null
    ) {
      return null;
    }
    return {
      todayCost: summary.todayCost ?? 0,
      thirtyDayCost: summary.thirtyDayCost ?? 0,
    };
  } catch {
    return null;
  }
}
