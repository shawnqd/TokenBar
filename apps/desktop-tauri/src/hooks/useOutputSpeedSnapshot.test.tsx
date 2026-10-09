import { act, renderHook } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { FIXTURE_SNAPSHOTS } from "../core/fixtures";
import { resetTrayCoreForTest } from "../surfaces/tray/trayCoreStore.testSupport";
import { trayCoreStore } from "../surfaces/tray/trayCoreStore";
import type { ProviderOutputSpeed } from "../types/bridge";
import { useOutputSpeedSnapshot } from "./useOutputSpeedSnapshot";

const grokKey = { providerId: "grok", accountKey: "default", sourceKey: "cli" };

const grokSpeed: ProviderOutputSpeed = {
  providerId: "grok",
  status: "recent",
  tokensPerSecond: 42.5,
  outputTokens: 850,
  updatedAtMs: 1_780_000_000_000,
  approximate: true,
  recentSamples: [],
};

describe("useOutputSpeedSnapshot refresh lifecycle", () => {
  beforeEach(() => {
    vi.useFakeTimers();
    resetTrayCoreForTest();
  });

  afterEach(() => {
    vi.restoreAllMocks();
    vi.useRealTimers();
  });

  it("retries after the real output-speed TTL when Grok data becomes available", async () => {
    // Keep this aligned with buildAppRuntime's production outputSpeed TTL.
    resetTrayCoreForTest({ outputSpeed: 15_000 });
    trayCoreStore.seed({
      ...FIXTURE_SNAPSHOTS.deepseekBalance,
      providerId: "grok",
      displayName: "Grok",
      accountKey: "default",
      sourceKey: "cli",
      capabilities: {
        ...FIXTURE_SNAPSHOTS.deepseekBalance.capabilities,
        supportsOutputSpeed: true,
      },
    });
    let attempts = 0;
    trayCoreStore.setEnrichmentRunner(async (kind, key) => {
      attempts += 1;
      trayCoreStore.commitRuntimeEnrichment(kind, key, {
        ok: attempts > 1,
        outputSpeed: attempts > 1 ? grokSpeed : null,
      });
    });

    const { result, unmount } = renderHook(() =>
      useOutputSpeedSnapshot(true, 1_000),
    );

    expect(result.current).toBeNull();
    expect(attempts).toBe(1);

    await act(async () => {
      await vi.advanceTimersByTimeAsync(14_999);
    });
    expect(attempts).toBe(1);

    await act(async () => {
      await vi.advanceTimersByTimeAsync(1);
    });
    expect(attempts).toBe(2);
    expect(result.current?.grok.tokensPerSecond).toBe(42.5);

    unmount();
    await act(async () => {
      await vi.advanceTimersByTimeAsync(1_000);
    });
    expect(attempts).toBe(2);
  });

  it("composes newly speed-capable providers into the snapshot", async () => {
    // Kimi, OpenCode and OpenCode Go joined OUTPUT_SPEED_PROVIDER_IDS with
    // their log parsers; the composition loop must carry their readings too.
    const kimiSpeed: ProviderOutputSpeed = {
      providerId: "kimi",
      status: "recent",
      tokensPerSecond: 31.0,
      outputTokens: 620,
      updatedAtMs: 1_780_000_000_000,
      approximate: true,
      recentSamples: [],
    };
    const opencodegoSpeed: ProviderOutputSpeed = {
      providerId: "opencodego",
      status: "recent",
      tokensPerSecond: 44.5,
      outputTokens: 910,
      updatedAtMs: 1_780_000_100_000,
      approximate: true,
      recentSamples: [],
    };
    for (const [providerId, displayName] of [
      ["kimi", "Kimi"],
      ["opencodego", "OpenCode Go"],
    ] as const) {
      trayCoreStore.seed({
        ...FIXTURE_SNAPSHOTS.deepseekBalance,
        providerId,
        displayName,
        accountKey: "default",
        sourceKey: "cli",
        capabilities: {
          ...FIXTURE_SNAPSHOTS.deepseekBalance.capabilities,
          supportsOutputSpeed: true,
        },
      });
    }
    trayCoreStore.setEnrichmentRunner(async (kind, key) => {
      trayCoreStore.commitRuntimeEnrichment(kind, key, {
        ok: true,
        outputSpeed:
          key.providerId === "kimi"
            ? kimiSpeed
            : key.providerId === "opencodego"
              ? opencodegoSpeed
              : null,
      });
    });

    const { result } = renderHook(() => useOutputSpeedSnapshot(true, 1_000));
    await act(async () => {
      await vi.advanceTimersByTimeAsync(1);
    });
    expect(result.current?.kimi.tokensPerSecond).toBe(31.0);
    expect(result.current?.opencodego.tokensPerSecond).toBe(44.5);
  });
});
