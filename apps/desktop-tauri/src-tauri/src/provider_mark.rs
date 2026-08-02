//! One-glyph brand marks for the taskbar strip.
//!
//! The strip has room for roughly one 12px cell per reading, and a provider's
//! NAME is the longest part of it — "Claude 5小时 12%" measures 103px against
//! 77px for a mark plus the same numbers. So the strip prints the mark.
//!
//! # Why a glyph and not the brand SVG
//!
//! Every provider ships a real brand SVG, and drawing one would mean an SVG
//! rasteriser (a large new dependency) plus a Direct2D bitmap path. It would
//! also not look better: the marks are 100x100 artwork and the strip is two
//! ~14px rows, where most of them reduce to a coloured blob. A single glyph in
//! the brand colour is drawn by the DirectWrite renderer that is already there,
//! stays crisp at any size, and is the same character the rest of the app
//! already uses when a provider has no SVG.
//!
//! Shapes repeat across providers on purpose — `◈` Claude and `◆` Codex are
//! near-identical outlines. The COLOUR separates them (warm brown vs teal), and
//! at this size colour is what the eye reads first anyway.
//!
//! Generated from `providerIcons.ts`, which is the registry the rest of the UI
//! uses. `scripts/check-provider-marks.mjs` fails the build if the two drift.

/// A provider's mark: the glyph to draw and the brand colour to draw it in.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct ProviderMark {
    pub glyph: char,
    /// 0x00RRGGBB, matching the convention the rest of the strip uses.
    pub color_rgb: u32,
}

/// Every provider the UI knows a mark for, sorted by id.
const MARKS: &[(&str, char, u32)] = &[
    ("abacus", 'A', 0x7c3aed),
    ("alibaba", '阿', 0xff6a00),
    ("alibabatokenplan", '阿', 0xff6a00),
    ("amp", '⚡', 0xdc2626),
    ("antigravity", '◉', 0x60ba7e),
    ("arkagentplan", 'A', 0x1f6fff),
    ("arkcodingplan", 'A', 0x1f6fff),
    ("augment", 'A', 0x6366f1),
    ("azureopenai", 'A', 0x0078d4),
    ("bedrock", 'B', 0xff9900),
    ("chutes", 'C', 0xff5c35),
    ("claude", '◈', 0xcc7c5e),
    ("codebuff", 'B', 0x44ff00),
    ("codex", '◆', 0x49a3b0),
    ("commandcode", 'C', 0x44ff00),
    ("copilot", '⬡', 0xa855f7),
    ("crof", 'C', 0x7c3aed),
    ("crossmodel", 'X', 0xc084fc),
    ("cursor", '▸', 0x00bfa5),
    ("deepgram", 'D', 0x13ef93),
    ("deepseek", 'D', 0x527df0),
    ("devin", 'D', 0x111827),
    ("doubao", 'D', 0x2563eb),
    ("elevenlabs", 'E', 0x111827),
    ("factory", '◎', 0xff6b35),
    ("gemini", '✦', 0xab87ea),
    ("grok", 'G', 0x111827),
    ("groq", 'G', 0xf55036),
    ("infini", 'I', 0x687fa1),
    ("jetbrains", 'J', 0xff3399),
    ("kilo", 'K', 0x5d87ff),
    ("kimi", '☽', 0xfe603c),
    ("kimik2", '☽', 0x4c00ff),
    ("kiro", 'K', 0xff9900),
    ("litellm", 'L', 0x0ea5e9),
    ("llmproxy", 'L', 0x4f46e5),
    ("manus", 'M', 0x34322d),
    ("mimo", 'M', 0xff6900),
    ("mimoapi", 'M', 0x2563eb),
    ("minimax", 'M', 0xfe603c),
    ("mistral", 'M', 0xff500f),
    ("nanogpt", 'N', 0x687fa1),
    ("ollama", '○', 0x8b95b0),
    ("openaiapi", 'O', 0x10a37f),
    ("opencode", '○', 0x3b82f6),
    ("opencodego", '○', 0x3b82f6),
    ("openrouter", 'R', 0x6b7280),
    ("perplexity", 'P', 0x1fb8cd),
    ("poe", 'P', 0x5d5fef),
    ("qoder", 'Q', 0x2563eb),
    ("sakana", 'S', 0x0ea5e9),
    ("stepfun", 'S', 0x999999),
    ("sub2api", 'S', 0x7c3aed),
    ("t3chat", 'T', 0x8b5cf6),
    ("venice", 'V', 0x111827),
    ("vertexai", '△', 0x4285f4),
    ("warp", 'W', 0x6366f1),
    ("wayfinder", 'W', 0x2563eb),
    ("windsurf", 'W', 0x22c55e),
    ("zai", 'Z', 0xe85a6a),
    ("zed", 'Z', 0x084ccf),
];

/// The mark for a provider id, or `None` when the id is unknown.
///
/// Unknown returns `None` rather than a placeholder glyph: the strip falls back
/// to the provider's name, which is still readable, instead of printing a mark
/// that means nothing.
pub fn provider_mark(provider_id: &str) -> Option<ProviderMark> {
    MARKS
        .binary_search_by(|(id, _, _)| (*id).cmp(provider_id))
        .ok()
        .map(|index| {
            let (_, glyph, color_rgb) = MARKS[index];
            ProviderMark { glyph, color_rgb }
        })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// `provider_mark` binary-searches, so an unsorted table silently misses.
    #[test]
    fn the_table_is_sorted_and_unique() {
        for pair in MARKS.windows(2) {
            assert!(pair[0].0 < pair[1].0, "{} then {}", pair[0].0, pair[1].0);
        }
    }

    #[test]
    fn every_entry_is_reachable() {
        for (id, glyph, color) in MARKS {
            assert_eq!(
                provider_mark(id),
                Some(ProviderMark { glyph: *glyph, color_rgb: *color }),
                "{id}"
            );
        }
    }

    #[test]
    fn the_providers_the_strip_shows_have_distinguishable_colours() {
        let codex = provider_mark("codex").expect("codex");
        let claude = provider_mark("claude").expect("claude");
        let grok = provider_mark("grok").expect("grok");
        // The glyphs may repeat; the colours carry the identity at 14px.
        assert_ne!(codex.color_rgb, claude.color_rgb);
        assert_ne!(codex.color_rgb, grok.color_rgb);
        assert_ne!(claude.color_rgb, grok.color_rgb);
    }

    #[test]
    fn an_unknown_provider_has_no_mark() {
        assert_eq!(provider_mark("not-a-provider"), None);
        assert_eq!(provider_mark(""), None);
    }
}
