/**
 * Core-level chart access. Surfaces must not call getProviderChartData
 * directly; this single entry keeps the policy in one place (the enrichment
 * scheduler owns scheduling; this is the read side for previews that ask
 * once). Every provider is asked uniformly; the backend decides how much is
 * real. The chart endpoint is deliberately allowed to return a bounded,
 * history-only bundle while a cold local transcript scan is still running, so
 * this adapter follows it with the dedicated local-usage read before exposing
 * the result to a surface. That keeps a timed-out chart from being mistaken
 * for "no usage".
 */
import {
  getProviderChartData,
  getProviderLocalUsageSummary,
} from "../lib/tauri";
import type {
  ProviderChartData,
  ProviderLocalUsageSummary,
} from "../types/bridge";

const CACHE_TTL_MS = 5 * 60 * 1000;
const chartCache = new Map<string, { data: ProviderChartData | null; at: number }>();
const inflight = new Map<string, Promise<ProviderChartData | null>>();

function emptyChart(providerId: string): ProviderChartData {
  return {
    providerId,
    costHistory: [],
    creditsHistory: [],
    usageBreakdown: [],
    localUsage: null,
  };
}

async function readChartWithLocalUsage(
  providerId: string,
  accountEmail?: string,
): Promise<ProviderChartData | null> {
  let chartData: ProviderChartData | null;
  try {
    chartData = await getProviderChartData(providerId, accountEmail);
  } catch {
    chartData = null;
  }

  // `get_provider_chart_data` has a short interactive budget for the history
  // walk. A null localUsage therefore means "not enriched yet", not "zero".
  // The dedicated command is the authoritative local-log read and may finish
  // after the bounded chart request. If the chart itself failed, retain the
  // real summary in an empty structural bundle so token usage is still shown.
  if (chartData?.localUsage == null || chartData == null) {
    let summary: ProviderLocalUsageSummary | null = null;
    try {
      summary = await getProviderLocalUsageSummary(providerId);
    } catch {
      // A missing/failed enrichment read leaves the real chart bundle intact;
      // the next scheduler pass may retry it.
    }
    if (summary) {
      chartData = {
        ...(chartData ?? emptyChart(providerId)),
        localUsage: summary,
      };
    }
  }

  return chartData;
}

export async function defaultChartLoader(
  providerId: string,
  accountEmail?: string,
): Promise<ProviderChartData | null> {
  const key = `${providerId}::${accountEmail ?? ""}`;
  const isTest = typeof import.meta !== "undefined" && (import.meta as { env?: { MODE?: string } }).env?.MODE === "test";
  const hit = chartCache.get(key);
  if (!isTest && hit && Date.now() - hit.at < CACHE_TTL_MS) return hit.data;
  const pending = inflight.get(key);
  if (pending) return pending;

  const request = readChartWithLocalUsage(providerId, accountEmail);
  inflight.set(key, request);
  try {
    const data = await request;
    if (!isTest) chartCache.set(key, { data, at: Date.now() });
    return data;
  } finally {
    if (inflight.get(key) === request) inflight.delete(key);
  }
}

export function clearChartCache(): void {
  chartCache.clear();
  inflight.clear();
}
