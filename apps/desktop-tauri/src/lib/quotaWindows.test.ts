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
});
