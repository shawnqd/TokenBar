/**
 * Whether the UI should fetch chart / local-usage data for a provider.
 *
 * Every provider is treated identically here: all of them go through the same
 * `get_provider_chart_data` fetch, and the backend decides how much is real.
 * `scan_local_cost` only parses the local logs we can read (Codex / Claude /
 * Grok today) and every other provider gets an empty/zero result back, so the
 * slot simply stays empty — never a synthetic zero. This replaces the old
 * per-provider allow-list, which drifted from the backend once already (Grok's
 * scanner existed before the frontend list mentioned it) and hid the whole
 * recent-usage block for every unlisted provider.
 */
export function providerSupportsChartData(_providerId: string): boolean {
  return true;
}
