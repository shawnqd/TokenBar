import { describe, expect, it } from "vitest";
import { localizeProviderError } from "./providerErrorText";
import { ALL_LOCALE_KEYS, type LocaleKey } from "../i18n/keys";

/** Key-name passthrough stands in for the real catalog: assertions match on
 *  which key was requested, not on any locale's wording. */
const t = (key: LocaleKey) => key;

describe("localizeProviderError", () => {
  it("maps network categories to their reason keys", () => {
    expect(localizeProviderError("network error: timeout: operation timed out", t)).toBe(
      "ProviderIssueNetworkTimeout",
    );
    expect(localizeProviderError("network error: connect: connection refused", t)).toBe(
      "ProviderIssueNetworkConnectionFailed",
    );
    expect(
      localizeProviderError("network error: body (https://api.example.com/v1/org_123)", t),
    ).toBe("ProviderIssueNetworkRequestFailed：body");
  });

  it("keeps sign-in, unsupported-source and empty-usage categories localized", () => {
    expect(localizeProviderError("Authentication required", t)).toBe(
      "ProviderIssueSignInRequired",
    );
    expect(localizeProviderError("oauth credentials not found", t)).toBe(
      "ProviderIssueSignInRequired",
    );
    expect(
      localizeProviderError("Source mode 'cli' not supported for this provider", t),
    ).toBe("ProviderIssueUnsupportedSourceModePrefix");
    expect(localizeProviderError("provider returned no usage data", t)).toBe(
      "ProviderIssueFetchNeedsAttention",
    );
  });

  it("localizes the standalone timeout and parse failures", () => {
    expect(localizeProviderError("Timeout", t)).toBe("ProviderIssueNetworkTimeout");
    expect(localizeProviderError("Parse error: expected value at line 1", t)).toBe(
      "ProviderIssueParseFailed",
    );
    expect(localizeProviderError("Failed to parse response: invalid token", t)).toBe(
      "ProviderIssueParseFailed",
    );
  });

  it("localizes the missing-cookie and not-installed categories", () => {
    expect(localizeProviderError("No cookies available for web API", t)).toBe(
      "ProviderIssueNoCookies",
    );
    expect(localizeProviderError("Provider not installed: codex", t)).toBe(
      "ProviderIssueNotInstalled：codex",
    );
  });

  it("classifies HTTP status codes from every backend shape", () => {
    expect(localizeProviderError("Codex API returned 401", t)).toBe(
      "ProviderIssueSignInRequired",
    );
    expect(localizeProviderError("Factory auth API returned status 403", t)).toBe(
      "ProviderIssueSignInRequired",
    );
    expect(localizeProviderError("Chutes usage returned status 429", t)).toBe(
      "ProviderIssueCategoryRateLimit",
    );
    expect(localizeProviderError("Azure OpenAI API error: HTTP 503: overloaded", t)).toBe(
      "ProviderIssueCategoryUpstream",
    );
    expect(localizeProviderError("DeepSeek API returned status 400", t)).toBe(
      "ProviderIssueNetworkRequestFailed",
    );
  });

  it("maps rate-limit and auth keyword fallbacks without a status code", () => {
    expect(localizeProviderError("provider rate limited the request", t)).toBe(
      "ProviderIssueCategoryRateLimit",
    );
    expect(localizeProviderError("OAuth error: invalid_grant", t)).toBe(
      "ProviderIssueSignInRequired",
    );
    expect(localizeProviderError("API request failed: connection reset", t)).toBe(
      "ProviderIssueNetworkRequestFailed",
    );
  });

  it("falls through verbatim so new backend categories stay visible", () => {
    const raw = "Missing userStatus";
    expect(localizeProviderError(raw, t)).toBe(raw);
  });

  it("only emits keys that exist in the catalog", () => {
    const used = new Set<LocaleKey>();
    const collect = (key: LocaleKey) => {
      used.add(key);
      return key;
    };
    for (const raw of [
      "network error: timeout: x",
      "network error: connect: x",
      "network error: body",
      "authentication required",
      "Timeout",
      "Parse error: x",
      "No cookies available for web API",
      "Provider not installed: x",
      "Source mode 'web' not supported for this provider",
      "Codex API returned 401",
      "Chutes usage returned status 429",
      "Azure OpenAI API error: HTTP 503: x",
      "DeepSeek API returned status 400",
      "provider rate limited",
      "invalid_grant",
      "API request failed: x",
      "no usage data",
    ]) {
      localizeProviderError(raw, collect);
    }
    for (const key of used) {
      expect(ALL_LOCALE_KEYS).toContain(key);
    }
  });
});
