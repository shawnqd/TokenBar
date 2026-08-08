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
  return {
    availability,
    primary: resolvePrimaryAuth(availability),
    // `null` capabilities — not loaded yet, or the command is not registered —
    // falls back to the old universal behaviour rather than hiding the entry.
    showApiKey: capabilities ? capabilities.supportsApiKey : true,
    showCookieSource: providerId !== "codex",
  };
}
