import { describe, expect, it } from "vitest";
import { resolveAuthEntries, resolvePrimaryAuth } from "./authEntries";

const OAUTH_ONLY = { supportsOAuth: true, supportsCli: true, supportsApiKey: false };
const API_ONLY = { supportsOAuth: false, supportsCli: false, supportsApiKey: true };

describe("resolveAuthEntries", () => {
  /**
   * The bug item 2 was raised for. Codex resolved to the bespoke branch, whose
   * dispatcher had no case for it, so its 认证来源 zone rendered help text and
   * nothing actionable.
   */
  it("gives Codex a sign-in entry rather than an empty zone", () => {
    const decision = resolveAuthEntries({
      providerId: "codex",
      cookieDomain: "chatgpt.com",
      dashboardUrl: "https://chatgpt.com/codex/settings/usage",
      capabilities: OAUTH_ONLY,
      isBespoke: false,
    });

    expect(decision.primary).toBe("signIn");
    expect(decision.availability.signIn).toBe(true);
    // Its own auto/manual/off source picker covers cookies; a second plain
    // paste card would be redundant.
    expect(decision.availability.cookie).toBe(false);
    expect(decision.showCookieSource).toBe(false);
  });

  it("keeps a cookie provider's cookie card as the lead entry", () => {
    const decision = resolveAuthEntries({
      providerId: "cursor",
      cookieDomain: "cursor.com",
      dashboardUrl: "https://cursor.com/dashboard",
      capabilities: API_ONLY,
      isBespoke: false,
    });
    expect(decision.primary).toBe("cookie");
    expect(decision.showCookieSource).toBe(true);
  });

  it("lets a hand-built credentials UI win over everything", () => {
    const decision = resolveAuthEntries({
      providerId: "claude",
      cookieDomain: "claude.ai",
      dashboardUrl: "https://claude.ai",
      capabilities: OAUTH_ONLY,
      isBespoke: true,
    });
    expect(decision.primary).toBe("bespoke");
    // A bespoke component tells its own OAuth story; a second sign-in entry
    // beside it would be two buttons for one thing.
    expect(decision.availability.signIn).toBe(false);
  });

  it("does not offer a sign-in with nowhere to go", () => {
    const decision = resolveAuthEntries({
      providerId: "somecli",
      cookieDomain: null,
      dashboardUrl: null,
      capabilities: OAUTH_ONLY,
      isBespoke: false,
    });
    expect(decision.availability.signIn).toBe(false);
    expect(decision.primary).toBe("apiKey");
  });

  it("shows the API key entry while capabilities are still loading", () => {
    // `null` is "the command has not answered yet", not "no API key". Hiding
    // the entry on null would blank the zone during every page load.
    const decision = resolveAuthEntries({
      providerId: "openrouter",
      cookieDomain: null,
      dashboardUrl: "https://openrouter.ai",
      capabilities: null,
      isBespoke: false,
    });
    expect(decision.showApiKey).toBe(true);
    expect(decision.primary).toBe("apiKey");
  });

  it("hides the API key entry when the provider has none", () => {
    const decision = resolveAuthEntries({
      providerId: "codex",
      cookieDomain: null,
      dashboardUrl: "https://chatgpt.com",
      capabilities: OAUTH_ONLY,
      isBespoke: false,
    });
    expect(decision.showApiKey).toBe(false);
  });
});

describe("resolvePrimaryAuth", () => {
  it("falls back to the API key when nothing else is available", () => {
    expect(
      resolvePrimaryAuth({ bespoke: false, cookie: false, signIn: false }),
    ).toBe("apiKey");
  });

  it("prefers bespoke, then cookie, then sign-in", () => {
    expect(resolvePrimaryAuth({ bespoke: true, cookie: true, signIn: true })).toBe(
      "bespoke",
    );
    expect(resolvePrimaryAuth({ bespoke: false, cookie: true, signIn: true })).toBe(
      "cookie",
    );
    expect(resolvePrimaryAuth({ bespoke: false, cookie: false, signIn: true })).toBe(
      "signIn",
    );
  });
});
