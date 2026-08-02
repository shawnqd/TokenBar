import { act, render, screen } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { LocaleProvider } from "../i18n/LocaleProvider";
import { buildBundle } from "../test/localeHarness";
import { useFormattedResetTime } from "./useFormattedResetTime";
import * as tauri from "../lib/tauri";

vi.mock("../lib/tauri", () => ({
  getLocaleStrings: vi.fn(),
  setUiLanguage: vi.fn(),
}));

vi.mock("@tauri-apps/api/event", () => ({
  listen: vi.fn().mockResolvedValue(() => {}),
}));

function Probe({
  resetsAt,
  fallback,
  relative,
}: {
  resetsAt: string | null;
  fallback: string | null;
  relative: boolean;
}) {
  const text = useFormattedResetTime(resetsAt, fallback, relative);
  return <span data-testid="reset">{text ?? "null"}</span>;
}

async function mountWithLocale(ui: React.ReactNode) {
  (tauri.getLocaleStrings as ReturnType<typeof vi.fn>).mockResolvedValue(
    buildBundle({
      MetricResetsIn: "Resets in",
      ResetsInHoursMinutes: "Resets in {}h {}m",
      ResetsInDaysHours: "Resets in {}d {}h",
      TodayAt: "Today at {}",
      TrayResetsDueNow: "Resetting",
      QuotaResetExpiredWaiting: "Expired — waiting for refresh",
      QuotaResetUnknown: "Reset time unknown",
    }),
  );
  const rendered = render(<LocaleProvider>{ui}</LocaleProvider>);
  await act(async () => {});
  return rendered;
}

describe("useFormattedResetTime", () => {
  beforeEach(() => {
    vi.useFakeTimers();
    vi.setSystemTime(new Date("2024-06-01T00:00:00Z"));
  });

  afterEach(() => {
    vi.useRealTimers();
  });

  it("returns a complete localized countdown in relative mode", async () => {
    const target = new Date("2024-06-01T03:42:00Z").toISOString();
    await mountWithLocale(
      <Probe resetsAt={target} fallback="later" relative={true} />,
    );
    expect(screen.getByTestId("reset")).toHaveTextContent("Resets in 3h 42m");
  });

  it("leaves fallback text unlabelled in relative mode", async () => {
    await mountWithLocale(
      <Probe resetsAt={null} fallback="3h" relative={true} />,
    );
    expect(screen.getByTestId("reset")).toHaveTextContent("3h");
  });

  it("names the day and the clock time in absolute mode", async () => {
    const target = new Date("2024-06-01T03:42:00Z").toISOString();
    await mountWithLocale(
      <Probe resetsAt={target} fallback="later" relative={false} />,
    );
    const node = screen.getByTestId("reset");
    // The day is what makes the time readable; the slot already means "reset",
    // so nothing prefixes it.
    expect(node).not.toHaveTextContent("Resets in");
    expect(node).toHaveTextContent("Today at");
  });

  it("reads an elapsed window as expired instead of counting down past zero", async () => {
    const target = new Date("2024-05-31T23:00:00Z").toISOString();
    await mountWithLocale(
      <Probe resetsAt={target} fallback="later" relative={true} />,
    );
    const node = screen.getByTestId("reset");
    expect(node).toHaveTextContent("Expired — waiting for refresh");
    expect(node.textContent).not.toContain("-");
  });

  it("says the reset time is unknown when there is no timestamp or description", async () => {
    await mountWithLocale(<Probe resetsAt={null} fallback={null} relative={true} />);
    expect(screen.getByTestId("reset")).toHaveTextContent("Reset time unknown");
  });
});
