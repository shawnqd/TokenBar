import { describe, expect, it } from "vitest";
import { normalizeLegacyLine } from "./TaskbarStripPreview";

describe("normalizeLegacyLine", () => {
  it("keeps the ready value and resolves nothing when the colour cannot be matched", () => {
    const cell = normalizeLegacyLine(
      { glyph: "¥", color: "#4d6bfe", text: "¥38.38" },
      undefined,
    );
    expect(cell.state).toBe("ready");
    expect(cell.tag).toBe("");
    expect(cell.value).toBe("¥38.38");
    // Colour "#4d6bfe" is not a known brand colour → no official mark.
    expect(cell.providerId).toBe("");
    expect(cell.icon).toBeNull();
    expect(cell.glyph).toBe("¥");
  });

  it("splits a tag + value out of the legacy text", () => {
    const cell = normalizeLegacyLine(
      { glyph: "◆", color: "#49a3b0", text: "周 18%" },
      undefined,
    );
    expect(cell.tag).toBe("周");
    expect(cell.value).toBe("18%");
  });

  it("resolves the provider id from the entry, not the colour", () => {
    const cell = normalizeLegacyLine(
      { glyph: "◆", color: "#000000", text: "5小时 59%" },
      { providerId: "codex", window: "session" },
    );
    expect(cell.providerId).toBe("codex");
    expect(cell.icon?.assetId).toBe("codex");
    expect(cell.icon?.brandColor).toBe("#49a3b0");
    expect(cell.icon?.fallbackGlyph).toBe("◆");
  });

  it("recognises a known brand colour as a fallback when there is no entry", () => {
    const cell = normalizeLegacyLine(
      { glyph: "◈", color: "#cc7c5e", text: "5小时 59%" },
      undefined,
    );
    // #cc7c5e is Claude's branded colour in the registry.
    expect(cell.providerId).toBe("claude");
    expect(cell.icon?.brandColor).toBe("#cc7c5e");
  });

  it("falls back to the full raw text when the value split fails", () => {
    const cell = normalizeLegacyLine(
      { glyph: null, color: null, text: "部署运行正常" },
      undefined,
    );
    expect(cell.tag).toBe("");
    expect(cell.value).toBe("部署运行正常");
  });
});
