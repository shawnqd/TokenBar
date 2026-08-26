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

function noop(): void {}

function makeLifecycle(): SurfaceLifecycle {
  return {
    create: noop,
    prepare: noop,
    reveal: noop,
    hide: noop,
    dispose: noop,
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
    lifecycle: makeLifecycle(),
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
    lifecycle: makeLifecycle(),
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
    lifecycle: makeLifecycle(),
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
    lifecycle: makeLifecycle(),
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
