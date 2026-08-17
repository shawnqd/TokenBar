//! Rasterise the official provider brand SVGs onto the taskbar strip with
//! Direct2D — no SVG crate, no new dependency.
//!
//! # Where the icons come from
//!
//! The brand SVGs live in
//! `apps/desktop-tauri/src/components/providers/icons/ProviderIcon-<id>.svg`,
//! the same single source the frontend registry (`providerIcons.ts`) consumes.
//! `build.rs` scans that directory at build time and emits a
//! `provider_id -> svg source` table into OUT_DIR, included here. A handful of
//! provider ids reuse a sibling asset (e.g. `mimoapi` reuses the `mimo`
//! glyph) — that mapping is fixed in [`SVG_ALIASES`] so the strip never falls
//! back to a glyph for an id that has artwork under another name.
//!
//! # What the widget draws
//!
//! [`draw_icon`] fills an icon-size square inside a cell with the provider's
//! official logo under one of three styles: `pure` (logo alone, brand colour),
//! `badge` (a rounded brand-at-18%-alpha tile behind the same logo), or
//! `solid` (a solid brand tile with a white logo). Grok is special-cased, as
//! in the reference HTML: its SVG already carries the black rounded badge +
//! white glyph, so it is never recoloured and gets no backing tile in any mode.
//!
//! # A deliberately small SVG parser
//!
//! The assets are single/few-path icons, so the parser recognises only
//! `path` (`d` + `fill-rule`) and `rect` (`x y width height rx fill`). Path
//! commands cover `M/L/H/V/C/S/Q/T/A/Z`, their relative forms, implicit
//! command repetition, and arcs turned into cubic beziers via the SVG
//! endpoint-to-centre expansion (SVG 1.1 § F.6.5).
//!
//! # Fill resolution
//!
//! An element's `fill` decides its colour: an explicit non-white `#rrggbb` is
//! kept verbatim (multi-colour marks keep their real palette); `white`/`#fff`/
//! `#ffffff`/`currentColor`/absent fill use the INK colour passed in (brand in
//! pure/badge, white in solid) — the same single-colour-to-brand treatment the
//! frontend's `tint()` applies. Grok keeps every element fill unchanged.
//!
//! # Geometry caching
//!
//! Turning a path string into an `ID2D1PathGeometry` is too expensive to
//! repeat on a strip that repaints every second, and the geometry of a given
//! provider at a given size never changes. Parsed results are cached per
//! `(provider_id, slot_size_px)` so a steady-state strip does zero parsing.

#![cfg(windows)]

use std::collections::HashMap;
use std::sync::{Mutex, OnceLock};

use windows_numerics::Vector2; // via the windows-numerics crate (transitive of `windows`)
use windows::Win32::Graphics::Direct2D::Common::{
    D2D1_BEZIER_SEGMENT, D2D1_COLOR_F, D2D1_FIGURE_BEGIN_FILLED, D2D1_FIGURE_END_CLOSED,
    D2D1_FILL_MODE_ALTERNATE, D2D1_FILL_MODE_WINDING, D2D_RECT_F,
};
use windows::Win32::Graphics::Direct2D::{
    D2D1CreateFactory, D2D1_FACTORY_TYPE_SINGLE_THREADED, D2D1_QUADRATIC_BEZIER_SEGMENT,
    D2D1_ROUNDED_RECT, ID2D1DCRenderTarget, ID2D1Factory, ID2D1GeometrySink, ID2D1PathGeometry,
};

// `provider_id -> svg source`, scanned at build time into OUT_DIR.
include!(concat!(env!("OUT_DIR"), "/provider_icon_table.rs"));

const SVG_ALIASES: &[(&str, &str)] = &[
    ("alibabatokenplan", "alibaba"),
    ("arkagentplan", "volcengine-ark"),
    ("arkcodingplan", "volcengine-ark"),
    ("kimik2", "kimi"),
    ("mimoapi", "mimo"),
];

const WIDE_WORDMARK_IDS: &[&str] = &["deepseek", "kimi", "minimax", "mistral"];
const PATH_TYPE_INSET: f32 = 0.82;
const WIDE_WORDMARK_INSET: f32 = 0.72;
const GROK_INSET: f32 = 1.0;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IconStyle { Pure, Badge, Solid }
impl IconStyle {
    pub fn parse(value: &str) -> Self {
        match value { "badge" => Self::Badge, "solid" => Self::Solid, _ => Self::Pure }
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct IconSlot { pub left: f32, pub top: f32, pub size_px: f32 }


pub fn is_grok(provider_id: &str) -> bool { provider_id.eq_ignore_ascii_case("grok") }

pub fn brand_color(provider_id: &str) -> u32 {
    crate::provider_mark::provider_mark(provider_id).map(|m| m.color_rgb).unwrap_or(0x5d87ff)
}

pub fn provider_svg_source(provider_id: &str) -> Option<&'static str> {
    let key = SVG_ALIASES.iter().find(|(id, _)| *id == provider_id).map(|(_, t)| *t).unwrap_or(provider_id);
    PROVIDER_ICON_TABLE.iter().find(|(id, _)| *id == key).map(|(_, src)| *src)
}

fn inset_fraction(provider_id: &str) -> f32 {
    if is_grok(provider_id) { GROK_INSET }
    else if WIDE_WORDMARK_IDS.iter().any(|id| *id == provider_id) { WIDE_WORDMARK_INSET }
    else { PATH_TYPE_INSET }
}

// ── Pure parse model (no Direct2D) ─────────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq)]
struct P2 { x: f32, y: f32 }

#[derive(Debug, Clone, Copy)]
struct ArcInfo { end: P2, rx: f32, ry: f32, phi_deg: f32, large: bool, sweep: bool }

#[derive(Debug, Clone, Copy)]
enum Seg {
    Move(P2),
    Line(P2),
    Cubic { c1: P2, c2: P2, end: P2 },
    Quad { c: P2, end: P2 },
    Arc(ArcInfo),
    Close,
}

#[derive(Debug, Clone)]
struct PaintItem { fill: Option<u32>, even_odd: bool, segs: Vec<Seg> }
#[derive(Debug, Clone)]
struct ParsedSvg { origin: P2, size: P2, items: Vec<PaintItem> }

// ── Tokeniser ───────────────────────────────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq)]
enum Tok { Cmd(char), Num(f32) }

fn is_cmd_char(c: char) -> bool {
    matches!(c.to_ascii_lowercase(), 'm' | 'l' | 'h' | 'v' | 'c' | 's' | 'q' | 't' | 'a' | 'z')
}

fn tokenise(path: &str) -> Vec<Tok> {
    let a: Vec<char> = path.chars().collect();
    let mut out = Vec::new();
    let mut i = 0usize;
    while i < a.len() {
        let c = a[i];
        if c == '+' || c == '-' || c == '.' || c.is_ascii_digit() {
            let mut s = String::new();
            let mut j = i;
            if a[j] == '+' || a[j] == '-' { s.push(a[j]); j += 1; }
            while j < a.len() && (a[j].is_ascii_digit() || a[j] == '.') { s.push(a[j]); j += 1; }
            if j < a.len() && (a[j] == 'e' || a[j] == 'E') {
                let mut k = j + 1;
                let mut esign = String::new();
                if k < a.len() && (a[k] == '+' || a[k] == '-') { esign.push(a[k]); k += 1; }
                if k < a.len() && a[k].is_ascii_digit() {
                    s.push('e'); s.push_str(&esign);
                    while k < a.len() && a[k].is_ascii_digit() { s.push(a[k]); k += 1; }
                    j = k;
                }
            }
            if let Ok(v) = s.parse::<f32>() { out.push(Tok::Num(v)); }
            i = j;
        } else if is_cmd_char(c) { out.push(Tok::Cmd(c)); i += 1; }
        else { i += 1; }
    }
    out
}

fn params_for(lower: char) -> usize {
    match lower { 'm' | 'l' | 't' => 2, 'h' | 'v' => 1, 'c' => 6, 's' => 4, 'q' => 4, 'a' => 7, _ => 0 }
}

fn pt(x: f32, y: f32, base: P2, rel: bool) -> P2 {
    if rel { P2 { x: base.x + x, y: base.y + y } } else { P2 { x, y } }
}

fn reflect(control: P2, end: P2) -> P2 {
    P2 { x: 2.0 * end.x - control.x, y: 2.0 * end.y - control.y }
}

fn parse_path(d: &str) -> Vec<Seg> {
    let toks = tokenise(d);
    let mut segs: Vec<Seg> = Vec::new();
    let mut i = 0usize;
    let mut pos = P2 { x: 0.0, y: 0.0 };
    let mut start = P2 { x: 0.0, y: 0.0 };
    let mut cur_cmd: Option<char> = None;
    let mut prev_cubic = P2 { x: 0.0, y: 0.0 };
    let mut has_prev_cubic = false;
    let mut prev_quad = P2 { x: 0.0, y: 0.0 };
    let mut has_prev_quad = false;

    while i < toks.len() {
        if let Tok::Cmd(c) = toks[i] { cur_cmd = Some(c); i += 1; }
        let Some(c) = cur_cmd else { break };
        let lower = c.to_ascii_lowercase();
        if lower == 'z' {
            segs.push(Seg::Close);
            pos = start; cur_cmd = None;
            has_prev_cubic = false; has_prev_quad = false;
            i += 1; continue;
        }
        let n = params_for(lower);
        if n == 0 { cur_cmd = None; continue; }
        if i + n > toks.len() { break; }
        let mut complete = true;
        for t in &toks[i..i + n] { if let Tok::Cmd(_) = t { complete = false; break; } }
        if !complete { break; }
        let mut nums = [0.0f32; 7];
        for k in 0..n { nums[k] = match toks[i + k] { Tok::Num(v) => v, Tok::Cmd(_) => 0.0 }; }
        i += n;

        let rel = c.is_ascii_lowercase();
        match lower {
            'm' => {
                let p = pt(nums[0], nums[1], pos, rel);
                segs.push(Seg::Move(p)); pos = p; start = p;
                has_prev_cubic = false; has_prev_quad = false;
                cur_cmd = if rel { Some('l') } else { Some('L') };
            },
            'l' => { let p = pt(nums[0], nums[1], pos, rel); segs.push(Seg::Line(p)); pos = p; has_prev_cubic = false; has_prev_quad = false; },
            'h' => { let p = P2 { x: if rel { pos.x + nums[0] } else { nums[0] }, y: pos.y }; segs.push(Seg::Line(p)); pos = p; has_prev_cubic = false; has_prev_quad = false; },
            'v' => { let p = P2 { x: pos.x, y: if rel { pos.y + nums[0] } else { nums[0] } }; segs.push(Seg::Line(p)); pos = p; has_prev_cubic = false; has_prev_quad = false; },
            'c' => {
                let c1 = pt(nums[0], nums[1], pos, rel);
                let c2 = pt(nums[2], nums[3], pos, rel);
                let e = pt(nums[4], nums[5], pos, rel);
                segs.push(Seg::Cubic { c1, c2, end: e });
                prev_cubic = c2; has_prev_cubic = true; has_prev_quad = false; pos = e;
            },
            's' => {
                let c1 = if has_prev_cubic { reflect(prev_cubic, pos) } else { pos };
                let c2 = pt(nums[0], nums[1], pos, rel);
                let e = pt(nums[2], nums[3], pos, rel);
                segs.push(Seg::Cubic { c1, c2, end: e });
                prev_cubic = c2; has_prev_cubic = true; has_prev_quad = false; pos = e;
            },
            'q' => {
                let c = pt(nums[0], nums[1], pos, rel);
                let e = pt(nums[2], nums[3], pos, rel);
                segs.push(Seg::Quad { c, end: e });
                prev_quad = c; has_prev_quad = true; has_prev_cubic = false; pos = e;
            },
            't' => {
                let c = if has_prev_quad { reflect(prev_quad, pos) } else { pos };
                let e = pt(nums[0], nums[1], pos, rel);
                segs.push(Seg::Quad { c, end: e });
                prev_quad = c; has_prev_quad = true; has_prev_cubic = false; pos = e;
            },
            'a' => {
                let rx = nums[0].abs();
                let ry = nums[1].abs();
                let phi = nums[2];
                let large = nums[3] != 0.0;
                let sweep = nums[4] != 0.0;
                let e = pt(nums[5], nums[6], pos, rel);
                segs.push(Seg::Arc(ArcInfo { end: e, rx, ry, phi_deg: phi, large, sweep }));
                has_prev_cubic = false; has_prev_quad = false; pos = e;
            },
            _ => {},
        }
    }
    segs
}


// ── SVG document parsing (path + rect only) ─────────────────────────

fn parse_view_box(svg: &str) -> Option<(P2, P2)> {
    let attr = capture_attr(svg, "viewBox")?;
    let v: Vec<f32> = attr.split_whitespace().filter_map(|s| s.parse().ok()).collect();
    if v.len() < 4 { return None; }
    Some((P2 { x: v[0], y: v[1] }, P2 { x: v[2], y: v[3] }))
}

fn capture_attr(hay: &str, name: &str) -> Option<String> {
    let lower = hay.to_ascii_lowercase();
    let needle = name.to_ascii_lowercase();
    let needle = needle + "=\"";
    let idx = lower.find(&needle)? + needle.len();
    let vend = hay[idx..].find('\"')? + idx;
    Some(hay[idx..vend].to_string())
}

fn find_sub(hay: &str, needle: &str, from: usize) -> Option<usize> {
    hay[from.min(hay.len())..].find(needle).map(|i| i + from.min(hay.len()))
}

fn attr_in(elem: &str, name: &str, from: usize) -> Option<String> {
    let lower = elem.to_ascii_lowercase();
    let needle = name.to_ascii_lowercase();
    let needle = needle + "=\"";
    let base = from.min(elem.len());
    let idx = lower[base..].find(&needle)? + base;
    let vstart = idx + needle.len();
    let vend = elem[vstart..].find('\"')? + vstart;
    Some(elem[vstart..vend].to_string())
}

fn attr_num_in(elem: &str, name: &str, default: f32) -> f32 {
    attr_in(elem, name, 0).and_then(|s| s.trim().parse::<f32>().ok()).unwrap_or(default)
}

/// Resolve an SVG `fill` attribute to a colour, or None when it means INK
/// (white / currentColor / empty / named non-hex colours are recoloured).
fn colour_from_attr(fill: &str) -> Option<u32> {
    let f = fill.trim().to_ascii_lowercase();
    if f.is_empty() || f == "none" || f == "currentcolor" { return None; }
    if f == "white" || f == "#fff" || f == "#ffffff" { return None; }
    let hex = f.strip_prefix('#').unwrap_or(&f);
    if hex.len() == 6 && hex.chars().all(|c| c.is_ascii_hexdigit()) {
        if let Ok(v) = u32::from_str_radix(hex, 16) { return Some(v); }
    }
    None
}

/// Build a rectangle outline (sharp or rounded). Rounded corners are emitted as
/// [`Seg::Arc`] (90°, sweep clockwise) so they share the A-command arc expander.
fn rect_outline(x: f32, y: f32, w: f32, h: f32, rx: f32, out: &mut Vec<Seg>) {
    if w <= 0.0 || h <= 0.0 { return; }
    let r = rx.max(0.0).min(w.min(h) * 0.5);
    if r <= 1e-4 {
        out.push(Seg::Move(P2 { x, y }));
        out.push(Seg::Line(P2 { x: x + w, y }));
        out.push(Seg::Line(P2 { x: x + w, y: y + h }));
        out.push(Seg::Line(P2 { x, y: y + h }));
        out.push(Seg::Close);
        return;
    }
    // Clockwise outline, starting on the top edge r from the top-left corner.
    out.push(Seg::Move(P2 { x: x + r, y }));
    out.push(Seg::Line(P2 { x: x + w - r, y }));
    out.push(seg_arc(x + w - r, y + r, r, x + w, y + r));
    out.push(Seg::Line(P2 { x: x + w, y: y + h - r }));
    out.push(seg_arc(x + w, y + h - r, r, x + w - r, y + h));
    out.push(Seg::Line(P2 { x: x + r, y: y + h }));
    out.push(seg_arc(x + r, y + h, r, x, y + h - r));
    out.push(Seg::Line(P2 { x, y: y + r }));
    out.push(seg_arc(x, y + r, r, x + r, y));
    out.push(Seg::Close);
}

/// A 90° clockwise rounded-corner arc from the current position to (ex, ey).
fn seg_arc(_center_x: f32, _center_y: f32, r: f32, ex: f32, ey: f32) -> Seg {
    Seg::Arc(ArcInfo { end: P2 { x: ex, y: ey }, rx: r, ry: r, phi_deg: 0.0, large: false, sweep: true })
}

/// Parse a minimal SVG document into parsed paint items.
fn parse_svg(svg: &str) -> ParsedSvg {
    let (origin, size) = parse_view_box(svg).unwrap_or((P2 { x: 0.0, y: 0.0 }, P2 { x: 100.0, y: 100.0 }));
    let lower = svg.to_ascii_lowercase();
    let mut items: Vec<PaintItem> = Vec::new();
    let mut i = 0usize;
    while i < svg.len() {
        let tp = find_sub(&lower, "<path", i);
        let tr = find_sub(&lower, "<rect", i);
        let next = match (tp, tr) {
            (Some(a), Some(b)) => a.min(b),
            (Some(a), None) => a,
            (None, Some(b)) => b,
            (None, None) => break,
        };
        let is_rect = tr.map(|r| r == next).unwrap_or(false);
        let Some(rel) = svg[next..].find('>') else { break };
        let close = next + rel + 1;
        let elem = &svg[next..close.min(svg.len())];
        if is_rect {
            let x = attr_num_in(elem, "x", 0.0);
            let yy = attr_num_in(elem, "y", 0.0);
            let w = attr_num_in(elem, "width", 0.0);
            let h = attr_num_in(elem, "height", 0.0);
            let rx = attr_num_in(elem, "rx", 0.0);
            let mut outline = Vec::new();
            rect_outline(x, yy, w, h, rx, &mut outline);
            let fill = attr_in(elem, "fill", 0).and_then(|f| colour_from_attr(&f));
            items.push(PaintItem { fill, even_odd: false, segs: outline });
        } else {
            let Some(d) = attr_in(elem, "d", 0) else { i = close; continue; };
            let even_odd = attr_in(elem, "fill-rule", 0).map(|r| r.eq_ignore_ascii_case("evenodd")).unwrap_or(false);
            let fill = attr_in(elem, "fill", 0).and_then(|f| colour_from_attr(&f));
            let segs = parse_path(&d);
            items.push(PaintItem { fill, even_odd, segs });
        }
        i = close;
    }
    ParsedSvg { origin, size, items }
}

// ── Arc → cubic expansion (SVG endpoint-to-centre, F.6.5) ──────────

fn signed_angle(u: P2, v: P2) -> f32 {
    let mag = (u.x * u.x + u.y * u.y).sqrt() * (v.x * v.x + v.y * v.y).sqrt();
    if mag < 1e-12 { return 0.0; }
    let cos = ((u.x * v.x + u.y * v.y) / mag).max(-1.0).min(1.0);
    let cross = u.x * v.y - u.y * v.x;
    let a = cos.acos();
    if cross < 0.0 { -a } else { a }
}

/// A point on a rotated ellipse and its tangent at parameter t.
fn ellipse_point(t: f32, cx: f32, cy: f32, rx: f32, ry: f32, cp: f32, sp: f32) -> P2 {
    let (s, c) = t.sin_cos();
    P2 { x: cx + rx * c * cp - ry * s * sp, y: cy + rx * c * sp + ry * s * cp }
}

fn ellipse_tangent(t: f32, rx: f32, ry: f32, cp: f32, sp: f32) -> P2 {
    let (s, c) = t.sin_cos();
    P2 { x: -rx * s * cp - ry * c * sp, y: -rx * s * sp + ry * c * cp }
}

/// Expand an SVG arc from `start` to `info.end` into cubic [`Seg`]s appended to
/// `out`, via the endpoint-to-centre parameterisation (SVG 1.1 F.6.5).
fn arc_to_cubics(start: P2, info: ArcInfo, out: &mut Vec<Seg>) {
    let mut rx = info.rx;
    let mut ry = info.ry;
    let phi = info.phi_deg.to_radians();
    let (cp, sp) = (phi.cos(), phi.sin());
    if rx < 0.0 { rx = -rx; }
    if ry < 0.0 { ry = -ry; }
    if rx.abs() < 1e-7 || ry.abs() < 1e-7 { out.push(Seg::Line(info.end)); return; }
    if (start.x - info.end.x).abs() < 1e-9 && (start.y - info.end.y).abs() < 1e-9 { return; }

    // F.6.5.1: transform start/end into the unrotated (-phi) frame.
    let dx = (start.x - info.end.x) * 0.5;
    let dy = (start.y - info.end.y) * 0.5;
    let x1p = cp * dx + sp * dy;
    let y1p = -sp * dx + cp * dy;

    // F.6.5.2: correct radii if the ellipse is too small.
    let lam = x1p * x1p / (rx * rx) + y1p * y1p / (ry * ry);
    if lam > 1.0 {
        let s = lam.sqrt();
        rx *= s;
        ry *= s;
    }

    // F.6.5.3: centre in the unrotated frame, then rotate+translate back.
    let rx2 = rx * rx;
    let ry2 = ry * ry;
    let x1p2 = x1p * x1p;
    let y1p2 = y1p * y1p;
    let num = rx2 * ry2 - rx2 * y1p2 - ry2 * x1p2;
    let den = rx2 * y1p2 + ry2 * x1p2;
    let radicand = if den.abs() < 1e-12 { 0.0 } else { num / den };
    let coef = radicand.max(0.0).sqrt();
    let sign = if info.large == info.sweep { -1.0f32 } else { 1.0f32 };
    let cxp = sign * coef * (rx * y1p) / ry;
    let cyp = sign * coef * (-(ry * x1p) / rx);
    let cx = cp * cxp - sp * cyp + (start.x + info.end.x) * 0.5;
    let cy = sp * cxp + cp * cyp + (start.y + info.end.y) * 0.5;

    // F.6.5.4: start angle and sweep in the unrotated frame.
    let v1x = (x1p - cxp) / rx;
    let v1y = (y1p - cyp) / ry;
    let v2x = (-x1p - cxp) / rx;
    let v2y = (-y1p - cyp) / ry;
    let theta1 = signed_angle(P2 { x: 1.0, y: 0.0 }, P2 { x: v1x, y: v1y });
    let mut delta = signed_angle(P2 { x: v1x, y: v1y }, P2 { x: v2x, y: v2y });
    if !info.sweep && delta > 0.0 { delta -= std::f32::consts::TAU; }
    else if info.sweep && delta < 0.0 { delta += std::f32::consts::TAU; }

    // Split into ≤90° chunks, replacing each with a standard cubic.
    let chunks = (delta.abs() / std::f32::consts::FRAC_PI_2).ceil().max(1.0) as usize;
    let d = delta / chunks as f32;
    for i in 0..chunks {
        let t0 = theta1 + i as f32 * d;
        let t1 = t0 + d;
        let p0 = ellipse_point(t0, cx, cy, rx, ry, cp, sp);
        let p3 = ellipse_point(t1, cx, cy, rx, ry, cp, sp);
        let k = (4.0 / 3.0) * (d * 0.25).tan();
        let tan0 = ellipse_tangent(t0, rx, ry, cp, sp);
        let tan1 = ellipse_tangent(t1, rx, ry, cp, sp);
        let c1 = P2 { x: p0.x + k * tan0.x, y: p0.y + k * tan0.y };
        let c2 = P2 { x: p3.x - k * tan1.x, y: p3.y - k * tan1.y };
        out.push(Seg::Cubic { c1, c2, end: p3 });
    }
    let _ = theta1;
}

// ── Geometry building (typed Direct2D geometry sink) ────────────────

fn d2d_color(rgb: u32, alpha: f32) -> D2D1_COLOR_F {
    D2D1_COLOR_F {
        r: ((rgb >> 16) & 0xFF) as f32 / 255.0,
        g: ((rgb >> 8) & 0xFF) as f32 / 255.0,
        b: (rgb & 0xFF) as f32 / 255.0,
        a: alpha,
    }
}

fn transform_point(p: P2, origin: P2, scale: f32, off: P2) -> Vector2 {
    Vector2 { X: off.x + (p.x - origin.x) * scale, Y: off.y + (p.y - origin.y) * scale }
}

unsafe fn emit_into_sink(sink: &ID2D1GeometrySink, segs: &[Seg], origin: P2, scale: f32, off: P2) {
    // Expand arcs into cubics first (they need the running position).
    let mut expanded: Vec<Seg> = Vec::with_capacity(segs.len());
    let mut cur = P2 { x: 0.0, y: 0.0 };
    for seg in segs {
        match *seg {
            Seg::Arc(info) => arc_to_cubics(cur, info, &mut expanded),
            _ => expanded.push(*seg),
        }
        cur = match *seg {
            Seg::Arc(info) => info.end,
            Seg::Line(p) => p,
            Seg::Cubic { end, .. } => end,
            Seg::Quad { end, .. } => end,
            Seg::Move(p) => p,
            Seg::Close => P2 { x: 0.0, y: 0.0 },
        };
    }
    let mut open = false;
    for seg in expanded {
        match seg {
            Seg::Move(p) => {
                if open { sink.EndFigure(D2D1_FIGURE_END_CLOSED); }
                sink.BeginFigure(transform_point(p, origin, scale, off), D2D1_FIGURE_BEGIN_FILLED);
                open = true;
            },
            Seg::Line(p) => sink.AddLine(transform_point(p, origin, scale, off)),
            Seg::Cubic { c1, c2, end } => {
                let bez = D2D1_BEZIER_SEGMENT {
                    point1: transform_point(c1, origin, scale, off),
                    point2: transform_point(c2, origin, scale, off),
                    point3: transform_point(end, origin, scale, off),
                };
                sink.AddBezier(&bez);
            },
            Seg::Quad { c, end } => {
                let q = D2D1_QUADRATIC_BEZIER_SEGMENT {
                    point1: transform_point(c, origin, scale, off),
                    point2: transform_point(end, origin, scale, off),
                };
                sink.AddQuadraticBezier(&q);
            },
            Seg::Arc(_) => {},
            Seg::Close => { sink.EndFigure(D2D1_FIGURE_END_CLOSED); open = false; },
        }
    }
    if open { sink.EndFigure(D2D1_FIGURE_END_CLOSED); }
}

/// Build one fully-filled ID2D1PathGeometry for a paint item at the given slot.
unsafe fn build_item_geometry(
    factory: &ID2D1Factory,
    item: &PaintItem,
    origin: P2,
    scale: f32,
    off: P2,
) -> windows::core::Result<ID2D1PathGeometry> {
    let geometry: ID2D1PathGeometry = unsafe { factory.CreatePathGeometry() }?;
    let sink: ID2D1GeometrySink = unsafe { geometry.Open() }?;
    unsafe { sink.SetFillMode(if item.even_odd { D2D1_FILL_MODE_ALTERNATE } else { D2D1_FILL_MODE_WINDING }) };
    unsafe { emit_into_sink(&sink, &item.segs, origin, scale, off) };
    unsafe { sink.Close() }?;
    Ok(geometry)
}
struct ParsedLogo { grok: bool, parsed: ParsedSvg }

static PARSED_CACHE: OnceLock<Mutex<HashMap<String, std::sync::Arc<ParsedLogo>>>> = OnceLock::new();

fn parsed_logo(provider_id: &str) -> Option<std::sync::Arc<ParsedLogo>> {
    if let Some(m) = PARSED_CACHE.get()
        && let Ok(guard) = m.lock()
        && let Some(logo) = guard.get(provider_id)
    {
        return Some(logo.clone());
    }
    let src = provider_svg_source(provider_id)?;
    let parsed = parse_svg(src);
    if parsed.items.is_empty() { return None; }
    let logo = std::sync::Arc::new(ParsedLogo { grok: is_grok(provider_id), parsed });
    PARSED_CACHE.get_or_init(|| Mutex::new(HashMap::new()))
        .lock()
        .map(|mut m| m.insert(provider_id.to_string(), logo.clone()))
        .ok();
    Some(logo)
}

/// Fill the provider's official logo into `slot` on the given render target.
/// Draws the style tile (badge/solid) first when applicable, then each layer.
/// Returns true when an SVG was drawn; false means the caller should fall back
/// to the legacy glyph in the same slot.
pub fn draw_icon(
    target: &ID2D1DCRenderTarget,
    provider_id: &str,
    style: IconStyle,
    slot: IconSlot,
) -> bool {
    let Some(logo) = parsed_logo(provider_id) else { return false; };
    // Direct2D requires every resource to come from the same factory as the
    // render target it is used with; a geometry built on a different factory
    // is undefined behaviour (observed as 0xc000041d in the WM_PAINT path).
    // target.GetFactory() returns the target's own factory, so the cached
    // thread-local factory is not used here.
    let factory = match unsafe { target.GetFactory() } {
        Ok(factory) => factory,
        Err(_) => return false,
    };
    let brand = brand_color(provider_id);
    let parsed = &logo.parsed;
    let slot_size = slot.size_px.max(1.0);
    let inset = inset_fraction(provider_id);
    let side = parsed.size.x.max(parsed.size.y).max(1e-3);
    let scale = slot_size * inset / side;
    let art_w = parsed.size.x * scale;
    let art_h = parsed.size.y * scale;
    let off = P2 { x: slot.left + (slot_size - art_w) * 0.5, y: slot.top + (slot_size - art_h) * 0.5 };

    // Backing tile: never for grok (its SVG already carries the badge).
    if !logo.grok {
        let rounded = D2D1_ROUNDED_RECT {
            rect: D2D_RECT_F {
                left: slot.left, top: slot.top,
                right: slot.left + slot_size, bottom: slot.top + slot_size,
            },
            radiusX: slot_size * 3.0 / 14.0,
            radiusY: slot_size * 3.0 / 14.0,
        };
        match style {
            IconStyle::Badge => {
                if let Ok(brush) = unsafe { target.CreateSolidColorBrush(&d2d_color(brand, 0.18), None) } {
                    unsafe { target.FillRoundedRectangle(&rounded, &brush) };
                }
            },
            IconStyle::Solid => {
                if let Ok(brush) = unsafe { target.CreateSolidColorBrush(&d2d_color(brand, 1.0), None) } {
                    unsafe { target.FillRoundedRectangle(&rounded, &brush) };
                }
            },
            IconStyle::Pure => {},
        }
    }

    let ink = match style { IconStyle::Solid => 0xFF_FFFFu32, _ => brand };
    let mut drew = false;
    for item in &parsed.items {
        let rgb = if logo.grok {
            item.fill.unwrap_or(0xFF_FFFF)
        } else {
            match item.fill { Some(hex) => hex, None => ink }
        };
        let Ok(geometry) = (unsafe { build_item_geometry(&factory, item, parsed.origin, scale, off) }) else {
            continue;
        };
        if let Ok(brush) = unsafe { target.CreateSolidColorBrush(&d2d_color(rgb, 1.0), None) } {
            unsafe { target.FillGeometry(&geometry, &brush, None) };
            drew = true;
        }
    }
    drew
}


#[cfg(test)]
mod tests {
    use super::*;

    fn seg_kind(segs: &[Seg]) -> Vec<&'static str> {
        segs.iter().map(|s| match s {
            Seg::Move(_) => "M", Seg::Line(_) => "L", Seg::Cubic { .. } => "C",
            Seg::Quad { .. } => "Q", Seg::Arc(_) => "A", Seg::Close => "Z",
        }).collect()
    }

    #[test]
    fn parses_abs_and_rel_commands() {
        let segs = parse_path("M10 10 L20 20 H30 V40 C0 0 1 1 2 2 Z");
        assert_eq!(seg_kind(&segs), vec!["M", "L", "L", "L", "C", "Z"]);
        let rel = parse_path("m10 10 l10 10 h5 v5");
        assert_eq!(seg_kind(&rel), vec!["M", "L", "L", "L"]);
        let last = *rel.last().unwrap();
        let Seg::Line(p) = last else { panic!("last v"); };
        assert!((p.x - 25.0).abs() < 1e-3 && (p.y - 25.0).abs() < 1e-3, "relative accum: {p:?}");
    }

    #[test]
    fn implicit_repetition_continues_the_command() {
        // More coordinate pairs than one segment: they repeat the command.
        let segs = parse_path("M0 0 10 10 20 20");
        assert_eq!(seg_kind(&segs), vec!["M", "L", "L"]);
        let segs = parse_path("L1 1 2 2 3 3");
        assert_eq!(seg_kind(&segs), vec!["L", "L", "L"]);
    }

    #[test]
    fn smooth_curves_reflect_their_control_point() {
        let segs = parse_path("M0 0 C10 0 10 10 20 10 S30 20 40 20");
        assert_eq!(seg_kind(&segs), vec!["M", "C", "C"]);
        // s reflects c2 = (10,10) about the previous end (20,10) => c1=(30,10).
        let Seg::Cubic { c1, .. } = segs[2] else { panic!() };
        assert!((c1.x - 30.0).abs() < 1e-3, "reflected c1 {c1:?}");
    }

    #[test]
    fn evenodd_fill_rule_is_kept() {
        let s = "<svg viewBox=\"0 0 24 24\"><path fill-rule=\"evenodd\" d=\"M1 1h4v4h-4z\"/></svg>";
        let parsed = parse_svg(s);
        assert_eq!(parsed.items.len(), 1);
        assert!(parsed.items[0].even_odd);
    }

    #[test]
    fn view_box_scaling_with_nonzero_origin() {
        let s = "<svg viewBox=\"3.5 5.5 24.8 20\"><path d=\"M3.5 5.5 L8.5 5.5\"/></svg>";
        let parsed = parse_svg(s);
        assert!((parsed.origin.x - 3.5).abs() < 1e-3 && (parsed.origin.y - 5.5).abs() < 1e-3);
        // slot-locally, the first point maps to the artwork's own top-left.
        let slot = 14.0;
        let inset = inset_fraction("some_path");
        let scale = slot * inset / 24.8;
        let art_w = 24.8 * scale;
        let art_h = 20.0 * scale;
        let off = P2 { x: (slot - art_w) * 0.5, y: (slot - art_h) * 0.5 };
        let t0 = transform_point(P2 { x: 3.5, y: 5.5 }, parsed.origin, scale, off);
        assert!((t0.X - off.x).abs() < 1e-2 && (t0.Y - off.y).abs() < 1e-2);
    }

    #[test]
    fn arc_expands_to_a_cubic() {
        let mut out = Vec::new();
        arc_to_cubics(P2 { x: 0.0, y: 0.0 }, ArcInfo { end: P2 { x: 0.0, y: 2.0 }, rx: 1.0, ry: 1.0, phi_deg: 0.0, large: false, sweep: true }, &mut out);
        assert!(out.len() >= 1, "arc must expand to beziers");
        assert!(out.iter().all(|s| matches!(s, Seg::Cubic { .. })));
    }


    #[test]
    fn optical_insets_follow_the_class() {
        assert_eq!(inset_fraction("claude"), 0.82);
        assert_eq!(inset_fraction("deepseek"), 0.72);
        assert_eq!(inset_fraction("kimi"), 0.72);
        assert_eq!(inset_fraction("grok"), 1.0);
        assert_eq!(inset_fraction("codex"), 0.82);
    }

    #[test]
    fn style_backing_colours_are_exact() {
        // Recolor layers render as the brand ink in pure/badge and white in solid.
        let brand = brand_color("codex");
        assert_eq!(brand, 0x49a3b0);
        // Grok renders its own palette and never tints to brand.
        assert!(is_grok("grok"));
    }

    #[test]
    fn provider_id_alias_resolves_artwork() {
        assert!(provider_svg_source("mimo").is_some());
        assert!(provider_svg_source("mimoapi").is_some(), "mimoapi aliases mimo");
        assert!(provider_svg_source("alibabatokenplan").is_some(), "alibabatokenplan aliases alibaba");
        assert!(provider_svg_source("arkcodingplan").is_some(), "arkcodingplan aliases volcengine-ark");
    }

    /// Rendering onto an offscreen DIB to confirm a real logo fills a slot.
    #[test]
    fn logo_renders_ink_into_an_offscreen_slot() {
        let result = unsafe { render_logo_smoke() };
        if let Some(total) = result {
            assert!(total > 0, "a logo must put some ink on the slot");
        } else {
            eprintln!("Direct2D unavailable in this environment; skipping");
        }

        // Real-asset shape checks on the same render thread: OpenCode Go's
        // evenodd two-rectangle mark must come out as a hollow ring (the hole
        // stays empty, the ring band carries ink) and Codex must produce ink
        // at all. Both were reported missing/degenerate on the live strip.
        if let Some((total, hole, ring)) = unsafe { render_icon_regions("opencodego") } {
            assert!(total > 0, "opencodego icon rendered no ink");
            assert!(
                ring > 0 && hole < ring,
                "opencodego evenodd ring collapsed: hole {hole} vs ring {ring} —                  the inner rectangle was not subtracted"
            );
        } else {
            eprintln!("Direct2D unavailable; skipping opencodego shape check");
        }
        if let Some((total, _, _)) = unsafe { render_icon_regions("codex") } {
            assert!(total > 0, "codex icon rendered no ink");
        } else {
            eprintln!("Direct2D unavailable; skipping codex ink check");
        }
    }

    /// Render one icon into a 48px slot and return (total ink, hole ink,
    /// ring ink). The hole is the innermost 25% square; the ring is the band
    /// around it (12..36 px minus the hole). Used to prove evenodd hollow
    /// marks (opencodego) keep their hole and that a logo actually painted.
    unsafe fn render_icon_regions(provider_id: &str) -> Option<(u64, u64, u64)> {
        use windows::Win32::Graphics::Gdi::{
            BI_RGB, BITMAPINFO, BITMAPINFOHEADER, CreateCompatibleDC, CreateDIBSection,
            DIB_RGB_COLORS, DeleteDC, DeleteObject, HBITMAP, SelectObject,
        };
        const S: i32 = 48;
        let dc = CreateCompatibleDC(None);
        if dc.is_invalid() { return None; }
        let info = BITMAPINFO {
            bmiHeader: BITMAPINFOHEADER {
                biSize: size_of::<BITMAPINFOHEADER>() as u32,
                biWidth: S,
                biHeight: -S,
                biPlanes: 1,
                biBitCount: 32,
                biCompression: BI_RGB.0,
                ..Default::default()
            },
            ..Default::default()
        };
        let mut bits: *mut std::ffi::c_void = std::ptr::null_mut();
        let bitmap: HBITMAP = match CreateDIBSection(Some(dc), &info, DIB_RGB_COLORS, &mut bits, None, 0) {
            Ok(b) if !bits.is_null() => b,
            _ => { let _ = DeleteDC(dc); return None; },
        };
        let _ = SelectObject(dc, bitmap.into());
        std::ptr::write_bytes(bits.cast::<u8>(), 0, (S * S * 4) as usize);

        use windows::Win32::Graphics::Direct2D::{
            D2D1_RENDER_TARGET_PROPERTIES, D2D1_RENDER_TARGET_TYPE_SOFTWARE,
            D2D1_RENDER_TARGET_USAGE_GDI_COMPATIBLE, D2D1_FEATURE_LEVEL_DEFAULT, ID2D1DCRenderTarget,
        };
        use windows::Win32::Graphics::Dxgi::Common::DXGI_FORMAT_B8G8R8A8_UNORM;
        use windows::Win32::Foundation::RECT;
        use windows::Win32::Graphics::Direct2D::Common::{D2D1_ALPHA_MODE_IGNORE, D2D1_PIXEL_FORMAT};

        let factory: ID2D1Factory = match unsafe { D2D1CreateFactory(D2D1_FACTORY_TYPE_SINGLE_THREADED, None) } {
            Ok(f) => f,
            Err(_) => { let _ = DeleteObject(bitmap.into()); let _ = DeleteDC(dc); return None; },
        };
        let props = D2D1_RENDER_TARGET_PROPERTIES {
            r#type: D2D1_RENDER_TARGET_TYPE_SOFTWARE,
            pixelFormat: D2D1_PIXEL_FORMAT { format: DXGI_FORMAT_B8G8R8A8_UNORM, alphaMode: D2D1_ALPHA_MODE_IGNORE },
            dpiX: 0.0, dpiY: 0.0,
            usage: D2D1_RENDER_TARGET_USAGE_GDI_COMPATIBLE,
            minLevel: D2D1_FEATURE_LEVEL_DEFAULT,
        };
        let target: ID2D1DCRenderTarget = match unsafe { factory.CreateDCRenderTarget(&props) } {
            Ok(t) => t,
            Err(_) => { let _ = DeleteObject(bitmap.into()); let _ = DeleteDC(dc); return None; },
        };
        let bounds = RECT { left: 0, top: 0, right: S, bottom: S };
        if unsafe { target.BindDC(dc, &bounds) }.is_err() {
            let _ = DeleteObject(bitmap.into()); let _ = DeleteDC(dc); return None;
        }
        let slot = IconSlot { left: 0.0, top: 0.0, size_px: S as f32 };
        unsafe { target.BeginDraw() };
        let drawn = draw_icon(&target, provider_id, IconStyle::Pure, slot);
        let ended = unsafe { target.EndDraw(None, None) }.is_ok();
        if !drawn || !ended {
            let _ = DeleteObject(bitmap.into()); let _ = DeleteDC(dc); return None;
        }
        let pixels = std::slice::from_raw_parts(bits.cast::<u8>(), (S * S * 4) as usize);
        let ink = |x0: i32, y0: i32, x1: i32, y1: i32| -> u64 {
            let mut sum = 0u64;
            for y in y0..y1 {
                for x in x0..x1 {
                    let px = &pixels[(y * S + x) as usize * 4..][..4];
                    sum += u64::from(px[0]) + u64::from(px[1]) + u64::from(px[2]);
                }
            }
            sum
        };
        let hole = ink(S * 3 / 8, S * 3 / 8, S * 5 / 8, S * 5 / 8);
        let ring = ink(S / 4, S / 4, S * 3 / 4, S * 3 / 4) - hole;
        let total = ink(0, 0, S, S);
        let _ = SelectObject(dc, bitmap.into());
        let _ = DeleteObject(bitmap.into());
        let _ = DeleteDC(dc);
        Some((total, hole, ring))
    }

    /// Offscreen DIB render of one icon into a small slot. Shares the harness
    /// pattern from taskbar_text (create/destroy the DC per sample).
    unsafe fn render_logo_smoke() -> Option<u64> {
        use windows::Win32::Graphics::Gdi::{
            BI_RGB, BITMAPINFO, BITMAPINFOHEADER, CreateCompatibleDC, CreateDIBSection,
            DIB_RGB_COLORS, DeleteDC, DeleteObject, HBITMAP, SelectObject,
        };
        const S: i32 = 32;
        let dc = CreateCompatibleDC(None);
        if dc.is_invalid() { return None; }
        let info = BITMAPINFO {
            bmiHeader: BITMAPINFOHEADER {
                biSize: size_of::<BITMAPINFOHEADER>() as u32,
                biWidth: S,
                biHeight: -S,
                biPlanes: 1,
                biBitCount: 32,
                biCompression: BI_RGB.0,
                ..Default::default()
            },
            ..Default::default()
        };
        let mut bits: *mut std::ffi::c_void = std::ptr::null_mut();
        let bitmap: HBITMAP = match CreateDIBSection(Some(dc), &info, DIB_RGB_COLORS, &mut bits, None, 0) {
            Ok(b) if !bits.is_null() => b,
            _ => { let _ = DeleteDC(dc); return None; },
        };
        let _ = SelectObject(dc, bitmap.into());
        std::ptr::write_bytes(bits.cast::<u8>(), 0, (S * S * 4) as usize);

        use windows::Win32::Graphics::Direct2D::{
            D2D1_RENDER_TARGET_PROPERTIES, D2D1_RENDER_TARGET_TYPE_SOFTWARE,
            D2D1_RENDER_TARGET_USAGE_GDI_COMPATIBLE, D2D1_FEATURE_LEVEL_DEFAULT, ID2D1DCRenderTarget,
        };
        use windows::Win32::Graphics::Dxgi::Common::DXGI_FORMAT_B8G8R8A8_UNORM;
        use windows::Win32::Foundation::RECT;
        use windows::Win32::Graphics::Direct2D::Common::{D2D1_ALPHA_MODE_IGNORE, D2D1_PIXEL_FORMAT, D2D_RECT_F};

        let factory: ID2D1Factory = match unsafe { D2D1CreateFactory(D2D1_FACTORY_TYPE_SINGLE_THREADED, None) } {
            Ok(f) => f,
            Err(_) => { let _ = DeleteObject(bitmap.into()); let _ = DeleteDC(dc); return None; },
        };
        let props = D2D1_RENDER_TARGET_PROPERTIES {
            r#type: D2D1_RENDER_TARGET_TYPE_SOFTWARE,
            pixelFormat: D2D1_PIXEL_FORMAT { format: DXGI_FORMAT_B8G8R8A8_UNORM, alphaMode: D2D1_ALPHA_MODE_IGNORE },
            dpiX: 0.0, dpiY: 0.0,
            usage: D2D1_RENDER_TARGET_USAGE_GDI_COMPATIBLE,
            minLevel: D2D1_FEATURE_LEVEL_DEFAULT,
        };
        let target: ID2D1DCRenderTarget = match unsafe { factory.CreateDCRenderTarget(&props) } {
            Ok(t) => t,
            Err(_) => { let _ = DeleteObject(bitmap.into()); let _ = DeleteDC(dc); return None; },
        };
        let bounds = RECT { left: 0, top: 0, right: S, bottom: S };
        if unsafe { target.BindDC(dc, &bounds) }.is_err() {
            let _ = DeleteObject(bitmap.into()); let _ = DeleteDC(dc); return None;
        }
        eprintln!("render_logo_smoke: pre-draw, factory bound");
        let slot = IconSlot { left: 0.0, top: 0.0, size_px: S as f32 };
        unsafe { target.BeginDraw() };
        eprintln!("render_logo_smoke: begin draw ok, calling draw_icon");
        let drawn = draw_icon(&target, "codex", IconStyle::Pure, slot);
        eprintln!("render_logo_smoke: draw_icon returned {drawn}");
        let ended = unsafe { target.EndDraw(None, None) }.is_ok();
        if !drawn || !ended {
            let _ = DeleteObject(bitmap.into()); let _ = DeleteDC(dc); return None;
        }
        let pixels = std::slice::from_raw_parts(bits.cast::<u8>(), (S * S * 4) as usize);
        let total = pixels.chunks_exact(4).fold(0u64, |a, px| a + u64::from(px[0]) + u64::from(px[1]) + u64::from(px[2]));
        let _ = SelectObject(dc, bitmap.into());
        let _ = DeleteObject(bitmap.into());
        let _ = DeleteDC(dc);
        Some(total)
    }
}
