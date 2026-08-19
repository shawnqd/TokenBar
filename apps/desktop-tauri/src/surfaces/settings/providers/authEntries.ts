/**
 * Which auth entries a provider's 认证来源 zone offers, and which one leads.
 *
 * Extracted from `AuthWorkspace` so it can be tested. That zone is the whole of
 * task package item 2 and had **no test coverage at all** — the decision below
 * is what once left Codex with an empty auth block, and nothing would have
 * caught it coming back.
 */

/** Which real UI methods a provider currently offers. */
export interface AuthEntryAvailability {
  bespoke: boolean;
  cookie: boolean;
  signIn: boolean;
}

export type PrimaryAuthKind = "bespoke" | "cookie" | "signIn" | "apiKey";

/** User-visible login methods. `bespoke` is never a fourth choice. */
export type UserAuthKind = "cookie" | "signIn" | "apiKey";

export function mapBespokeToUserKind(providerId: string): UserAuthKind {
  switch (providerId) {
    case "claude":
      return "cookie";
    case "gemini":
    case "jetbrains":
    case "kiro":
      return "signIn";
    default:
      return "apiKey";
  }
}

export function userFacingAuthMethods(
  methods: PrimaryAuthKind[],
  providerId: string,
): UserAuthKind[] {
  const out: UserAuthKind[] = [];
  for (const method of methods) {
    const mapped: UserAuthKind =
      method === "bespoke" ? mapBespokeToUserKind(providerId) : method;
    if (!out.includes(mapped)) {
      out.push(mapped);
    }
  }
  if (out.length === 0) out.push("apiKey");
  return out;
}

/** The subset of `ProviderAuthCapabilitiesBridge` this decision reads. */
export interface AuthCapabilityInput {
  supportsOAuth: boolean;
  supportsCli: boolean;
  supportsApiKey: boolean;
}

export interface AuthEntryInput {
  providerId: string;
  /** `null` when the provider has no cookie domain at all. */
  cookieDomain: string | null;
  /** `null` when there is nowhere for a browser sign-in to go. */
  dashboardUrl: string | null;
  /** `null` while `get_provider_auth_capabilities` has not answered yet. */
  capabilities: AuthCapabilityInput | null;
  /** Whether a hand-built credentials component exists for this provider. */
  isBespoke: boolean;
}

export interface AuthEntryDecision {
  availability: AuthEntryAvailability;
  primary: PrimaryAuthKind;
  showApiKey: boolean;
  showCookieSource: boolean;
  /**
   * Every method this provider can actually be authenticated with, in the
   * order they should be offered. `primary` is always the first.
   *
   * A provider is authenticated **one way at a time**, so these are mutually
   * exclusive choices rather than a lead card plus an "other methods" drawer.
   * The zone used to render them as the latter, which read as a hierarchy that
   * does not exist and buried a provider's only real option — Codex's sign-in
   * sat behind a collapsed row on a page whose visible card said "CLI".
   */
  methods: PrimaryAuthKind[];
}

/**
 * Pick the auth surface that stays expanded, in a fixed preference order.
 *
 * A hand-built credentials UI beats the generic entries; a cookie-domain
 * provider keeps its long-standing cookie-primary behaviour; only then does a
 * bare OAuth/CLI sign-in lead. `apiKey` is the guaranteed last resort —
 * `ApiKeySection` self-hides when a provider has no API-key entry, exactly as
 * it did before this list existed.
 */
export function resolvePrimaryAuth(
  availability: AuthEntryAvailability,
): PrimaryAuthKind {
  if (availability.bespoke) return "bespoke";
  if (availability.cookie) return "cookie";
  if (availability.signIn) return "signIn";
  return "apiKey";
}

/**
 * Resolve the whole zone in one place.
 *
 * Codex is the reason this function is worth having. It has no bespoke
 * component, its cookies are covered by its own auto/manual/off source picker
 * rather than by a plain paste card, and it authenticates by OAuth/CLI — so
 * every other branch declines and only the sign-in entry is left. Get any one
 * of those wrong and its 认证来源 zone renders empty, which is exactly what it
 * used to do.
 */
export function resolveAuthEntries(input: AuthEntryInput): AuthEntryDecision {
  const { providerId, cookieDomain, dashboardUrl, capabilities, isBespoke } = input;

  // Codex keeps its long-standing carve-out: its own auto/manual/off cookie
  // *source* picker already covers cookies for this provider, so a second,
  // plain cookie-paste card would be redundant.
  const cookie = cookieDomain !== null && providerId !== "codex";
  const supportsOAuth = capabilities?.supportsOAuth ?? false;
  const supportsCli = capabilities?.supportsCli ?? false;
  // `triggerProviderLogin` only has a real destination when the provider
  // advertises one, exactly as the 切换账号 quick action already gates itself.
  // Bespoke components tell their own OAuth/CLI story, so this appears only
  // where nothing else would.
  const signIn = !isBespoke && (supportsOAuth || supportsCli) && dashboardUrl !== null;

  const availability: AuthEntryAvailability = { bespoke: isBespoke, cookie, signIn };
  const primary = resolvePrimaryAuth(availability);
  // `null` capabilities — not loaded yet, or the command is not registered —
  // falls back to the old universal behaviour rather than hiding the entry.
  const showApiKey = capabilities ? capabilities.supportsApiKey : true;

  // Preference order, filtered to what exists. `primary` leads by construction
  // because `resolvePrimaryAuth` walks this same order.
  const methods: PrimaryAuthKind[] = [];
  if (availability.bespoke) methods.push("bespoke");
  if (availability.cookie) methods.push("cookie");
  if (availability.signIn) methods.push("signIn");
  if (showApiKey) methods.push("apiKey");
  // `resolvePrimaryAuth` returns `apiKey` as its last resort even where the
  // provider has none, so the list would otherwise come back empty and the
  // zone would render nothing at all.
  if (methods.length === 0) methods.push(primary);

  return { availability, primary, showApiKey, showCookieSource: providerId !== "codex", methods };
}
