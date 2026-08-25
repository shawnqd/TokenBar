import { describe, expect, it } from "vitest";
import { providerCapabilities } from "./providerCapabilities";

describe("providerCapabilities", () => {
  it("prefers the backend-reported flags when present", () => {
    const caps = providerCapabilities({
      providerId: "codex",
      capabilities: { outputSpeed: false, localUsage: true, providerDashboard: false, statusPage: false, login: false },
    });
    expect(caps.outputSpeed).toBe(false);
    expect(caps.localUsage).toBe(true);
    expect(caps.providerDashboard).toBe(false);
  });

  it("derives the known-parse fallback for snapshots without flags", () => {
    const codex = providerCapabilities({ providerId: "codex", capabilities: undefined });
    expect(codex.outputSpeed).toBe(true);
    expect(codex.localUsage).toBe(true);

    const cursor = providerCapabilities({ providerId: "cursor", capabilities: undefined });
    expect(cursor.outputSpeed).toBe(false);
    expect(cursor.localUsage).toBe(false);
  });
});
