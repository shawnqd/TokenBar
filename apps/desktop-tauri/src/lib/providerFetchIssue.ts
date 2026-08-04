/**
 * Shared classification of provider fetch failures for UI presentation
 * (TASK-021 item 3). Does not change retry protocol or credentials — it only
 * maps free-text errors into stable categories so every surface can show the
 * same tone without inventing recovery behaviour.
 */

export type ProviderFetchIssueCategory =
  | "auth"
  | "rate_limit"
  | "network_transient"
  | "upstream"
  | "stale_or_empty"
  | "unknown";

export interface ProviderFetchIssue {
  category: ProviderFetchIssueCategory;
  /** True only for categories that *may* recover without re-auth. */
  mayBeTransient: boolean;
}

export function classifyProviderFetchIssue(message: string | null | undefined): ProviderFetchIssue {
  if (!message || !message.trim()) {
    return { category: "stale_or_empty", mayBeTransient: true };
  }
  const lower = message.toLowerCase();

  if (
    lower.includes("oauth credentials not found") ||
    lower.includes("sign-in was not found") ||
    lower.includes("sign-in expired") ||
    lower.includes("authentication required") ||
    lower.includes("not signed in") ||
    lower.includes("auth.json not found") ||
    lower.includes("credentials not found") ||
    lower.includes("unauthorized") ||
    lower.includes("401")
  ) {
    return { category: "auth", mayBeTransient: false };
  }

  if (
    lower.includes("rate limit") ||
    lower.includes("too many requests") ||
    lower.includes("429") ||
    lower.includes("quota exceeded")
  ) {
    return { category: "rate_limit", mayBeTransient: true };
  }

  if (
    lower.startsWith("network error:") ||
    lower.includes("timed out") ||
    lower.includes("timeout") ||
    lower.includes("connection reset") ||
    lower.includes("temporarily unavailable") ||
    lower.includes("503") ||
    lower.includes("502") ||
    lower.includes("504")
  ) {
    return { category: "network_transient", mayBeTransient: true };
  }

  if (
    lower.includes("server error") ||
    lower.includes("500") ||
    lower.includes("upstream") ||
    lower.includes("bad gateway")
  ) {
    return { category: "upstream", mayBeTransient: true };
  }

  if (
    lower.includes("no data") ||
    lower.includes("not fetched yet") ||
    lower.includes("stale")
  ) {
    return { category: "stale_or_empty", mayBeTransient: true };
  }

  return { category: "unknown", mayBeTransient: false };
}

export function providerFetchIssueLocaleKey(
  category: ProviderFetchIssueCategory,
):
  | "ProviderIssueCategoryAuth"
  | "ProviderIssueCategoryRateLimit"
  | "ProviderIssueCategoryTransient"
  | "ProviderIssueCategoryUpstream"
  | "ProviderIssueCategoryStale"
  | "ProviderIssueCategoryUnknown" {
  switch (category) {
    case "auth":
      return "ProviderIssueCategoryAuth";
    case "rate_limit":
      return "ProviderIssueCategoryRateLimit";
    case "network_transient":
      return "ProviderIssueCategoryTransient";
    case "upstream":
      return "ProviderIssueCategoryUpstream";
    case "stale_or_empty":
      return "ProviderIssueCategoryStale";
    default:
      return "ProviderIssueCategoryUnknown";
  }
}
