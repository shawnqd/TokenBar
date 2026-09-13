import { getProviderIcon } from "../../../components/providers/providerIcons";
import {
  resolveAuthEntries,
  userFacingAuthMethods,
  type UserAuthKind,
} from "../providers/authEntries";
import type {
  ProviderAuthCapabilitiesBridge,
  ProviderCatalogEntry,
} from "../../../types/bridge";

export type AuthMethod = UserAuthKind;

export interface FixtureProvider {
  id: string;
  name: string;
  color: string;
  enabled: boolean;
  cookieDomain: string | null;
  methods: AuthMethod[];
  primary: AuthMethod | null;
}

export const METHOD_LABEL: Record<AuthMethod, string> = {
  signIn: "CLI / OAuth 登录",
  cookie: "已保存 Cookie",
  apiKey: "API 密钥",
};

// Claude's bespoke control is only a keychain preference, not its login
// mechanism; keep the real browser/CLI choices visible for it. This shapes
// which runtime-backed control renders — it fabricates no capability data.
const BESPOKE = new Set(["gemini", "jetbrains", "kiro"]);

function methodsFor(
  id: string,
  cookieDomain: string | null,
  capabilities: ProviderAuthCapabilitiesBridge | null | undefined,
): AuthMethod[] {
  if (!capabilities) return [];
  const decision = resolveAuthEntries({
    providerId: id,
    cookieDomain,
    dashboardUrl: null,
    capabilities,
    isBespoke: BESPOKE.has(id),
  });
  return userFacingAuthMethods(decision.methods, id);
}

function buildProvider(
  entry: ProviderCatalogEntry,
  enabled: boolean,
  capabilities?: ProviderAuthCapabilitiesBridge | null,
): FixtureProvider {
  const methods = methodsFor(entry.id, entry.cookieDomain, capabilities);
  return {
    id: entry.id,
    name: entry.displayName,
    color: getProviderIcon(entry.id).brandColor,
    enabled,
    cookieDomain: entry.cookieDomain,
    methods,
    primary: methods[0] ?? null,
  };
}

/**
 * Assemble the settings catalog from the Rust bootstrap payload only
 * (2026-09-06 source contract). There is deliberately no static fallback: an
 * empty or failed catalog renders the page's loading/error state instead of
 * a hardcoded provider list. Per-provider identity (cookie domain) and auth
 * entries come from the backend; a provider whose capabilities have not
 * resolved yet simply has no methods until they arrive.
 */
export function buildProviderCatalog(
  catalog: ProviderCatalogEntry[] | null | undefined,
  enabledIds: string[] = [],
  orderIds: string[] = [],
  runtimeCapabilitiesById?: Record<string, ProviderAuthCapabilitiesBridge | null>,
): FixtureProvider[] {
  if (!catalog || catalog.length === 0) return [];
  const byId = new Map(
    catalog.map((entry) => [
      entry.id,
      buildProvider(
        entry,
        enabledIds.includes(entry.id),
        runtimeCapabilitiesById?.[entry.id],
      ),
    ]),
  );
  const seen = new Set<string>();
  const ordered: FixtureProvider[] = [];
  for (const id of orderIds) {
    const row = byId.get(id);
    if (row) {
      ordered.push(row);
      seen.add(id);
    }
  }
  for (const row of byId.values()) {
    if (!seen.has(row.id)) ordered.push(row);
  }
  return ordered;
}

export const COOKIE_IMPORT_ID = "__cookie__";

/** Providers that accept an imported web session: every runtime catalog
 *  entry with a cookie domain, except Codex whose web session is
 *  supplementary-only and never its login path. */
export function cookieImportTargets(catalog: FixtureProvider[]): string[] {
  return catalog
    .filter((row) => row.cookieDomain && row.id !== "codex")
    .map((row) => row.id);
}

/** Enabled-provider choices for entry pickers, straight from the runtime
 *  catalog. Disabled providers are omitted exactly like the tray does. */
export function catalogChoices(
  catalog: ProviderCatalogEntry[] | null | undefined,
  enabledIds: string[] | undefined,
): { id: string; label: string }[] {
  const enabled = new Set(enabledIds ?? []);
  return (catalog ?? [])
    .filter((row) => enabled.has(row.id))
    .map((row) => ({ id: row.id, label: row.displayName }));
}
