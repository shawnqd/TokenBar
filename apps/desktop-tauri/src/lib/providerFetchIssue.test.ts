import { describe, expect, it } from "vitest";
import { classifyProviderFetchIssue } from "./providerFetchIssue";

describe("classifyProviderFetchIssue", () => {
  it("classifies auth failures", () => {
    expect(classifyProviderFetchIssue("OAuth credentials not found").category).toBe(
      "auth",
    );
    expect(classifyProviderFetchIssue("authentication required").mayBeTransient).toBe(
      false,
    );
  });

  it("classifies rate limits as transient", () => {
    const issue = classifyProviderFetchIssue("HTTP 429 too many requests");
    expect(issue.category).toBe("rate_limit");
    expect(issue.mayBeTransient).toBe(true);
  });

  it("classifies network errors as transient", () => {
    const issue = classifyProviderFetchIssue(
      "network error: error sending request for url (https://example.com)",
    );
    expect(issue.category).toBe("network_transient");
    expect(issue.mayBeTransient).toBe(true);
  });

  it("treats empty messages as stale/empty", () => {
    expect(classifyProviderFetchIssue(null).category).toBe("stale_or_empty");
  });
});
