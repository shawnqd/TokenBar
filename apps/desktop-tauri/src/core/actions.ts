/**
 * Replaceable surface actions (CORE-02 types only). Pages dispatch these;
 * this lane does not call Tauri.
 */

import type { Language, SettingsUpdate } from "../types/bridge";

export type SurfaceActionKind =
  | "refresh"
  | "openSettings"
  | "quit"
  | "selectProvider"
  | "openProviderDetail"
  | "openExternalUsage"
  | "openExternalStatus"
  | "triggerLogin"
  | "dismiss"
  | "reorderProviders"
  | "clearCache"
  | "setApiKey"
  | "removeApiKey"
  | "setManualCookie"
  | "removeManualCookie"
  | "importCookieFile"
  | "openProviderLogin"
  | "captureProviderLogin"
  | "closeProviderLogin"
  | "addTokenAccount"
  | "removeTokenAccount"
  | "setActiveTokenAccount"
  | "revokeCredentials"
  | "setCookieSource"
  | "setRegion"
  | "resetSettings"
  | "closeSettings"
  | "openExternalUrl"
  | "openPath"
  | "setWorkspaceId"
  | "setGatewayUrl"
  | "setIdePath"
  | "updateSettings"
  | "registerGlobalShortcut"
  | "unregisterGlobalShortcut"
  | "setUiLanguage"
  | "playNotificationSound";

export type SurfaceTarget =
  | { kind: "app" }
  | { kind: "summary" }
  | { kind: "settings"; tab?: string }
  | { kind: "provider"; providerId: string }
  | { kind: "providerOptional"; providerId: string | null };

export type SurfaceAction =
  | { type: "refresh"; target?: SurfaceTarget; force?: boolean }
  | { type: "openSettings"; target: Extract<SurfaceTarget, { kind: "settings" }> }
  | { type: "quit"; target: Extract<SurfaceTarget, { kind: "app" }> }
  | {
      type: "selectProvider";
      target: Extract<SurfaceTarget, { kind: "providerOptional" }>;
    }
  | {
      type: "openProviderDetail";
      target: Extract<SurfaceTarget, { kind: "provider" }>;
    }
  | {
      type: "openExternalUsage";
      target: Extract<SurfaceTarget, { kind: "provider" }>;
    }
  | {
      type: "openExternalStatus";
      target: Extract<SurfaceTarget, { kind: "provider" }>;
    }
  | {
      type: "triggerLogin";
      target: Extract<SurfaceTarget, { kind: "provider" }>;
    }
  | { type: "dismiss"; target: Extract<SurfaceTarget, { kind: "summary" }> }
  | {
      type: "reorderProviders";
      target: Extract<SurfaceTarget, { kind: "summary" }>;
      providerIds: string[];
    }
  | { type: "clearCache"; target: Extract<SurfaceTarget, { kind: "settings" }> }
  | {
      type: "setApiKey";
      target: Extract<SurfaceTarget, { kind: "provider" }>;
      apiKey: string;
      label?: string;
    }
  | { type: "removeApiKey"; target: Extract<SurfaceTarget, { kind: "provider" }> }
  | {
      type: "setManualCookie";
      target: Extract<SurfaceTarget, { kind: "provider" }>;
      cookieHeader: string;
    }
  | { type: "removeManualCookie"; target: Extract<SurfaceTarget, { kind: "provider" }> }
  | {
      type: "importCookieFile";
      target: Extract<SurfaceTarget, { kind: "settings" }>;
      contents: string;
      providerIds: string[];
    }
  | { type: "openProviderLogin"; target: Extract<SurfaceTarget, { kind: "provider" }> }
  | { type: "captureProviderLogin"; target: Extract<SurfaceTarget, { kind: "provider" }> }
  | { type: "closeProviderLogin"; target: Extract<SurfaceTarget, { kind: "summary" }> }
  | {
      type: "addTokenAccount";
      target: Extract<SurfaceTarget, { kind: "provider" }>;
      label: string;
      token: string;
    }
  | {
      type: "removeTokenAccount";
      target: Extract<SurfaceTarget, { kind: "provider" }>;
      accountId: string;
    }
  | {
      type: "setActiveTokenAccount";
      target: Extract<SurfaceTarget, { kind: "provider" }>;
      accountId: string;
    }
  | { type: "revokeCredentials"; target: Extract<SurfaceTarget, { kind: "provider" }> }
  | {
      type: "setCookieSource";
      target: Extract<SurfaceTarget, { kind: "provider" }>;
      source: string;
    }
  | {
      type: "setRegion";
      target: Extract<SurfaceTarget, { kind: "provider" }>;
      region: string;
    }
  | { type: "resetSettings"; target: Extract<SurfaceTarget, { kind: "settings" }> }
  | { type: "closeSettings"; target: Extract<SurfaceTarget, { kind: "settings" }> }
  | {
      type: "openExternalUrl";
      target: Extract<SurfaceTarget, { kind: "app" }>;
      url: string;
    }
  | {
      type: "openPath";
      target: Extract<SurfaceTarget, { kind: "app" }>;
      path: string;
    }
  | {
      type: "setWorkspaceId";
      target: Extract<SurfaceTarget, { kind: "provider" }>;
      workspaceId: string;
    }
  | {
      type: "setGatewayUrl";
      target: Extract<SurfaceTarget, { kind: "provider" }>;
      gatewayUrl: string;
    }
  | {
      type: "setIdePath";
      target: Extract<SurfaceTarget, { kind: "app" }>;
      path: string;
    }
  | {
      type: "updateSettings";
      target: Extract<SurfaceTarget, { kind: "settings" }>;
      patch: SettingsUpdate;
    }
  | {
      type: "registerGlobalShortcut";
      target: Extract<SurfaceTarget, { kind: "app" }>;
      accelerator: string;
    }
  | {
      type: "unregisterGlobalShortcut";
      target: Extract<SurfaceTarget, { kind: "app" }>;
    }
  | {
      type: "setUiLanguage";
      target: Extract<SurfaceTarget, { kind: "settings" }>;
      language: Language;
    }
  | {
      type: "playNotificationSound";
      target: Extract<SurfaceTarget, { kind: "settings" }>;
    };

/**
 * Canonical TS ↔ Rust wire payload. The in-process SurfaceAction *is* the
 * invoke body: nested `target.kind` / `target.providerId` / `target.tab`.
 * Rust `commands::surface_action` deserializes this exact JSON.
 */
export function surfaceActionWire(action: SurfaceAction): SurfaceAction {
  return action;
}
