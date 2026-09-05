import { TEST_PROVIDER_CATALOG } from "../../../test/providerCatalog";
import { getProviderIcon } from "../../../components/providers/providerIcons";
import {
  resolveAuthEntries,
  userFacingAuthMethods,
  type UserAuthKind,
} from "../providers/authEntries";
import type { ProviderAuthCapabilitiesBridge } from "../../../types/bridge";

export type AuthMethod = UserAuthKind;
export type QuotaKind =
  | "claude"
  | "codex"
  | "gemini"
  | "balance"
  | "hybrid"
  | "status";
export interface FixtureProvider {
  id: string;
  name: string;
  color: string;
  enabled: boolean;
  status: "ok" | "stale" | "error";
  updated: string;
  metric: string;
  email: string | null;
  plan: string | null;
  methods: AuthMethod[];
  primary: AuthMethod;
  hasDash: boolean;
  hasStatus: boolean;
  hasBuy: boolean;
  hasToken: boolean;
  cookieSaved: boolean;
  cookieSource: boolean;
  loggedIn: boolean;
  creds: string;
  region: string | null;
  quota: QuotaKind;
  error: string | null;
  history: boolean;
}

export const METHOD_LABEL: Record<AuthMethod, string> = {
  signIn: "打开登录",
  cookie: "网页会话",
  apiKey: "密钥",
};

/** Mirrors `ProviderId::cookie_domain` — local catalog, not a network read. */
const COOKIE_DOMAIN: Record<string, string> = {
  claude: "claude.ai",
  cursor: "cursor.com",
  factory: "app.factory.ai",
  codex: "chatgpt.com",
  gemini: "aistudio.google.com",
  kiro: "kiro.dev",
  kimi: "kimi.moonshot.cn",
  kimik2: "platform.moonshot.cn",
  minimax: "platform.minimax.io",
  opencode: "opencode.ai",
  augment: "app.augmentcode.com",
  amp: "sourcegraph.com",
  antigravity: "antigravity.ai",
  alibaba: "dashscope.console.aliyun.com",
  alibabatokenplan: "bailian.console.aliyun.com",
  ollama: "ollama.com",
  t3chat: "t3.chat",
  perplexity: "perplexity.ai",
  abacus: "apps.abacus.ai",
  mistral: "admin.mistral.ai",
  opencodego: "opencode.ai",
  manus: "manus.im",
  mimo: "platform.xiaomimimo.com",
  mimoapi: "platform.xiaomimimo.com",
  commandcode: "commandcode.ai",
  grok: "grok.com",
  qoder: "qoder.com",
  sakana: "console.sakana.ai",
};

// Preview-only fallback. Runtime pages replace this with the Rust capability
// report; a dashboard URL alone is never treated as a login action.
const LOGIN_FLOW = new Set(["codex", "gemini", "copilot"]);

// Claude's bespoke control is only a keychain preference, not its login
// mechanism; keep the real browser/CLI choices visible for it.
const BESPOKE = new Set(["gemini", "jetbrains", "kiro"]);
const TOKEN_ACCOUNTS = new Set(["claude", "cursor"]);
const BUY = new Set(["deepseek"]);
const HISTORY = new Set(["codex"]);
const REGION: Record<string, string> = {
  azureopenai: "美国东部",
  vertexai: "美国东部",
  bedrock: "美国东部",
};

// Demo credentials are intentionally removed: the providers page must read
// real API keys / cookies / login state from the backend instead of pretending
// a provider is already configured.

function methodsFor(
  id: string,
  runtimeCapabilities?: ProviderAuthCapabilitiesBridge | null,
  runtime = false,
): AuthMethod[] {
  if (runtime) {
    if (!runtimeCapabilities) return [];
    const decision = resolveAuthEntries({
      providerId: id,
      cookieDomain: COOKIE_DOMAIN[id] ?? null,
      dashboardUrl: null,
      capabilities: runtimeCapabilities,
      isBespoke: BESPOKE.has(id),
    });
    return userFacingAuthMethods(decision.methods, id);
  }
  const cookieDomain = COOKIE_DOMAIN[id] ?? null;
  const decision = resolveAuthEntries({
    providerId: id,
    cookieDomain,
    dashboardUrl: null,
    capabilities: {
      supportsOAuth: LOGIN_FLOW.has(id),
      supportsCli: LOGIN_FLOW.has(id),
      supportsApiKey: true,
      // HTML 原型只覆盖有网页会话的 provider;无能力者由真实桥接能力驱动。
      supportsWeb: true,
      loginFlow: LOGIN_FLOW.has(id) ? `${id}_login` : null,
    },
    isBespoke: BESPOKE.has(id),
  });
  return userFacingAuthMethods(decision.methods, id);
}

function buildProvider(
  id: string,
  name: string,
  enabled: boolean,
  runtimeCapabilities?: ProviderAuthCapabilitiesBridge | null,
  runtime = false,
): FixtureProvider {
  const methods = methodsFor(id, runtimeCapabilities, runtime);
  const icon = getProviderIcon(id);
  return {
    id,
    name,
    color: icon.brandColor,
    enabled,
    status: enabled ? "ok" : "error",
    updated: "",
    metric: "",
    email: null,
    plan: null,
    methods,
    primary: methods[0] ?? "apiKey",
    hasDash: Boolean(COOKIE_DOMAIN[id] || LOGIN_FLOW.has(id)),
    hasStatus: id === "azureopenai" || id === "gemini" || id === "claude",
    hasBuy: BUY.has(id),
    hasToken: TOKEN_ACCOUNTS.has(id),
    cookieSaved: false,
    cookieSource: Boolean(COOKIE_DOMAIN[id]) && id !== "codex",
    loggedIn: false,
    creds: "",
    region: REGION[id] ?? null,
    quota: "status",
    error: null,
    history: HISTORY.has(id),
  };
}

export function buildProviderCatalog(
  catalog?: Array<{ id: string; displayName: string }>,
  enabledIds: string[] = [],
  orderIds: string[] = [],
  runtimeCapabilitiesById?: Record<string, ProviderAuthCapabilitiesBridge | null>,
): FixtureProvider[] {
  const source =
    catalog && catalog.length > 0
      ? catalog.map((row) => [row.id, row.displayName] as const)
      : TEST_PROVIDER_CATALOG;
  const byId = new Map(
    source.map(([id, name]) => [
      id,
      buildProvider(
        id,
        name,
        enabledIds.includes(id),
        runtimeCapabilitiesById?.[id],
        runtimeCapabilitiesById !== undefined,
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

/** Full catalog used by settings previews and provider dropdowns. */
export const HTML_PROVIDERS: FixtureProvider[] = buildProviderCatalog();

export const COOKIE_IMPORT_ID = "__cookie__";

export const COOKIE_IMPORT_TARGETS = Object.keys(COOKIE_DOMAIN).filter(
  (id) => id !== "codex",
);

export function listSub(provider: FixtureProvider): string {
  if (!provider.enabled) return "未配置";
  return provider.updated || "刚刚";
}

export function catalogChoices(
  enabledIds: string[] | undefined,
): { id: string; label: string }[] {
  const enabled = new Set(enabledIds ?? []);
  return HTML_PROVIDERS.filter((row) => enabled.has(row.id)).map((row) => ({
    id: row.id,
    label: row.name,
  }));
}
