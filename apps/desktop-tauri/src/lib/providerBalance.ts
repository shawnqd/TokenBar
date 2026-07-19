import type { ProviderDetail, ProviderUsageSnapshot } from "../types/bridge";

/**
 * Normalized "prepaid balance / status" view for balance-type providers.
 *
 * The backend does not yet expose balance as structured data. Instead each
 * balance provider stuffs a human-readable string into a rate-window field that
 * is semantically meant for quota/reset text, and every provider does it
 * differently:
 *
 *   • DeepSeek  — synthesizes a 0%/100% primary window and puts the balance in
 *                 `primary.resetDescription`, e.g.
 *                 "¥38.88 (Paid: ¥38.88 / Granted: ¥0.00)".
 *                 `planName` echoes "CNY balance: ¥38.88".
 *   • MiMo      — primary window is a "No token-plan usage" marker when the
 *                 account has no token plan; the balance lives in
 *                 `secondary.resetDescription`, e.g.
 *                 "12.50 CNY balance (Paid: 8.25 CNY / Granted: 4.25 CNY)".
 *                 With a token plan, `primary` is a real quota window and the
 *                 balance is still in `secondary`.
 *   • MiMo API  — first validates its API key, then reads the associated MiMo
 *                 console session for its pay-as-you-go balance when available.
 *
 * This helper is the single place that understands those encodings, so the card
 * can render one consistent block. When the backend later splits these into
 * clean structured fields, only this file needs to change.
 */
export interface BalanceView {
  /** "balance" → currency amount (Option A layout); "status" → plain text. */
  kind: "balance" | "status";
  /** Section title, e.g. "余额" or "API 状态". */
  title: string;
  /** Formatted amount for balances, or the status text for status views. */
  amount: string;
  /** Muted secondary line for balances, e.g. "含赠送 ¥0.00", or null. */
  breakdown: string | null;
  /** True when the value could not be read (e.g. "Balance unavailable"). */
  unavailable: boolean;
  /** Original raw text, kept for tooltips/fallback. */
  raw: string;
}

/** Which rate windows carry balance/marker data and must not render as quota. */
export type BalanceWindow = "primary" | "secondary";

export interface ProviderBalanceInfo {
  /** Parsed balance/status to render in the unified block, or null. */
  balance: BalanceView | null;
  /** Windows to exclude from the quota metric rows (balance carriers/markers). */
  excludeWindows: Set<BalanceWindow>;
  /** True when `planName` is really a balance echo and should be hidden. */
  suppressPlanBadge: boolean;
}

export type ProviderDetailBalanceWindow = "session" | "weekly";

export interface ProviderDetailBalanceInfo {
  balance: BalanceView | null;
  excludeWindows: Set<ProviderDetailBalanceWindow>;
  suppressPlan: boolean;
  balanceOnly: boolean;
}

const MIMO_NO_PLAN_MARKER = "No active MiMo Token Plan";
const BALANCE_TITLE = "余额";
const STATUS_TITLE = "API 状态";

const NONE: ProviderBalanceInfo = {
  balance: null,
  excludeWindows: new Set(),
  suppressPlanBadge: false,
};

export function getProviderBalance(
  provider: ProviderUsageSnapshot,
): ProviderBalanceInfo {
  if (provider.error) return NONE;

  if (provider.providerId === "deepseek") {
    const balance = parseBalanceText(provider.primary.resetDescription);
    return {
      balance,
      // The primary window is a synthetic 0/100% placeholder — never a quota.
      excludeWindows: balance ? new Set<BalanceWindow>(["primary"]) : new Set(),
      suppressPlanBadge: !!balance,
    };
  }

  if (provider.providerId === "mimo") {
    const noPlan = provider.primary.resetDescription === MIMO_NO_PLAN_MARKER;
    const balance = parseBalanceText(provider.secondary?.resetDescription ?? null);
    const exclude = new Set<BalanceWindow>();
    if (noPlan) exclude.add("primary");
    if (balance) exclude.add("secondary");
    const status = noPlan && !balance
      ? {
          kind: "status" as const,
          title: "Token Plan",
          amount: "No active MiMo Token Plan",
          breakdown: null,
          unavailable: true,
          raw: MIMO_NO_PLAN_MARKER,
        }
      : null;
    return {
      balance: balance ?? status,
      excludeWindows: exclude,
      // With a token plan the badge is a real plan name; keep it. Without one,
      // the badge is a "{balance} {currency}" echo, so hide it.
      suppressPlanBadge: noPlan && !!balance,
    };
  }

  if (provider.providerId === "mimoapi") {
    const raw = provider.primary.resetDescription?.trim();
    const balance = parseBalanceText(raw ?? null);
    return {
      balance: balance ?? {
        kind: "status",
        title: STATUS_TITLE,
        amount: raw && raw.length > 0 ? raw : "API key status unavailable",
        breakdown: null,
        unavailable: !raw || raw.length === 0,
        raw: raw ?? "",
      },
      excludeWindows: new Set<BalanceWindow>(["primary"]),
      suppressPlanBadge: false,
    };
  }

  return NONE;
}

/**
 * Settings uses a ProviderDetail DTO with session/weekly field names, while
 * tray cards use primary/secondary. Normalize both through the same balance
 * parser so a balance-only account cannot fall back to a synthetic 0% bar.
 */
export function getProviderDetailBalance(
  provider: ProviderDetail,
): ProviderDetailBalanceInfo {
  if (provider.lastError) {
    return {
      balance: null,
      excludeWindows: new Set(),
      suppressPlan: false,
      balanceOnly: false,
    };
  }

  if (provider.id === "deepseek") {
    const balance = parseBalanceText(provider.session?.resetDescription ?? null);
    return {
      balance,
      excludeWindows: balance
        ? new Set<ProviderDetailBalanceWindow>(["session"])
        : new Set(),
      suppressPlan: !!balance,
      balanceOnly: !!balance,
    };
  }

  if (provider.id === "mimo") {
    const noPlan = provider.session?.resetDescription === MIMO_NO_PLAN_MARKER;
    const balance = parseBalanceText(provider.weekly?.resetDescription ?? null);
    const excludeWindows = new Set<ProviderDetailBalanceWindow>();
    if (noPlan) excludeWindows.add("session");
    if (balance) excludeWindows.add("weekly");
    const balanceOnly = noPlan && !!balance;
    return {
      balance,
      excludeWindows,
      suppressPlan: balanceOnly,
      balanceOnly,
    };
  }

  return {
    balance: null,
    excludeWindows: new Set(),
    suppressPlan: false,
    balanceOnly: false,
  };
}

/**
 * Extract a balance amount and optional paid/granted breakdown from one of the
 * ad-hoc backend strings. Returns null for empty/non-balance text.
 */
export function parseBalanceText(raw: string | null): BalanceView | null {
  if (!raw) return null;
  const text = raw.trim();
  if (!text || text === MIMO_NO_PLAN_MARKER) return null;

  // A successful API-key probe is a status sentence, not monetary data. Do
  // not place it under the \"Balance\" heading simply because this helper is
  // shared by balance-capable providers.
  const hasMonetaryBalanceShape =
    /^CNY\s*[¥￥]?\s*\d[\d,]*(?:\.\d+)?\s*(?:balance\b|\(\s*Paid\s*:)/i.test(text) ||
    /^(?:[¥￥$]\s*)?\d[\d,]*(?:\.\d+)?\s*(?:[A-Za-z]{3}|[¥￥$])?\s+balance\b/i.test(text) ||
    /^余额\s*[:：]/.test(text) ||
    // DeepSeek returns its prepaid amount as "¥38.81 (Paid: … / Granted: …)"
    // rather than "38.81 CNY balance". Treat that explicit monetary breakdown
    // as a balance too, otherwise its synthetic 0% rate window leaks into UI.
    /^(?:[¥￥$]\s*)?\d[\d,]*(?:\.\d+)?\s*\(\s*Paid\s*:/i.test(text);
  if (!hasMonetaryBalanceShape && !/\bbalance unavailable\b|余额不可用/i.test(text)) {
    return null;
  }

  if (/unavailable/i.test(text)) {
    return {
      kind: "balance",
      title: BALANCE_TITLE,
      amount: "—",
      breakdown: "暂不可用",
      unavailable: true,
      raw: text,
    };
  }

  // Amount = leading token before the first "(", "（", " — ", or " balance".
  const head = text.split(/\s*[（(]|\s+—\s+|\s+balance\b/i)[0]?.trim();
  const amount = normalizeBalanceCurrency(head && head.length > 0 ? head : text);

  // Breakdown from the "(Paid: X / Granted: Y)" tail — surface the gifted part.
  let breakdown: string | null = null;
  const paren = text.match(/[（(]([^）)]*)[）)]/);
  if (paren) {
    const granted = paren[1].match(/Granted:\s*([^/）)]+)/i);
    if (granted) breakdown = `含赠送 ${normalizeBalanceCurrency(granted[1].trim())}`;
  }

  return {
    kind: "balance",
    title: BALANCE_TITLE,
    amount,
    breakdown,
    unavailable: false,
    raw: text,
  };
}

/** Use one compact currency convention across every balance provider. */
function normalizeBalanceCurrency(value: string): string {
  const normalized = value.trim().replace(/^￥/, "¥");
  const cnySuffix = normalized.match(/^¥?(-?\d[\d,]*(?:\.\d+)?)\s*CNY$/i);
  if (cnySuffix) return `¥${cnySuffix[1]}`;
  const cnyPrefix = normalized.match(/^CNY\s*¥?(-?\d[\d,]*(?:\.\d+)?)$/i);
  if (cnyPrefix) return `¥${cnyPrefix[1]}`;
  return normalized;
}
