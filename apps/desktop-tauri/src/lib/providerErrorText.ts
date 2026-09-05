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
  if (
    lower.includes("no usage data") ||
    lower.includes("no quota data") ||
    lower.includes("empty usage")
  ) {
    return t("ProviderIssueFetchNeedsAttention");
  }
  return message;
}
