import type { LocaleKey } from "../i18n/keys";

/**
 * The tray flyout and the startup dashboard render raw provider errors through
 * MenuCard, independently of the Provider settings detail pane. Translate the
 * stable error categories here while leaving the original diagnostic available
 * through the Copy button.
 */
export function localizeProviderError(
  message: string,
  t: (key: LocaleKey) => string,
): string {
  const lower = message.toLowerCase();
  // Say *why*, not *where*. This used to print the failed URL and nothing
  // else, which was the least useful half of the error and — on Claude — put
  // the account's organization id on screen. The reason now survives from Rust
  // (see `describe_network_error`), and a timeout or a refused connection is
  // what actually tells the user whether to retry or to check the network.
  const network = message.match(/^network error:\s*(.+)$/is);
  if (network) {
    const detail = network[1].trim();
    if (/^timeout:/i.test(detail)) return t("ProviderIssueNetworkTimeout");
    if (/^connect:/i.test(detail)) return t("ProviderIssueNetworkConnectionFailed");
    // Anything else: name it as a request failure and append the cause chain
    // with the URL stripped, so the message stays diagnostic without carrying
    // an account identifier. The unedited original is still on the Copy button.
    const withoutUrl = detail.replace(/\s*\(https?:\/\/[^\s)]+\)/g, "").trim();
    return withoutUrl
      ? t("ProviderIssueNetworkRequestFailed") + "：" + withoutUrl
      : t("ProviderIssueNetworkRequestFailed");
  }
  if (lower.startsWith("network error:")) {
    return t("ProviderIssueNetworkConnectionFailed");
  }
  if (
    lower.includes("oauth credentials not found") ||
    lower.includes("sign-in was not found") ||
    lower.includes("sign-in expired") ||
    lower === "authentication required"
  ) {
    return t("ProviderIssueSignInRequired");
  }
  // Standalone timeout (ProviderError::Timeout) — same advice as a network
  // timeout: usually transient, refresh to retry.
  if (lower.trim() === "timeout") {
    return t("ProviderIssueNetworkTimeout");
  }
  // Parse failures — serde/JSON chains are noise in a hover tip; the category
  // tells the user the provider answered with something unusable.
  if (/^(parse error:|failed to parse)/i.test(message.trim())) {
    return t("ProviderIssueParseFailed");
  }
  if (lower.startsWith("no cookies available")) {
    return t("ProviderIssueNoCookies");
  }
  const notInstalled = message.match(/^provider not installed:\s*(.+)$/is);
  if (notInstalled) {
    const detail = notInstalled[1].trim();
    return detail
      ? `${t("ProviderIssueNotInstalled")}：${detail}`
      : t("ProviderIssueNotInstalled");
  }
  if (/not supported for this provider/i.test(message)) {
    return t("ProviderIssueUnsupportedSourceModePrefix");
  }
  // Every provider formats non-2xx replies as "…returned status 500",
  // "HTTP 401", "status code 403" or "API error 429: …". The status code is
  // the one part that maps to an actionable reason, so classify on it.
  const status = message.match(
    /\b(?:https?|status code|returned status|returned|api error)\s*:?\s*(\d{3})\b/i,
  );
  if (status) {
    const code = Number(status[1]);
    if (code === 401 || code === 403) return t("ProviderIssueSignInRequired");
    if (code === 429) return t("ProviderIssueCategoryRateLimit");
    if (code >= 500) return t("ProviderIssueCategoryUpstream");
    return t("ProviderIssueNetworkRequestFailed");
  }
  if (/rate[ -]?limit|too many requests/i.test(message)) {
    return t("ProviderIssueCategoryRateLimit");
  }
  if (
    /unauthorized|forbidden|invalid[ _-]?api[ _-]?key|invalid[_ ]?token|token expired|invalid_grant/i.test(
      message,
    )
  ) {
    return t("ProviderIssueSignInRequired");
  }
  if (/^api request failed/i.test(message.trim())) {
    return t("ProviderIssueNetworkRequestFailed");
  }
  if (
    lower.includes("no usage data") ||
    lower.includes("no quota data") ||
    lower.includes("empty usage")
  ) {
    return t("ProviderIssueFetchNeedsAttention");
  }
  return message;
}
