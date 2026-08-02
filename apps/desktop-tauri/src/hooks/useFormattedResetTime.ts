import { useEffect, useState } from "react";
import { formatResetDisplay, type ResetDisplay } from "../lib/quotaDisplay";
import { useLocale } from "./useLocale";

/** How often a live reset row re-renders. Item C requires at least per-minute. */
const TICK_MS = 30_000;

/**
 * Format a provider's reset timestamp for display.
 *
 * All rules live in `formatResetDisplay`; this hook only supplies the clock and
 * keeps it ticking while the row needs it. Notably it does not refetch anything
 * — the countdown advances purely from local time, so a stale provider snapshot
 * still shows a correct remaining duration.
 *
 * Returns `null` only when there is genuinely nothing to render.
 */
export function useResetDisplay(
  resetsAt: string | null,
  resetDescription: string | null,
  relative: boolean,
): ResetDisplay {
  const { t } = useLocale();
  const [nowMs, setNowMs] = useState(() => Date.now());

  const display = formatResetDisplay({
    resetsAt,
    resetDescription,
    relative,
    t,
    nowMs,
  });

  // Only run a timer for rows whose text can change on its own: a countdown, an
  // absolute time that can cross midnight or expire. Provider-description and
  // unknown rows are static, so they cost nothing.
  const ticking = display.ticking;
  useEffect(() => {
    if (!ticking) return;
    const id = window.setInterval(() => setNowMs(Date.now()), TICK_MS);
    return () => window.clearInterval(id);
  }, [ticking]);

  return display;
}

/**
 * Text-only wrapper kept for the existing call sites that render a plain
 * string. Prefer `useResetDisplay` when the caller wants to style or branch on
 * the reset state (for example to mark an expired window).
 */
export function useFormattedResetTime(
  resetsAt: string | null,
  fallback: string | null,
  relative: boolean,
): string | null {
  const display = useResetDisplay(resetsAt, fallback, relative);
  return display.text || null;
}
