/**
 * Core-level chart access. Surfaces must not call getProviderChartData
 * directly; this single entry keeps the policy in one place (the enrichment
 * scheduler owns scheduling; this is the read side for previews that ask
 * once). Every provider is asked uniformly; the backend decides how much is
 * real, and failures return null (slots stay empty — never a fake value).
 */
import { getProviderChartData } from "../lib/tauri";
import type { ProviderChartData } from "../types/bridge";

const CACHE_TTL_MS = 5 * 60 * 1000;
const chartCache = new Map<string, { data: ProviderChartData | null; at: number }>();

export async function defaultChartLoader(
  providerId: string,
  accountEmail?: string,
): Promise<ProviderChartData | null> {
  const key = `${providerId}::${accountEmail ?? ""}`;
  const isTest = typeof import.meta !== "undefined" && (import.meta as { env?: { MODE?: string } }).env?.MODE === "test";
  const hit = chartCache.get(key);
  if (!isTest && hit && Date.now() - hit.at < CACHE_TTL_MS) return hit.data;
  try {
    const data = await getProviderChartData(providerId, accountEmail);
    if (!isTest) chartCache.set(key, { data, at: Date.now() });
    return data;
  } catch {
    chartCache.delete(key);
    return null;
  }
}

export function clearChartCache(): void {
  chartCache.clear();
}
