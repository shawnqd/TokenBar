import { describe, expect, it } from "vitest";
import { TASKBAR_PROVIDER_AUTO } from "../../types/bridge";
import {
  expandFloatBarEntries,
  floatBarEntriesFromIds,
  floatBarIdsFromEntries,
  resolveFloatBarEntries,
} from "./floatBarEntries";

describe("floatBarEntries adapter", () => {
  it("treats an empty persisted list as follow-enabled session + weekly", () => {
    expect(floatBarEntriesFromIds([])).toEqual([
      { providerId: TASKBAR_PROVIDER_AUTO, window: "session" },
      { providerId: TASKBAR_PROVIDER_AUTO, window: "weekly" },
    ]);
    expect(
      floatBarIdsFromEntries([
        { providerId: TASKBAR_PROVIDER_AUTO, window: "session" },
        { providerId: "deepseek", window: "balance" },
      ]),
    ).toEqual(["deepseek"]);
  });

  it("prefers floatBarEntries over legacy ids when present", () => {
    const settings = {
      floatBarEntries: [{ providerId: "claude", window: "weekly" as const }],
      floatBarProviderIds: ["codex"],
    };
    expect(resolveFloatBarEntries(settings)).toEqual([
      { providerId: "claude", window: "weekly" },
    ]);
  });

  it("falls back to legacy ids when entries missing", () => {
    const settings = {
      floatBarProviderIds: ["codex", "claude"],
    } as any;
    expect(resolveFloatBarEntries(settings)).toEqual([
      { providerId: "codex", window: "primary" },
      { providerId: "claude", window: "primary" },
    ]);
  });

  it("treats explicit empty entries as follow-enabled placeholder", () => {
    const settings = {
      floatBarEntries: [],
      floatBarProviderIds: ["codex"],
    } as any;
    expect(resolveFloatBarEntries(settings)).toEqual([
      { providerId: TASKBAR_PROVIDER_AUTO, window: "session" },
      { providerId: TASKBAR_PROVIDER_AUTO, window: "weekly" },
    ]);
  });

  it("does not break migration when entries damaged → still returns legacy", () => {
    // if floatBarEntries is malformed non-array, fallback to ids
    const settings = {
      floatBarEntries: null as any,
      floatBarProviderIds: ["codex"],
    };
    expect(resolveFloatBarEntries(settings)).toEqual([
      { providerId: "codex", window: "primary" },
    ]);
  });

  it("expands auto entries positionally to enabled providers", () => {
    const entries = [
      { providerId: TASKBAR_PROVIDER_AUTO, window: "session" as const },
      { providerId: TASKBAR_PROVIDER_AUTO, window: "weekly" as const },
      { providerId: "cursor", window: "primary" as const },
    ];
    expect(
      expandFloatBarEntries(entries, ["claude", "codex", "cursor"]),
    ).toEqual([
      { providerId: "claude", window: "session" },
      { providerId: "codex", window: "weekly" },
      { providerId: "cursor", window: "primary" },
    ]);
  });

  it("drops specific entries whose provider is not enabled", () => {
    const entries = [
      { providerId: "claude", window: "primary" as const },
      { providerId: "codex", window: "primary" as const },
    ];
    expect(expandFloatBarEntries(entries, ["claude"])).toEqual([
      { providerId: "claude", window: "primary" },
    ]);
  });
});
