import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import {
  SURFACE_DESCRIPTORS,
  getSurface,
  listSurfaces,
  canActivate,
  runLifecycle,
  kindFromWindowLabel,
  activateSurface,
  deactivateSurface,
  activeSurfaces,
  isSurfaceActive,
  surfaceHostError,
  resetSurfaceHostForTest,
  subscribeSurface,
} from "./surfaceRegistry";
import type { SurfaceKind } from "./surfaceRegistry";

describe("SurfaceRegistry", () => {
  it("has static descriptors for all four surfaces", () => {
    const kinds: SurfaceKind[] = ["trayPanel", "floatBar", "taskbarStatus", "settings"];
    for (const kind of kinds) {
      const d = getSurface(kind);
      expect(d).toBeDefined();
      expect(d?.kind).toBe(kind);
      expect(d?.pluginId).toMatch(/^builtin:/);
      expect(d?.version).toBeDefined();
      expect(d?.lifecycle).toBeDefined();
      expect(typeof d?.lifecycle.create).toBe("function");
      expect(typeof d?.lifecycle.prepare).toBe("function");
      expect(typeof d?.lifecycle.reveal).toBe("function");
      expect(typeof d?.lifecycle.hide).toBe("function");
      expect(typeof d?.lifecycle.dispose).toBe("function");
    }
    expect(Object.keys(SURFACE_DESCRIPTORS)).toHaveLength(4);
    expect(listSurfaces()).toHaveLength(4);
  });

  it("descriptor completeness matches spec: trayPanel always, floatBar setting-gated floatBar.enabled, taskbar native-child, settings on-demand", () => {
    const tray = getSurface("trayPanel")!;
    expect(tray.activationPolicy).toBe("always");
    expect(tray.hostType).toBe("tauri-webview");
    expect(tray.settingsNamespace).toBeNull();

    const float = getSurface("floatBar")!;
    expect(float.activationPolicy).toBe("setting-gated");
    expect(float.hostType).toBe("tauri-webview");
    expect(float.settingsNamespace).toBe("floatBar");

    const taskbar = getSurface("taskbarStatus")!;
    expect(taskbar.activationPolicy).toBe("setting-gated");
    expect(taskbar.hostType).toBe("native-child");
    expect(taskbar.settingsNamespace).toBe("taskbar");

    const settings = getSurface("settings")!;
    expect(settings.activationPolicy).toBe("on-demand");
    expect(settings.hostType).toBe("tauri-webview");
  });

  it("canActivate pure function respects settings", () => {
    expect(canActivate("trayPanel", {})).toBe(true);
    expect(canActivate("trayPanel", { floatBarEnabled: false })).toBe(true);

    expect(canActivate("floatBar", { floatBarEnabled: false })).toBe(false);
    expect(canActivate("floatBar", { floatBarEnabled: true })).toBe(true);
    expect(canActivate("floatBar", { "floatBar.enabled": true } as unknown as Record<string, unknown>)).toBe(true);
    expect(canActivate("floatBar", { floatBar: { enabled: true } } as unknown as Record<string, unknown>)).toBe(true);
    expect(canActivate("floatBar", { floatBar: { enabled: false } } as unknown as Record<string, unknown>)).toBe(false);

    expect(canActivate("taskbarStatus", { taskbarWidgetEnabled: false })).toBe(false);
    expect(canActivate("taskbarStatus", { taskbarWidgetEnabled: true })).toBe(true);
    expect(canActivate("taskbarStatus", { "taskbar.enabled": true } as unknown as Record<string, unknown>)).toBe(true);

    expect(canActivate("settings", {})).toBe(true);
  });

  it("listSurfaces returns all descriptors and is stable", () => {
    const list = listSurfaces();
    const kinds = list.map((d) => d.kind).sort();
    expect(kinds).toEqual(["floatBar", "settings", "taskbarStatus", "trayPanel"]);
  });

  it("lifecycle phase calls are invocable and isolated per surface", async () => {
    const tray = getSurface("trayPanel")!;
    const calls: string[] = [];
    tray.lifecycle.create = vi.fn(() => { calls.push("create"); });
    tray.lifecycle.prepare = vi.fn(() => { calls.push("prepare"); });
    tray.lifecycle.reveal = vi.fn(() => { calls.push("reveal"); });
    tray.lifecycle.hide = vi.fn(() => { calls.push("hide"); });
    tray.lifecycle.dispose = vi.fn(() => { calls.push("dispose"); });

    await runLifecycle("trayPanel", "create");
    await runLifecycle("trayPanel", "prepare");
    await runLifecycle("trayPanel", "reveal");
    await runLifecycle("trayPanel", "hide");
    await runLifecycle("trayPanel", "dispose");

    expect(calls).toEqual(["create", "prepare", "reveal", "hide", "dispose"]);

    // other surface not affected
    const float = getSurface("floatBar")!;
    const floatCalls: string[] = [];
    float.lifecycle.reveal = vi.fn(() => { floatCalls.push("float-reveal"); });
    await runLifecycle("floatBar", "reveal");
    expect(floatCalls).toEqual(["float-reveal"]);
    expect(calls).toHaveLength(5);
  });

  it("getSurface returns undefined for unknown kind", () => {
    expect(getSurface("unknown" as SurfaceKind)).toBeUndefined();
  });
});

describe("SurfaceRegistry host", () => {
  beforeEach(() => {
    resetSurfaceHostForTest();
  });
  afterEach(() => {
    resetSurfaceHostForTest();
  });

  it("kindFromWindowLabel maps host input labels", () => {
    expect(kindFromWindowLabel("settings")).toBe("settings");
    expect(kindFromWindowLabel("floatbar")).toBe("floatBar");
    expect(kindFromWindowLabel("flyout")).toBe("trayPanel");
    expect(kindFromWindowLabel("main")).toBeNull();
  });

  it("activate/deactivate populate activeSurfaces and isolate failures", async () => {
    const tray = getSurface("trayPanel")!;
    tray.lifecycle.reveal = vi.fn();
    await activateSurface("trayPanel");
    expect(activeSurfaces().map((s) => s.kind)).toContain("trayPanel");
    expect(isSurfaceActive("trayPanel")).toBe(true);

    const settings = getSurface("settings")!;
    settings.lifecycle.create = vi.fn(() => {
      throw new Error("settings boom");
    });
    await activateSurface("settings");
    expect(isSurfaceActive("settings")).toBe(true);
    expect(surfaceHostError("settings")).toMatch(/settings boom/);
    expect(isSurfaceActive("trayPanel")).toBe(true);

    await deactivateSurface("trayPanel");
    expect(isSurfaceActive("trayPanel")).toBe(false);
  });

  it("subscribeSurface is a no-op until the surface is active, then follows the host", async () => {
    const idle = vi.fn();
    const stopIdle = subscribeSurface("trayPanel", idle);
    stopIdle();
    expect(idle).not.toHaveBeenCalled();
    await activateSurface("trayPanel");
    const live = vi.fn();
    const stop = subscribeSurface("trayPanel", live);
    expect(isSurfaceActive("trayPanel")).toBe(true);
    stop();
    await deactivateSurface("trayPanel");
  });
});
