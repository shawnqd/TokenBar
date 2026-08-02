import type { OutputSpeedSnapshot } from "../types/bridge";

/**
 * Providers whose local logs record enough to measure output speed.
 *
 * Speed is read from the CLI's own session logs, not from any provider API, so
 * a provider only appears here once its log format has actually been decoded —
 * see `commands/output_speed.rs`. Keeping the list in one place is why adding
 * Grok touched the backend and this file rather than every surface that shows a
 * speed readout.
 */
export const OUTPUT_SPEED_PROVIDER_IDS = ["codex", "claude", "grok"] as const;

export type OutputSpeedProviderId = keyof OutputSpeedSnapshot;

/** Narrow a provider id to one the speed snapshot carries, or null. */
export function outputSpeedProviderId(
  providerId: string,
): OutputSpeedProviderId | null {
  return (OUTPUT_SPEED_PROVIDER_IDS as readonly string[]).includes(providerId)
    ? (providerId as OutputSpeedProviderId)
    : null;
}
