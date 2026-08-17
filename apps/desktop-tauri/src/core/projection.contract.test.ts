import { describe, expect, it } from "vitest";
import fromBridgeSource from "./fromBridge.ts?raw";
import projectionSource from "./projection.ts?raw";

const PROVIDER_NAME_COMPARE =
  /(?:displayName|providerId)\s*[=!]==\s*['"](?!auto['"])[^'"]+['"]/;
const DISPLAY_NAME_METHOD =
  /displayName\s*\.\s*(includes|startsWith|endsWith|toLowerCase)/;
const PROVIDER_SWITCH = /switch\s*\(\s*[\w.]*(?:displayName|providerId)\s*\)/;

describe("projection has no provider display-name branches", () => {
  it("fails if projection.ts compares provider display names or ids", () => {
    expect(projectionSource).not.toMatch(PROVIDER_NAME_COMPARE);
    expect(projectionSource).not.toMatch(DISPLAY_NAME_METHOD);
    expect(projectionSource).not.toMatch(PROVIDER_SWITCH);
  });

  it("fails if fromBridge.ts special-cases provider names", () => {
    expect(fromBridgeSource).not.toMatch(PROVIDER_NAME_COMPARE);
    expect(fromBridgeSource).not.toMatch(DISPLAY_NAME_METHOD);
    expect(fromBridgeSource).not.toMatch(PROVIDER_SWITCH);
  });
});
