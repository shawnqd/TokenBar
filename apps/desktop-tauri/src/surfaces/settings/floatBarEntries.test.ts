import { describe, expect, it } from "vitest";
import { TASKBAR_PROVIDER_AUTO } from "../../types/bridge";
import {
  floatBarEntriesFromIds,
  floatBarIdsFromEntries,
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
});
