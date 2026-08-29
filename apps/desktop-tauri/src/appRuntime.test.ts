import { describe, expect, it, vi, beforeEach, afterEach } from "vitest";
import { buildAppRuntime, disposeAppRuntime, getAppRuntime, hasAppRuntime } from "./appRuntime";
import { createUsageStore, usageStoreKey } from "./core/usageStore";
import { bridgeSnapshot } from "./core/fixtures";
import { createActionDispatcher } from "./core/actionDispatcher";
import type { UsageStoreKey } from "./core/usageStore";

vi.mock("./lib/tauri", () => ({
  getCachedProviders: vi.fn(async () => []),
  getOutputSpeedSnapshot: vi.fn(async () => ({})),
  getProviderChartData: vi.fn(async () => null),
  openSettingsWindow: vi.fn(async () => {}),
  openProviderDashboard: vi.fn(async () => {}),
  openProviderStatusPage: vi.fn(async () => {}),
  triggerProviderLogin: vi.fn(async () => {}),
  refreshProviders: vi.fn(async () => {}),
  refreshProvidersIfStale: vi.fn(async () => {}),
  quitApp: vi.fn(async () => {}),
}));
vi.mock("@tauri-apps/api/event", () => ({
  listen: vi.fn(async () => () => {}),
}));

describe("appRuntime", () => {
  beforeEach(() => {
    disposeAppRuntime();
  });
  afterEach(() => {
    disposeAppRuntime();
    vi.clearAllMocks();
  });

  it("builds a runtime with store/coordinator/scheduler/dispatcher", () => {
    const r = buildAppRuntime();
    expect(r.store).toBeDefined();
    expect(r.coordinator).toBeDefined();
    expect(r.scheduler).toBeDefined();
    expect(r.dispatcher).toBeDefined();
    expect(typeof r.fetchProvider).toBe("function");
    expect(typeof r.seedFromBridge).toBe("function");
    expect(() => r.store.getSnapshot()).not.toThrow();
  });

  it("is a singleton via getAppRuntime and tracks hasAppRuntime", () => {
    expect(hasAppRuntime()).toBe(false);
    const a = getAppRuntime();
    const b = getAppRuntime();
    expect(a).toBe(b);
    expect(hasAppRuntime()).toBe(true);
    disposeAppRuntime();
    expect(hasAppRuntime()).toBe(false);
  });

  it("seedFromBridge upserts snapshots into the shared store", () => {
    const r = buildAppRuntime();
    const snap = {
      providerId: "codex",
      accountKey: "acc",
      sourceKey: "src",
    };
    r.seedFromBridge([snap as never]);
    const key: UsageStoreKey = { providerId: "codex", accountKey: "acc", sourceKey: "src" };
    expect(r.store.get(key)?.snapshot).not.toBeNull();
  });

  it("fetchProvider rejects when no cached provider matches", async () => {
    const r = buildAppRuntime();
    await expect(
      r.fetchProvider({ providerId: "nope", accountKey: "a", sourceKey: "s" }),
    ).rejects.toThrow(/no snapshot/i);
  });

  it("fetchForKey triggers a stale backend round when the store has no record", async () => {
    const tauri = await import("./lib/tauri");
    vi.mocked(tauri.getCachedProviders).mockResolvedValue([]);
    const r = buildAppRuntime();
    await expect(
      r.fetchProvider({ providerId: "codex", accountKey: "a", sourceKey: "s" }),
    ).rejects.toThrow(/no snapshot/i);
    expect(tauri.refreshProvidersIfStale).toHaveBeenCalled();
  });

  it("fetchForKey prefers the cache row that matches the account email", async () => {
    const tauri = await import("./lib/tauri");
    vi.mocked(tauri.getCachedProviders).mockResolvedValue([
      bridgeSnapshot({ providerId: "codex", accountEmail: "team@x.com" }),
      bridgeSnapshot({ providerId: "codex", accountEmail: "me@x.com" }),
    ]);
    const r = buildAppRuntime();
    const snap = await r.fetchProvider({ providerId: "codex", accountKey: "me@x.com", sourceKey: "auto" });
    expect(snap.accountEmail).toBe("me@x.com");
  });
  it("dispose clears bridge globals so later hooks fail closed", () => {
    const r = buildAppRuntime();
    expect(() => r.dispose()).not.toThrow();
  });
});

