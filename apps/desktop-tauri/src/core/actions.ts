/**
 * Replaceable surface actions (CORE-02 types only). Pages dispatch these;
 * this lane does not call Tauri.
 */

export type SurfaceActionKind =
  | "refresh"
  | "openSettings"
  | "quit"
  | "selectProvider"
  | "openProviderDetail"
  | "openExternalUsage"
  | "openExternalStatus"
  | "triggerLogin";

export type SurfaceTarget =
  | { kind: "app" }
  | { kind: "summary" }
  | { kind: "settings"; tab?: string }
  | { kind: "provider"; providerId: string }
  | { kind: "providerOptional"; providerId: string | null };

export type SurfaceAction =
  | { type: "refresh"; target?: SurfaceTarget }
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
    };
