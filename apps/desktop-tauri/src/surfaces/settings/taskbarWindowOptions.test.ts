import { describe, expect, it } from "vitest";
import {
  CONFIGURABLE_TASKBAR_WINDOWS,
  TASKBAR_WINDOW_LABELS_ZH,
  taskbarWindowLabelFor,
  taskbarWindowOptionsFor,
} from "./taskbarWindowOptions";

describe("taskbar window vocabulary", () => {
  it("exposes real product windows, not internal primary/session words", () => {
    expect(CONFIGURABLE_TASKBAR_WINDOWS).toEqual([
      "session",
      "weekly",
      "daily",
      "monthly",
      "balance",
      "speed",
    ]);
    expect(TASKBAR_WINDOW_LABELS_ZH.session).toBe("5h");
    expect(Object.values(TASKBAR_WINDOW_LABELS_ZH)).not.toContain("会话");
    expect(Object.values(TASKBAR_WINDOW_LABELS_ZH)).not.toContain("主窗口");
  });

  it("filters each provider from the same live availability map", () => {
    const availability = {
      zai: ["balance" as const],
      codex: ["session" as const, "weekly" as const],
    };

    expect(
      taskbarWindowOptionsFor({ providerId: "zai", window: "balance" }, availability),
    ).toEqual(["balance"]);
    expect(
      taskbarWindowOptionsFor({ providerId: "auto", window: "session" }, availability),
    ).toEqual(["session", "weekly", "balance"]);
  });

  it("keeps a stale named selection editable but never resurrects primary", () => {
    const availability = { grok: ["monthly" as const] };
    expect(
      taskbarWindowOptionsFor({ providerId: "grok", window: "weekly" }, availability),
    ).toEqual(["monthly", "weekly"]);
    expect(
      taskbarWindowOptionsFor({ providerId: "grok", window: "primary" }, availability),
    ).toEqual(["monthly"]);
  });

  it("expands compact native Chinese tags in the settings control", () => {
    const compactChinese: Record<string, string> = {
      TaskbarWindowSession: "5h",
      TaskbarWindowWeekly: "周",
      TaskbarWindowDaily: "日",
      TaskbarWindowMonthly: "月",
      TaskbarWindowBalance: "余",
      TaskbarWindowSpeed: "速",
    };
    const t = (key: string) => compactChinese[key] ?? key;

    expect(taskbarWindowLabelFor("balance", t, "chinese")).toBe("余额");
    expect(taskbarWindowLabelFor("speed", t, "chinese")).toBe("速度");
    expect(taskbarWindowLabelFor("weekly", t, "chinese")).toBe("周");
  });
});
