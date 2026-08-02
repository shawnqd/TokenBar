/**
 * Which providers have chart or local-usage data worth fetching.
 *
 * This is a capability of OUR code, not a property of the provider: it lists
 * the providers we have written a local-log parser or a dashboard chart loader
 * for. It cannot be derived from a snapshot, so it has to be stated — but it
 * must be stated only once.
 *
 * It is the mirror of two Rust functions:
 *   * `commands::chart::scan_local_cost` — the local session-log scanners
 *   * `commands::chart::load_openai_dashboard_chart_data` — the hosted charts
 *
 * `scripts/check-chart-providers.mjs` fails the build when this set and those
 * two disagree. That check exists because they did: Grok's local-usage scanner
 * was added in Rust while this set still said `claude, codex, openai`, so the
 * frontend never asked for the data and the card stayed empty with a working
 * backend behind it.
 */
const PROVIDER_CHART_DATA_IDS = new Set(["claude", "codex", "grok", "openai"]);

export function providerSupportsChartData(providerId: string): boolean {
  return PROVIDER_CHART_DATA_IDS.has(providerId.toLowerCase());
}
