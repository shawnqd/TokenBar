/**
 * Resolve a provider's capability flags.
 *
 * The bridge reports `snapshot.capabilities` from Rust (the single source of
 * truth — it lists which provider session logs we actually parse). When a
 * snapshot is cached before this field existed, or comes from a fixture, we
 * fall back to the same known-parse sets the Rust side uses so the UI never
 * hides a slot that the backend can actually fill (nor shows one it cannot).
 */
import type {
  ProviderCapabilitiesSnapshot,
  ProviderUsageSnapshot,
} from "../types/bridge";

const OUTPUT_SPEED_PARSED = new Set(["codex", "claude", "grok"]);
const LOCAL_USAGE_PARSED = new Set(["codex", "claude", "grok"]);

export function providerCapabilities(
  provider: Pick<ProviderUsageSnapshot, "providerId" | "capabilities">,
): ProviderCapabilitiesSnapshot {
  if (provider.capabilities) return provider.capabilities;
  const id = provider.providerId.toLowerCase();
  return {
    outputSpeed: OUTPUT_SPEED_PARSED.has(id),
    localUsage: LOCAL_USAGE_PARSED.has(id),
    providerDashboard: true,
    statusPage: false,
    login: true,
  };
}
