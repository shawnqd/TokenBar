import { describe, expect, it } from "vitest";

import {
  FORECAST_UNAVAILABLE,
  formatResetDisplay,
  primaryQuotaState,
  quotaDisplayContext,
  quotaForecastDisplay,
  quotaLevel,
  quotaPercentContext,
  quotaPercentDisplay,
  quotaSemanticsLabelKey,
  resolveProviderStatus,
  STALE_AFTER_MS,
} from "./quotaDisplay";
import type { LocaleKey } from "../i18n/keys";
import type {
  PaceSnapshot,
  ProviderUsageSnapshot,
  RateWindowSnapshot,
  SettingsSnapshot,
} from "../types/bridge";

/**
 * Echo the key back so assertions read as the key that was chosen, except for
 * the two countdown templates, where the `{}` substitution is what is under
 * test. The strings mirror `en-US.ftl`.
 */
const TEMPLATES: Partial<Record<LocaleKey, string>> = {
  ResetsInDaysHours: "Resets in {}d {}h",
  ResetsInHoursMinutes: "Resets in {}h {}m",
  TodayAt: "Today at {}",
  TomorrowAt: "Tomorrow at {}",
};
const t = (key: LocaleKey) => TEMPLATES[key] ?? key;

function rate(overrides: Partial<RateWindowSnapshot> = {}): RateWindowSnapshot {
  return {
    usedPercent: 40,
    remainingPercent: 60,
kind: "session",
        windowMinutes: 300,
    resetsAt: "2026-07-30T12:00:00.000Z",
    resetDescription: null,
    isExhausted: false,
    reservePercent: null,
    reserveDescription: null,
    ...overrides,
  };
}

const ctx = {
  showAsUsed: true,
  resetTimeRelative: true,
  highUsageThreshold: 70,
  criticalUsageThreshold: 90,
};

describe("quotaDisplayContext", () => {
  const settings = {
    floatBarShowAsUsed: false,
    floatBarResetTimeRelative: true,
    dashboardShowAsUsed: true,
    dashboardResetTimeRelative: false,
    dashboardProviderIds: [],
    taskbarShowAsUsed: false,
    taskbarResetTimeRelative: true,
    taskbarTooltipEntries: [],
    highUsageThreshold: 75,
    criticalUsageThreshold: 95,
  } as unknown as SettingsSnapshot;

  it("reads each component's own keys and never another component's", () => {
    expect(quotaDisplayContext(settings, "floatBar")).toMatchObject({
      showAsUsed: false,
      resetTimeRelative: true,
    });
    expect(quotaDisplayContext(settings, "dashboard")).toMatchObject({
      showAsUsed: true,
      resetTimeRelative: false,
    });
    expect(quotaPercentContext(settings, "taskbar")).toMatchObject({
      showAsUsed: false,
    });
  });

  /**
   * The taskbar renders no reset text, so it has no reset-time setting to read.
   * `quotaPercentContext` is the only shape it can be asked for, and it must not
   * carry a reset field that would look configurable.
   */
  it("gives the taskbar a percentage context with no reset-time mode", () => {
    const ctx = quotaPercentContext(settings, "taskbar");
    expect(ctx).not.toHaveProperty("resetTimeRelative");
  });

  it("carries the user's configured thresholds", () => {
    expect(quotaDisplayContext(settings, "dashboard")).toMatchObject({
      highUsageThreshold: 75,
      criticalUsageThreshold: 95,
    });
    expect(quotaPercentContext(settings, "taskbar")).toMatchObject({
      highUsageThreshold: 75,
      criticalUsageThreshold: 95,
    });
  });

  it("falls back to defaults for a snapshot missing the split fields", () => {
    const legacy = {} as unknown as SettingsSnapshot;
    expect(quotaDisplayContext(legacy, "dashboard")).toEqual({
      showAsUsed: true,
      resetTimeRelative: true,
      highUsageThreshold: 70,
      criticalUsageThreshold: 90,
    });
  });
});

describe("primaryQuotaState", () => {
  function provider(
    overrides: Partial<ProviderUsageSnapshot> = {},
  ): ProviderUsageSnapshot {
    return {
      providerId: "codex",
      displayName: "Codex",
      primary: rate(),
      secondary: null,
      modelSpecific: null,
      tertiary: null,
      extraRateWindows: [],
      cost: null,
      planName: null,
      accountEmail: null,
      sourceLabel: "auto",
      updatedAt: "2026-07-30T10:00:00.000Z",
      error: null,
      pace: null,
      accountOrganization: null,
      trayStatusLabel: null,
      ...overrides,
    };
  }

  it("reports a real percentage quota as such", () => {
    expect(primaryQuotaState(provider())).toBe("quota");
  });

  it("reports a failed fetch as an error", () => {
    expect(primaryQuotaState(provider({ error: "boom" }))).toBe("error");
  });

  /**
   * sub2api-style providers emit rows like "Subscription active" or "No quota
   * data" as informational windows. Drawing those as 0% would invent a
   * measurement.
   */
  it("reports an informational primary window as non-quota", () => {
    const informational = provider({
      primary: rate({
        usedPercent: 0,
        isInformational: true,
        resetDescription: "Subscription active",
      }),
    });
    expect(primaryQuotaState(informational)).toBe("informational");
  });

  /**
   * Balance providers smuggle a prepaid amount through a synthetic 0% window;
   * that window is not informational-flagged, so the balance parser is what
   * identifies it.
   */
  it("reports a balance carrier window as non-quota", () => {
    const deepseek = provider({
      providerId: "deepseek",
      displayName: "DeepSeek",
      primary: rate({
        usedPercent: 0,
        resetDescription: "¥38.81 (Paid: ¥38.81 / Granted: ¥0.00)",
      }),
    });
    expect(primaryQuotaState(deepseek)).toBe("informational");
  });

  it("keeps a balance provider's real quota window when it has one", () => {
    const mimoWithPlan = provider({
      providerId: "mimo",
      displayName: "MiMo",
      primary: rate({ usedPercent: 40 }),
    });
    expect(primaryQuotaState(mimoWithPlan)).toBe("quota");
  });
});

describe("quotaPercentDisplay", () => {
  it("shows the used figure and fills to it", () => {
    const display = quotaPercentDisplay(rate({ usedPercent: 40 }), ctx);
    expect(display).toMatchObject({
      semantics: "used",
      percent: 40,
      rounded: 40,
      fillPercent: 40,
      level: "normal",
    });
  });

  it("shows the remaining figure and fills to it", () => {
    const display = quotaPercentDisplay(rate({ usedPercent: 40 }), {
      ...ctx,
      showAsUsed: false,
    });
    expect(display).toMatchObject({
      semantics: "remaining",
      percent: 60,
      fillPercent: 60,
    });
  });

  it("grades the same risk in both semantics", () => {
    const heavy = rate({ usedPercent: 92 });
    expect(quotaPercentDisplay(heavy, ctx).level).toBe("critical");
    expect(quotaPercentDisplay(heavy, { ...ctx, showAsUsed: false }).level).toBe(
      "critical",
    );
  });

  it("derives remaining from used rather than trusting an inconsistent pair", () => {
    const inconsistent = rate({ usedPercent: 40, remainingPercent: 5 });
    expect(
      quotaPercentDisplay(inconsistent, { ...ctx, showAsUsed: false }).percent,
    ).toBe(60);
  });

  it("clamps a non-finite or out-of-range used percentage", () => {
    expect(quotaPercentDisplay(rate({ usedPercent: Number.NaN }), ctx).percent).toBe(0);
    expect(quotaPercentDisplay(rate({ usedPercent: 140 }), ctx).percent).toBe(100);
    expect(quotaPercentDisplay(rate({ usedPercent: -20 }), ctx).percent).toBe(0);
  });

  it("marks informational rows so they are never drawn as a quota bar", () => {
    const info = rate({ isInformational: true, resetDescription: "¥38.88" });
    expect(quotaPercentDisplay(info, ctx).isInformational).toBe(true);
  });
});

describe("quotaLevel", () => {
  it("uses the configured thresholds instead of hardcoded cutoffs", () => {
    const thresholds = { highUsageThreshold: 50, criticalUsageThreshold: 80 };
    expect(quotaLevel(49, thresholds)).toBe("normal");
    expect(quotaLevel(50, thresholds)).toBe("high");
    expect(quotaLevel(80, thresholds)).toBe("critical");
  });

  it("reports exhausted at or above 100 percent and when flagged", () => {
    expect(quotaLevel(100, ctx)).toBe("exhausted");
    expect(quotaLevel(10, ctx, true)).toBe("exhausted");
  });
});

describe("quotaSemanticsLabelKey", () => {
  it("maps each semantics to one shared suffix key", () => {
    expect(quotaSemanticsLabelKey("used")).toBe("PanelUsedSuffix");
    expect(quotaSemanticsLabelKey("remaining")).toBe("PanelLeftSuffix");
  });
});

describe("resolveProviderStatus", () => {
  const updatedAt = "2026-07-30T10:00:00.000Z";
  const nowMs = Date.parse(updatedAt);

  it("reports an error when the fetch failed", () => {
    expect(
      resolveProviderStatus({ error: "boom", updatedAt }, nowMs),
    ).toBe("error");
  });

  it("reports ready for a fresh snapshot", () => {
    expect(resolveProviderStatus({ error: null, updatedAt }, nowMs)).toBe("ready");
  });

  it("reports stale once the snapshot ages past the cutoff", () => {
    expect(
      resolveProviderStatus({ error: null, updatedAt }, nowMs + STALE_AFTER_MS + 1),
    ).toBe("stale");
  });

  it("reports loading when there is no usable timestamp", () => {
    expect(resolveProviderStatus({ error: null, updatedAt: "" }, nowMs)).toBe("loading");
    expect(
      resolveProviderStatus({ error: null, updatedAt: "not-a-date" }, nowMs),
    ).toBe("loading");
  });
});

describe("formatResetDisplay", () => {
  const nowMs = Date.parse("2026-07-30T10:00:00.000Z");
  const base = { relative: true, t, nowMs, timeZone: "UTC", locale: "en-US" };

  it("renders a countdown in relative mode", () => {
    const result = formatResetDisplay({
      ...base,
      resetsAt: "2026-07-30T13:42:00.000Z",
      resetDescription: null,
    });
    expect(result.kind).toBe("relative");
    expect(result.text).toBe("Resets in 3h 42m");
    expect(result.ticking).toBe(true);
  });

  it("renders days and hours for a multi-day countdown", () => {
    const result = formatResetDisplay({
      ...base,
      resetsAt: "2026-08-02T14:00:00.000Z",
      resetDescription: null,
    });
    expect(result.kind).toBe("relative");
    expect(result.text).toBe("Resets in 3d 4h");
  });

  // Every absolute reset says which day and carries a label. A bare "3:30 PM"
  // in the slot a countdown used to occupy names neither.
  it("names today and labels a same-day absolute reset", () => {
    const result = formatResetDisplay({
      ...base,
      relative: false,
      resetsAt: "2026-07-30T15:30:00.000Z",
      resetDescription: null,
    });
    expect(result.kind).toBe("absolute");
    expect(result.text).toBe("Today at 3:30 PM");
  });

  it("names tomorrow rather than dating a reset a few hours away", () => {
    const result = formatResetDisplay({
      ...base,
      relative: false,
      resetsAt: "2026-07-31T07:30:00.000Z",
      resetDescription: null,
    });
    expect(result.kind).toBe("absolute");
    expect(result.text).toBe("Tomorrow at 7:30 AM");
  });

  it("gives a calendar date once the reset is further out than tomorrow", () => {
    const result = formatResetDisplay({
      ...base,
      relative: false,
      resetsAt: "2026-08-02T07:30:00.000Z",
      resetDescription: null,
    });
    expect(result.kind).toBe("absolute");
    expect(result.text).toContain("Aug 2");
    expect(result.text).toContain("7:30 AM");
  });

  // "Tomorrow" is a calendar-day step, not a 24-hour one. 23:30 today and
  // 00:30 tomorrow are an hour apart but two different days.
  it("counts calendar days, not elapsed hours, when choosing the wording", () => {
    const lateEvening = {
      ...base,
      relative: false,
      nowMs: Date.parse("2026-07-30T23:30:00.000Z"),
      resetDescription: null,
    };
    expect(
      formatResetDisplay({ ...lateEvening, resetsAt: "2026-07-31T00:30:00.000Z" }).text,
    ).toBe("Tomorrow at 12:30 AM");
    expect(
      formatResetDisplay({ ...lateEvening, resetsAt: "2026-07-30T23:45:00.000Z" }).text,
    ).toBe("Today at 11:45 PM");
  });

  // The day words are resolved in the display timezone, not the host's.
  it("resolves the day in the configured timezone", () => {
    const args = {
      ...base,
      relative: false,
      resetsAt: "2026-07-31T02:00:00.000Z",
      resetDescription: null,
    };
    // 02:00Z on the 31st is still the 30th in New York → same day there.
    expect(formatResetDisplay({ ...args, timeZone: "America/New_York" }).text).toBe(
      "Today at 10:00 PM",
    );
    expect(formatResetDisplay({ ...args, timeZone: "UTC" }).text).toBe(
      "Tomorrow at 2:00 AM",
    );
  });

  // A translation that loses its placeholder must not swallow the time.
  it("appends the time when a template has lost its placeholder", () => {
    const result = formatResetDisplay({
      ...base,
      relative: false,
      resetsAt: "2026-07-30T15:30:00.000Z",
      resetDescription: null,
      t: ((key: LocaleKey) =>
        key === "TodayAt" ? "Today" : TEMPLATES[key] ?? key) as typeof t,
    });
    expect(result.text).toBe("Today 3:30 PM");
  });

  /**
   * No "Reset:" prefix. It was tried and dropped: this string only renders in a
   * slot that already means "reset" — beside a quota title, or in the floating
   * bar's chip behind a reset icon — where the word was pure repetition in the
   * one place with no width to spare.
   */
  it("labels nothing, because its slot already means reset", () => {
    const result = formatResetDisplay({
      ...base,
      relative: false,
      resetsAt: "2026-07-30T15:30:00.000Z",
      resetDescription: null,
    });
    expect(result.text).not.toMatch(/reset/i);
  });

  it("reads an elapsed reset as expired rather than a negative countdown", () => {
    const result = formatResetDisplay({
      ...base,
      resetsAt: "2026-07-30T09:00:00.000Z",
      resetDescription: null,
    });
    expect(result.kind).toBe("expired");
    expect(result.text).toBe("QuotaResetExpiredWaiting");
    // Still ticking so the row recovers by itself once the window renews.
    expect(result.ticking).toBe(true);
  });

  it("passes the provider's own wording through when there is no timestamp", () => {
    const result = formatResetDisplay({
      ...base,
      resetsAt: null,
      resetDescription: "Resets at the start of your billing cycle",
    });
    expect(result.kind).toBe("providerDescription");
    expect(result.text).toBe("Resets at the start of your billing cycle");
    expect(result.ticking).toBe(false);
  });

  it("never invents a reset time when nothing is known", () => {
    const result = formatResetDisplay({
      ...base,
      resetsAt: null,
      resetDescription: null,
    });
    expect(result.kind).toBe("unknown");
    expect(result.text).toBe("QuotaResetUnknown");
  });

  it("treats an unparseable timestamp as having no timestamp", () => {
    const result = formatResetDisplay({
      ...base,
      resetsAt: "tomorrow-ish",
      resetDescription: "  provider text  ",
    });
    expect(result.kind).toBe("providerDescription");
    expect(result.text).toBe("provider text");
  });
});

describe("quotaForecastDisplay", () => {
  const pace: PaceSnapshot = {
    stage: "ahead",
    deltaPercent: 12.5,
    willLastToReset: false,
    etaSeconds: 4200,
    expectedUsedPercent: 55,
    actualUsedPercent: 67.5,
    speedMultiplierToReset: null,
  };

  it("exposes the forecast when the window can support one", () => {
    expect(quotaForecastDisplay(pace, rate())).toEqual({
      available: true,
      projectedUsedPercent: 55,
      deltaPercent: 12.5,
      lastsToReset: false,
      etaSeconds: 4200,
      // The stage comes from shared Rust and is carried through rather than
      // re-derived in TS, so the +/-2/6/12 thresholds live in exactly one place.
      stage: "ahead",
      speedMultiplierToReset: null,
    });
  });

  it("reports unavailable rather than zero when there is no pace data", () => {
    expect(quotaForecastDisplay(null, rate())).toEqual(FORECAST_UNAVAILABLE);
  });

  it("reports unavailable for informational rows", () => {
    expect(quotaForecastDisplay(pace, rate({ isInformational: true }))).toEqual(
      FORECAST_UNAVAILABLE,
    );
  });

  it("reports unavailable without a window duration or reset boundary", () => {
    expect(quotaForecastDisplay(pace, rate({ windowMinutes: null }))).toEqual(
      FORECAST_UNAVAILABLE,
    );
    expect(quotaForecastDisplay(pace, rate({ resetsAt: null }))).toEqual(
      FORECAST_UNAVAILABLE,
    );
  });

  it("drops a non-finite ETA instead of forwarding it", () => {
    const result = quotaForecastDisplay(
      { ...pace, etaSeconds: Number.NaN },
      rate(),
    );
    expect(result.available).toBe(true);
    expect(result.etaSeconds).toBeNull();
  });
});
