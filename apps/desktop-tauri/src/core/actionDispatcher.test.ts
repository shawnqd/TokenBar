import { describe, expect, it, vi } from "vitest";
import { createActionDispatcher, routeAction } from "./actionDispatcher";
import type { SurfaceAction } from "./actions";

describe("actionDispatcher", () => {
  it("dispatches each known action kind to its handler", async () => {
    const handlers = {
      refresh: vi.fn(async () => ({ status: "handled" as const, data: "refreshed" })),
      openSettings: vi.fn(async () => ({ status: "handled" as const })),
      quit: vi.fn(async () => ({ status: "handled" as const })),
      selectProvider: vi.fn(async () => ({ status: "handled" as const })),
      openProviderDetail: vi.fn(async () => ({ status: "handled" as const })),
      openExternalUsage: vi.fn(async () => ({ status: "handled" as const })),
      openExternalStatus: vi.fn(async () => ({ status: "handled" as const })),
      triggerLogin: vi.fn(async () => ({ status: "handled" as const })),
    };
    const dispatcher = createActionDispatcher(handlers);

    const cases: SurfaceAction[] = [
      { type: "refresh" },
      { type: "openSettings", target: { kind: "settings", tab: "general" } },
      { type: "quit", target: { kind: "app" } },
      { type: "selectProvider", target: { kind: "providerOptional", providerId: "codex" } },
      { type: "openProviderDetail", target: { kind: "provider", providerId: "codex" } },
      { type: "openExternalUsage", target: { kind: "provider", providerId: "codex" } },
      { type: "openExternalStatus", target: { kind: "provider", providerId: "codex" } },
      { type: "triggerLogin", target: { kind: "provider", providerId: "codex" } },
    ];

    for (const action of cases) {
      const res = await dispatcher.dispatch(action);
      expect(res.status).toBe("handled");
    }
    expect(handlers.refresh).toHaveBeenCalledTimes(1);
    expect(handlers.openSettings).toHaveBeenCalledTimes(1);
    expect(handlers.quit).toHaveBeenCalledTimes(1);
  });

  it("unknown action returns unknown without throwing", async () => {
    const dispatcher = createActionDispatcher({
      refresh: async () => ({ status: "handled" as const }),
    });
    const unknown = { type: "openSettings", target: { kind: "settings", tab: "general" } } as SurfaceAction;
    const res = await dispatcher.dispatch(unknown);
    expect(res.status).toBe("unknown");

    const totallyUnknown = { type: "nonExistent" as unknown as SurfaceAction["type"] } as SurfaceAction;
    const res2 = await dispatcher.dispatch(totallyUnknown);
    expect(res2.status).toBe("unknown");
  });

  it("handler error is isolated and returns error status, not throw", async () => {
    const dispatcher = createActionDispatcher({
      refresh: async () => { throw new Error("handler boom"); },
      quit: async () => ({ status: "handled" as const }),
    });
    const res = await dispatcher.dispatch({ type: "refresh" } as SurfaceAction);
    expect(res.status).toBe("error");
    expect(res.error).toMatch(/handler boom/);

    const res2 = await dispatcher.dispatch({ type: "quit", target: { kind: "app" } } as SurfaceAction);
    expect(res2.status).toBe("handled");
  });

  it("routeAction pure function maps action to target kind", () => {
    expect(routeAction({ type: "refresh" } as SurfaceAction)).toBe("summary");
    expect(routeAction({ type: "openSettings", target: { kind: "settings", tab: "general" } } as SurfaceAction)).toBe("settings");
    expect(routeAction({ type: "quit", target: { kind: "app" } } as SurfaceAction)).toBe("app");
    expect(routeAction({ type: "selectProvider", target: { kind: "providerOptional", providerId: null } } as SurfaceAction)).toBe("providerOptional");
    expect(routeAction({ type: "openProviderDetail", target: { kind: "provider", providerId: "claude" } } as SurfaceAction)).toBe("provider");
    expect(routeAction({ type: "triggerLogin", target: { kind: "provider", providerId: "codex" } } as SurfaceAction)).toBe("provider");
  });

  it("dispatch error does not crash subsequent dispatches", async () => {
    let count = 0;
    const dispatcher = createActionDispatcher({
      refresh: async () => {
        count++;
        if (count === 1) throw new Error("first fail");
        return { status: "handled" as const };
      },
    });
    const r1 = await dispatcher.dispatch({ type: "refresh" } as SurfaceAction);
    const r2 = await dispatcher.dispatch({ type: "refresh" } as SurfaceAction);
    expect(r1.status).toBe("error");
    expect(r2.status).toBe("handled");
    expect(count).toBe(2);
  });
});
