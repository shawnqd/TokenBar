import { recordLifecycleTrace } from "./runtimeDiagnostics";
import { getCoreBridgeStore } from "./useCoreBridge";

export type SurfaceKind =
  | "trayPanel"
  | "floatBar"
  | "taskbarStatus"
  | "settings";

export type ActivationPolicy = "always" | "setting-gated" | "on-demand";
export type HostType = "tauri-webview" | "native-child";
export type LifecyclePhase =
  | "create"
  | "prepare"
  | "reveal"
  | "hide"
  | "dispose";

export interface SurfaceLifecycle {
  create: () => Promise<void> | void;
  prepare: () => Promise<void> | void;
  reveal: () => Promise<void> | void;
  hide: () => Promise<void> | void;
  dispose: () => Promise<void> | void;
}

export interface SurfaceDescriptor {
  pluginId: string;
  kind: SurfaceKind;
  activationPolicy: ActivationPolicy;
  hostType: HostType;
  settingsNamespace: string | null;
  requiredCapabilities: string[];
  lifecycle: SurfaceLifecycle;
  version: string;
  targetKinds: string[];
}

const surfaceUnsubs = new Map<SurfaceKind, () => void>();

function hostLifecycle(kind: SurfaceKind): SurfaceLifecycle {
  return {
    create: () => {
      recordLifecycleTrace(`builtin:${kind}`, "create");
    },
    prepare: () => {
      // Replica already seeded by ensureAppRuntimeBooted. Prepare only
      // asserts the process cache is attached for this surface.
      if (!getCoreBridgeStore() && kind !== "taskbarStatus") {
        recordLifecycleTrace(`builtin:${kind}`, "prepare", "replica not attached");
      }
    },
    reveal: () => {
      const store = getCoreBridgeStore();
      const previous = surfaceUnsubs.get(kind);
      if (previous) previous();
      if (store) {
        surfaceUnsubs.set(
          kind,
          store.subscribe(() => {
            notifyHost();
          }),
        );
      }
    },
    hide: () => {
      recordLifecycleTrace(`builtin:${kind}`, "hide");
    },
    dispose: () => {
      const unsub = surfaceUnsubs.get(kind);
      if (unsub) unsub();
      surfaceUnsubs.delete(kind);
    },
  };
}

export const SURFACE_DESCRIPTORS: Record<SurfaceKind, SurfaceDescriptor> = {
  trayPanel: {
    pluginId: "builtin:trayPanel",
    kind: "trayPanel",
    activationPolicy: "always",
    hostType: "tauri-webview",
    settingsNamespace: null,
    requiredCapabilities: [],
    lifecycle: hostLifecycle("trayPanel"),
    version: "1.0.0",
    targetKinds: ["summary", "provider"],
  },
  floatBar: {
    pluginId: "builtin:floatBar",
    kind: "floatBar",
    activationPolicy: "setting-gated",
    hostType: "tauri-webview",
    settingsNamespace: "floatBar",
    requiredCapabilities: ["hasQuota"],
    lifecycle: hostLifecycle("floatBar"),
    version: "1.0.0",
    targetKinds: ["provider"],
  },
  taskbarStatus: {
    pluginId: "builtin:taskbarStatus",
    kind: "taskbarStatus",
    activationPolicy: "setting-gated",
    hostType: "native-child",
    settingsNamespace: "taskbar",
    requiredCapabilities: ["hasQuota", "hasBalance"],
    lifecycle: hostLifecycle("taskbarStatus"),
    version: "1.0.0",
    targetKinds: ["provider"],
  },
  settings: {
    pluginId: "builtin:settings",
    kind: "settings",
    activationPolicy: "on-demand",
    hostType: "tauri-webview",
    settingsNamespace: "settings",
    requiredCapabilities: [],
    lifecycle: hostLifecycle("settings"),
    version: "1.0.0",
    targetKinds: ["settings"],
  },
};

export function getSurface(
  kind: SurfaceKind,
): SurfaceDescriptor | undefined {
  return SURFACE_DESCRIPTORS[kind];
}

export function listSurfaces(): SurfaceDescriptor[] {
  return Object.values(SURFACE_DESCRIPTORS);
}

export function canActivate(
  kind: SurfaceKind,
  settings: Record<string, unknown>,
): boolean {
  const desc = SURFACE_DESCRIPTORS[kind];
  if (!desc) return false;
  if (desc.activationPolicy === "always") return true;
  if (desc.activationPolicy === "on-demand") return true;
  // setting-gated
  if (kind === "floatBar") {
    // support multiple shapes: floatBarEnabled boolean, or nested floatBar.enabled
    const v =
      (settings as Record<string, unknown>)["floatBarEnabled"] ??
      (settings as Record<string, unknown>)["floatBar.enabled"] ??
      (settings as Record<string, unknown>)["floatBar"] ??
      null;
    if (typeof v === "boolean") return v;
    if (v && typeof v === "object") {
      const nested = v as Record<string, unknown>;
      if (typeof nested["enabled"] === "boolean") return nested["enabled"] as boolean;
    }
    return false;
  }
  if (kind === "taskbarStatus") {
    const v =
      (settings as Record<string, unknown>)["taskbarWidgetEnabled"] ??
      (settings as Record<string, unknown>)["taskbar.enabled"] ??
      (settings as Record<string, unknown>)["taskbar"] ??
      null;
    if (typeof v === "boolean") return v;
    if (v && typeof v === "object") {
      const nested = v as Record<string, unknown>;
      if (typeof nested["enabled"] === "boolean") return nested["enabled"] as boolean;
    }
    return false;
  }
  return false;
}

export async function runLifecycle(
  kind: SurfaceKind,
  phase: LifecyclePhase,
): Promise<void> {
  const desc = getSurface(kind);
  if (!desc) throw new Error(`unknown surface ${kind}`);
  const fn = desc.lifecycle[phase];
  if (!fn) return;
  await fn();
}

export type WindowSurfaceKind = Exclude<SurfaceKind, "taskbarStatus">;

const active = new Map<SurfaceKind, { at: number; error?: string }>();
const hostListeners = new Set<() => void>();

function notifyHost(): void {
  for (const listener of [...hostListeners]) {
    try {
      listener();
    } catch {
      // surface subscribers must not break the host
    }
  }
}

/** Window label is host input; the returned kind is what production routing uses. */
export function kindFromWindowLabel(label: string): WindowSurfaceKind | null {
  if (label === "settings") return "settings";
  if (label === "floatbar") return "floatBar";
  if (label === "flyout") return "trayPanel";
  return null;
}

export function subscribeHost(listener: () => void): () => void {
  hostListeners.add(listener);
  return () => {
    hostListeners.delete(listener);
  };
}

export function activeSurfaces(): SurfaceDescriptor[] {
  const out: SurfaceDescriptor[] = [];
  for (const kind of active.keys()) {
    const desc = getSurface(kind);
    if (desc) out.push(desc);
  }
  return out;
}

export function isSurfaceActive(kind: SurfaceKind): boolean {
  return active.has(kind);
}

export function surfaceHostError(kind: SurfaceKind): string | undefined {
  return active.get(kind)?.error;
}

export async function activateSurface(
  kind: SurfaceKind,
  settings?: Record<string, unknown>,
): Promise<void> {
  const desc = getSurface(kind);
  if (!desc) {
    recordLifecycleTrace(kind, "activate", `unknown surface ${kind}`);
    return;
  }
  if (settings && !canActivate(kind, settings)) {
    recordLifecycleTrace(kind, "skip", "activation policy blocked");
    return;
  }
  try {
    await runLifecycle(kind, "create");
    await runLifecycle(kind, "prepare");
    await runLifecycle(kind, "reveal");
    active.set(kind, { at: Date.now() });
    recordLifecycleTrace(desc.pluginId, "reveal");
  } catch (error) {
    const msg = error instanceof Error ? error.message : String(error);
    active.set(kind, { at: Date.now(), error: msg });
    recordLifecycleTrace(desc.pluginId, "reveal", msg);
  }
  notifyHost();
}

export async function deactivateSurface(kind: SurfaceKind): Promise<void> {
  const desc = getSurface(kind);
  try {
    if (desc) {
      await runLifecycle(kind, "hide");
      await runLifecycle(kind, "dispose");
    }
  } catch (error) {
    const msg = error instanceof Error ? error.message : String(error);
    recordLifecycleTrace(desc?.pluginId ?? kind, "dispose", msg);
  }
  active.delete(kind);
  recordLifecycleTrace(desc?.pluginId ?? kind, "hide");
  notifyHost();
}

export function subscribeSurface(
  kind: SurfaceKind,
  listener: () => void,
): () => void {
  if (!active.has(kind)) {
    return () => {};
  }
  return subscribeHost(listener);
}

export function resetSurfaceHostForTest(): void {
  for (const unsub of surfaceUnsubs.values()) {
    try {
      unsub();
    } catch {
      // test reset
    }
  }
  surfaceUnsubs.clear();
  active.clear();
  hostListeners.clear();
}
