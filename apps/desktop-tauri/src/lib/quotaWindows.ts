import type {
  FloatBarResetWindow,
  ProviderUsageSnapshot,
  RateWindowSnapshot,
} from "../types/bridge";

/**
 * Selecting a provider's quota window by cycle.
 *
 * Providers disagree about which slot carries which cycle — Codex reports its
 * weekly quota as `primary` while Claude's `primary` is the 5-hour session — so
 * a window is matched by what it IS, never by slot position. Reading slots
 * positionally is exactly the bug that once made Claude's session usage render
 * as its weekly figure.
 *
 * What a window is comes from `window.kind`, decided once in Rust when the
 * snapshot is built (`src-tauri/src/quota_cycle.rs`). This module used to
 * re-derive it from `windowMinutes` against its own copy of the duration bands,
 * and the copy drifted: it matched on length ALONE, so a provider publishing a
 * percentage with no declared length matched no cycle and the floating bar
 * showed nothing where the taskbar strip showed a reading.
 */

/** Every real quota window a provider published, in slot order. */
function allWindows(provider: ProviderUsageSnapshot): RateWindowSnapshot[] {
  return [
    provider.primary,
    provider.secondary,
    provider.tertiary,
    ...provider.extraRateWindows.map((extra) => extra.window),
  ].filter(
    (window): window is RateWindowSnapshot =>
      window != null && !window.isInformational,
  );
}

/**
 * Resolve one window kind against a provider, or null when it publishes none.
 *
 * Returning null rather than a fallback window is the point: a provider without
 * a weekly cycle must show nothing for "weekly", not somebody else's number.
 */
export function windowByKind(
  provider: ProviderUsageSnapshot,
  kind: FloatBarResetWindow,
): RateWindowSnapshot | null {
  if (kind === "primary") {
    // "This provider's main quota, whatever cycle it is." Balance carriers are
    // skipped: a prepaid provider synthesises a 0%/100% window purely to smuggle
    // its amount through `resetDescription`, and it is NOT flagged
    // informational, so taking the primary slot on trust would print a reset
    // time belonging to a window that is not a cycle at all. Reset-credit
    // carriers get the same treatment (UP-W-015).
    return (
      allWindows(provider).find(
        (w) => !looksLikeBalance(w) && !looksLikeResetCreditCarrier(w),
      ) ?? null
    );
  }
  return allWindows(provider).find((window) => window.kind === kind) ?? null;
}

/**
 * Whether a window is really a prepaid balance wearing a rate window's clothes.
 *
 * Detected by the shape of the text, not by a provider list, so a new prepaid
 * provider is covered on arrival. Mirrors the same rule in
 * `taskbar_entries::parse_balance_amount`; kept deliberately crude because the
 * only decision it drives is "skip this one".
 */
function looksLikeBalance(window: RateWindowSnapshot): boolean {
  const text = window.resetDescription?.trim();
  if (!text || !/\d/.test(text)) return false;
  return /[¥￥$]|CNY|USD/i.test(text) && !/unavailable|余额不可用/i.test(text);
}

/**
 * Whether a window is the Codex reset-credit counter wearing a rate window's
 * clothes.
 *
 * Rust flags the live reset-credits row informational (UP-W-015), but cached
 * snapshots that predate the flag carry it as an unflagged 0% window whose
 * description is exactly "N reset credits available". Detecting the carrier by
 * its text keeps the primary lookup from reading a stale placeholder as a real
 * "0% used" quota, without needing the provider list.
 */
function looksLikeResetCreditCarrier(window: RateWindowSnapshot): boolean {
  const text = window.resetDescription?.trim();
  return Boolean(text && /^\d+\s+reset credits? available$/i.test(text));
}
