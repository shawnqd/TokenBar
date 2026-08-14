import { describe, expect, it } from "vitest";
import { windowByKind } from "./quotaWindows";
import type {
  ProviderUsageSnapshot,
  QuotaCycleKind,
  RateWindowSnapshot,
} from "../types/bridge";

function rateWindow(
  kind: QuotaCycleKind | null,
  overrides: Partial<RateWindowSnapshot> = {},
): RateWindowSnapshot {
  return {
    usedPercent: 42,
    remainingPercent: 58,
    kind,
    windowMinutes: null,
    resetsAt: null,
    resetDescription: null,
    isExhausted: false,
    reservePercent: null,
    reserveDescription: null,
    ...overrides,
  };
}

function provider(
  primary: RateWindowSnapshot,
  rest: Partial<ProviderUsageSnapshot> = {},
): ProviderUsageSnapshot {
  return {
    providerId: "p",
    displayName: "P",
    primary,
    primaryLabel: null,
    secondary: null,
    modelSpecific: null,
    tertiary: null,
    extraRateWindows: [],
    cost: null,
    planName: null,
    accountEmail: null,
    sourceLabel: "auto",
    updatedAt: "2026-08-01T00:00:00Z",
    error: null,
    pace: null,
    accountOrganization: null,
    trayStatusLabel: null,
    fetchDurationMs: null,
    ...rest,
  } as ProviderUsageSnapshot;
}

describe("windowByKind", () => {
  it("matches the cycle the backend decided, not the slot position", () => {
    // Claude's shape: the 5-hour session leads, the weekly sits in `secondary`.
    const claude = provider(rateWindow("session"), {
      secondary: rateWindow("weekly", { usedPercent: 70 }),
    });

    expect(windowByKind(claude, "session")?.usedPercent).toBe(42);
    expect(windowByKind(claude, "weekly")?.usedPercent).toBe(70);
    expect(windowByKind(claude, "monthly")).toBeNull();
  });

  /**
   * The drift this module was rewritten to remove. It used to match on
   * `windowMinutes` alone, so a provider that reports a percentage without
   * declaring a length matched no cycle at all — the floating bar showed
   * nothing where the taskbar strip, which asks the provider's slot name as a
   * fallback, showed a reading. The classification now happens once in Rust and
   * arrives as `kind`, so both surfaces get the same answer.
   */
  it("resolves a window whose provider declared no length", () => {
    const noLength = provider(rateWindow("monthly", { windowMinutes: null }));
    expect(windowByKind(noLength, "monthly")?.usedPercent).toBe(42);
  });

  /**
   * The other drift: a monthly window used to satisfy an open-ended
   * `minutes >= 7 days` weekly test, so one window answered to two cycles.
   */
  it("never lets one window answer to two cycles", () => {
    const monthly = provider(
      rateWindow("monthly", { windowMinutes: 30 * 24 * 60 }),
    );
    expect(windowByKind(monthly, "monthly")).not.toBeNull();
    expect(windowByKind(monthly, "weekly")).toBeNull();
  });

  it("returns nothing rather than somebody else's window", () => {
    const sessionOnly = provider(rateWindow("session"));
    expect(windowByKind(sessionOnly, "weekly")).toBeNull();
    expect(windowByKind(sessionOnly, "daily")).toBeNull();
  });

  it("skips informational rows, which are not quotas", () => {
    const informational = provider(
      rateWindow("weekly", { isInformational: true }),
    );
    expect(windowByKind(informational, "weekly")).toBeNull();
  });

  describe("primary", () => {
    it("is whatever the provider leads with, cycle or not", () => {
      const unnamed = provider(rateWindow(null));
      expect(windowByKind(unnamed, "primary")?.usedPercent).toBe(42);
    });

    /**
     * A prepaid provider synthesises a 0%/100% window purely to smuggle its
     * amount through `resetDescription`, and it is NOT flagged informational.
     * Taking the primary slot on trust printed a reset time for a window that
     * is not a cycle at all. Rust already skipped these; this module did not.
     */
    it("skips a balance carrier masquerading as a window", () => {
      const prepaid = provider(
        rateWindow(null, {
          usedPercent: 0,
          resetDescription: "¥38.88 (Paid: ¥38.88 / Granted: ¥0.00)",
        }),
        { secondary: rateWindow("monthly", { usedPercent: 12 }) },
      );
      expect(windowByKind(prepaid, "primary")?.usedPercent).toBe(12);
    });

    it("keeps a real reset description that merely mentions a number", () => {
      const real = provider(
        rateWindow(null, { resetDescription: "Resets on the 1st" }),
      );
      expect(windowByKind(real, "primary")).not.toBeNull();
    });
  });

  describe("Codex weekly quota without a 5-hour window (UP-W-015)", () => {
    /**
     * Codex reports its weekly quota in `primary`; a weekly-only plan has no
     * 5-hour session window at all. The weekly must resolve as both "primary"
     * and "weekly", and no session window may be invented.
     */
    it("resolves the weekly window when no session window exists", () => {
      const weeklyOnly = provider(
        rateWindow("weekly", { usedPercent: 40, windowMinutes: 7 * 24 * 60 }),
      );
      expect(windowByKind(weeklyOnly, "primary")?.usedPercent).toBe(40);
      expect(windowByKind(weeklyOnly, "weekly")?.usedPercent).toBe(40);
      expect(windowByKind(weeklyOnly, "session")).toBeNull();
    });

    /**
     * The upstream observation (Windows PR #277): Codex's primary slot can be
     * an informational placeholder while the real weekly quota sits in
     * `secondary`. The primary lookup must fall through to the real window,
     * never treat the placeholder as the quota.
     */
    it("falls through an informational primary to the real weekly window", () => {
      const placeholder = provider(
        rateWindow("session", { isInformational: true, usedPercent: 0 }),
        { secondary: rateWindow("weekly", { usedPercent: 55 }) },
      );
      expect(windowByKind(placeholder, "primary")?.usedPercent).toBe(55);
      expect(windowByKind(placeholder, "weekly")?.usedPercent).toBe(55);
      expect(windowByKind(placeholder, "session")).toBeNull();
    });

    /**
     * When every window is informational (e.g. the whole response was a
     * placeholder), no lookup may return one of them as a real quota.
     */
    it("never promotes an informational-only provider to a quota", () => {
      const allInformational = provider(
        rateWindow("session", { isInformational: true }),
        {
          secondary: rateWindow("weekly", { isInformational: true }),
          tertiary: rateWindow("monthly", { isInformational: true }),
        },
      );
      expect(windowByKind(allInformational, "primary")).toBeNull();
      expect(windowByKind(allInformational, "weekly")).toBeNull();
      expect(windowByKind(allInformational, "session")).toBeNull();
    });

    /**
     * Codex reset credits arrive as an extra "reset-credits" window that is
     * informational by design (it carries inventory, not a percentage). It
     * must never be chosen as the provider's primary quota.
     */
    it("skips the informational reset-credits extra window", () => {
      const withResetCredits = provider(
        rateWindow("weekly", { usedPercent: 12 }),
        {
          extraRateWindows: [
            {
              id: "reset-credits",
              title: "Reset credits",
              window: rateWindow(null, {
                usedPercent: 0,
                isInformational: true,
                resetDescription: "3 reset credits available",
              }),
            },
          ],
        },
      );
      expect(windowByKind(withResetCredits, "primary")?.usedPercent).toBe(12);
      expect(windowByKind(withResetCredits, "weekly")?.usedPercent).toBe(12);
      // The reset-credit row is not a cycle and must not answer to any.
      expect(windowByKind(withResetCredits, "session")).toBeNull();
      expect(windowByKind(withResetCredits, "monthly")).toBeNull();
    });

    /**
     * A 0% window that is NOT flagged informational but whose description
     * names reset credits is the same placeholder from cached snapshots that
     * predate the informational flag. With no real quota window at all, the
     * primary lookup must return nothing rather than this carrier — a fake
     * "0% used" with "N reset credits available" would read as a real quota.
     */
    it("never chooses an unflagged reset-credit carrier as the primary", () => {
      const stale = provider(
        rateWindow("session", { isInformational: true, usedPercent: 0 }),
        {
          extraRateWindows: [
            {
              id: "reset-credits",
              title: "Reset credits",
              window: rateWindow(null, {
                usedPercent: 0,
                resetDescription: "2 reset credits available",
              }),
            },
          ],
        },
      );
      expect(windowByKind(stale, "primary")).toBeNull();
      expect(windowByKind(stale, "weekly")).toBeNull();
    });
  });
});
