import type { SurfaceAction, SurfaceActionKind, SurfaceTarget } from "./actions";

export type DispatchHandler = (
  action: SurfaceAction,
) => Promise<ActionResult> | ActionResult;

export type ActionResultStatus = "handled" | "unknown" | "error";

export interface ActionResult {
  status: ActionResultStatus;
  error?: string;
  data?: unknown;
}

export interface ActionDispatcher {
  dispatch: (action: SurfaceAction) => Promise<ActionResult>;
  route: (action: SurfaceAction) => string;
}

export function routeAction(action: SurfaceAction): string {
  // Map SurfaceAction -> target kind string matching SurfaceTarget
  // Use action.target.kind when present; fallback based on type
  const anyAction = action as unknown as { target?: SurfaceTarget; type: string };
  if (anyAction.target && typeof (anyAction.target as { kind?: unknown }).kind === "string") {
    return (anyAction.target as { kind: string }).kind;
  }
  // No target (e.g. refresh without target) -> app/summary
  switch (anyAction.type) {
    case "refresh":
      return "summary";
    case "openSettings":
      return "settings";
    case "quit":
      return "app";
    case "selectProvider":
      return "providerOptional";
    case "openProviderDetail":
    case "openExternalUsage":
    case "openExternalStatus":
    case "triggerLogin":
      return "provider";
    default:
      return "unknown";
  }
}

export interface ActionDispatcherOptions {
  /**
   * Used when a kind has no local handler. Production wires this to
   * `invokeSurfaceAction` so every user action hits Rust `surface_action`.
   */
  fallback?: DispatchHandler;
}

export function createActionDispatcher(
  handlers: Partial<Record<SurfaceActionKind, DispatchHandler>> = {},
  options?: ActionDispatcherOptions,
): ActionDispatcher {
  const dispatch = async (action: SurfaceAction): Promise<ActionResult> => {
    const kind = (action as { type: SurfaceActionKind }).type;
    const handler = handlers[kind] ?? options?.fallback;
    if (!handler) {
      return { status: "unknown", error: `unknown action: ${String(kind)}` };
    }
    try {
      const result = await handler(action);
      if (!result || typeof result.status !== "string") {
        return { status: "handled", data: result };
      }
      return result;
    } catch (e) {
      const msg = e instanceof Error ? e.message : String(e);
      return { status: "error", error: msg };
    }
  };

  return {
    dispatch,
    route: routeAction,
  };
}
