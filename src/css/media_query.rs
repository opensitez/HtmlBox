//! Media query evaluation.

#![allow(unused_imports)]
use std::collections::{HashMap, HashSet};
use rayon::prelude::*;
use crate::types::*;
use super::*;

// ─── Media Query Evaluator ───────────────────────────────────────────────────

/// Evaluate a CSS @media condition string.
/// Returns true if the condition matches the given viewport dimensions.
/// `condition` is the full text after "@media" (trimmed).
pub fn evaluate_media(condition: &str, vw: f32, vh: f32) -> bool {
    let cond = condition.trim();
    if cond.is_empty() { return true; }

    // ⛔ `only` is a no-op qualifier — `only print` IS `print` (Media Queries
    // §3). Leaving it attached meant the string matched no known media type
    // and fell through to the permissive default, so a print stylesheet
    // written `@media only print` (or `<link media="only print">`) applied to
    // the SCREEN: `display: block` everywhere, columns and floats dropped,
    // navigation hidden. A page styled that way renders as one long column,
    // exactly as if it had been printed.
    let cond = match cond.len() >= 5 && cond[..5].eq_ignore_ascii_case("only ") {
        true  => cond[5..].trim_start(),
        false => cond,
    };
    if cond.is_empty() { return true; }

    // Handle comma-separated list at top level (OR semantics)
    // We first split on `and`/`or` outside parens, then check named types.
    // But comma is always OR at the top level.
    {
        let mut depth = 0usize;
        let bytes = cond.as_bytes();
        let mut comma_pos: Option<usize> = None;
        for (i, &b) in bytes.iter().enumerate() {
            match b {
                b'(' => depth += 1,
                b')' => { if depth > 0 { depth -= 1; } }
                b',' if depth == 0 => { comma_pos = Some(i); break; }
                _ => {}
            }
        }
        if let Some(pos) = comma_pos {
            let left  = &cond[..pos];
            let right = &cond[pos+1..];
            return evaluate_media(left.trim(), vw, vh) || evaluate_media(right.trim(), vw, vh);
        }
    }

    // Handle `not` prefix (before `and`/`or` splitting)
    if let Some(rest) = cond.strip_prefix("not ") {
        return !evaluate_media(rest.trim(), vw, vh);
    }

    // Handle `and` combinator outside parens
    if let Some(idx) = find_keyword_outside_parens(cond, " and ") {
        let left  = &cond[..idx];
        let right = &cond[idx + 5..];
        return evaluate_media(left.trim(), vw, vh) && evaluate_media(right.trim(), vw, vh);
    }

    // Handle `or` combinator outside parens
    if let Some(idx) = find_keyword_outside_parens(cond, " or ") {
        let left  = &cond[..idx];
        let right = &cond[idx + 4..];
        return evaluate_media(left.trim(), vw, vh) || evaluate_media(right.trim(), vw, vh);
    }

    // Named media types (no parens)
    if !cond.starts_with('(') {
        return match cond.to_ascii_lowercase().as_str() {
            "screen" | "all" => true,
            // Everything that is not a screen. The deprecated types are listed
            // because they must not match either — a `<link media="handheld">`
            // sheet applying to a desktop render is the same failure as the
            // print one.
            "print" | "speech" | "aural" | "braille" | "embossed"
            | "handheld" | "projection" | "tty" | "tv" => false,
            // Anything unrecognised is more likely a parse artefact than a
            // real media type, so it stays permissive rather than silently
            // dropping a stylesheet.
            _ => true,
        };
    }

    // Strip outer parens for feature queries
    let inner = if cond.starts_with('(') && cond.ends_with(')') {
        &cond[1..cond.len()-1]
    } else {
        cond
    };
    let lower = inner.to_ascii_lowercase();
    let lower = lower.trim();

    if let Some(rest) = lower.strip_prefix("min-width:") {
        return vw >= parse_media_px(rest.trim());
    }
    if let Some(rest) = lower.strip_prefix("max-width:") {
        return vw <= parse_media_px(rest.trim());
    }
    if let Some(rest) = lower.strip_prefix("min-height:") {
        return vh >= parse_media_px(rest.trim());
    }
    if let Some(rest) = lower.strip_prefix("max-height:") {
        return vh <= parse_media_px(rest.trim());
    }
    if let Some(rest) = lower.strip_prefix("orientation:") {
        return match rest.trim() {
            "landscape" => vw > vh,
            "portrait"  => vh >= vw,
            _ => true,
        };
    }
    if let Some(rest) = lower.strip_prefix("prefers-color-scheme:") {
        return match rest.trim() {
            "light" => true,
            "dark"  => false,
            _ => true,
        };
    }
    if let Some(rest) = lower.strip_prefix("hover:") {
        return match rest.trim() { "hover" => true, "none" => false, _ => true };
    }
    if let Some(rest) = lower.strip_prefix("pointer:") {
        return match rest.trim() { "fine" => true, "coarse" | "none" => false, _ => true };
    }
    if let Some(rest) = lower.strip_prefix("min-resolution:") {
        let s = rest.trim().trim_end_matches("dpi").trim_end_matches("dpcm").trim();
        let dpi: f32 = s.parse().unwrap_or(0.0);
        return dpi <= 96.0;
    }
    if let Some(rest) = lower.strip_prefix("max-resolution:") {
        let s = rest.trim().trim_end_matches("dpi").trim_end_matches("dpcm").trim();
        let dpi: f32 = s.parse().unwrap_or(0.0);
        return dpi >= 96.0;
    }
    // Modern range syntax: `width >= 300px`, `width > 300px`, etc.
    fn parse_media_range(expr: &str, dim: f32) -> Option<bool> {
        let e = expr.trim();
        if let Some(rest) = e.strip_prefix(">=") { return Some(dim >= parse_media_px(rest.trim())); }
        if let Some(rest) = e.strip_prefix("<=") { return Some(dim <= parse_media_px(rest.trim())); }
        if let Some(rest) = e.strip_prefix('>')  { return Some(dim >  parse_media_px(rest.trim())); }
        if let Some(rest) = e.strip_prefix('<')  { return Some(dim <  parse_media_px(rest.trim())); }
        if let Some(rest) = e.strip_prefix(':')  { return Some((dim - parse_media_px(rest.trim())).abs() < 0.5); }
        None
    }
    if let Some(rest) = lower.strip_prefix("width")  { if let Some(v) = parse_media_range(rest, vw) { return v; } }
    if let Some(rest) = lower.strip_prefix("height") { if let Some(v) = parse_media_range(rest, vh) { return v; } }
    if let Some(rest) = lower.strip_prefix("inline-size")  { if let Some(v) = parse_media_range(rest, vw) { return v; } }
    if let Some(rest) = lower.strip_prefix("block-size")   { if let Some(v) = parse_media_range(rest, vh) { return v; } }

    // Unknown feature — fail-open
    true
}

/// Find byte index of `keyword` in `s` where it is not inside parentheses.
pub(crate) fn find_keyword_outside_parens(s: &str, keyword: &str) -> Option<usize> {
    let bytes = s.as_bytes();
    let kw = keyword.as_bytes();
    let mut depth = 0usize;
    let mut i = 0;
    while i + kw.len() <= bytes.len() {
        match bytes[i] {
            b'(' => { depth += 1; i += 1; }
            b')' => { if depth > 0 { depth -= 1; } i += 1; }
            _ => {
                if depth == 0 && bytes[i..].starts_with(kw) {
                    return Some(i);
                }
                i += 1;
            }
        }
    }
    None
}

/// A length in a media query, in px.
///
/// ⛔ This was a THIRD private unit table — `px`, `em`, and a bare `parse()`
/// for everything else. A bare parse of `"40rem"` FAILS, giving 0, and
/// `(min-width: 40rem)` with a threshold of 0 matches every viewport. Every
/// rem-based breakpoint — which is what Bootstrap and Tailwind emit — was
/// therefore always on. Measured: `(min-width: 4000rem)` (64000px) matched a
/// 1200px viewport; Chrome correctly does not.
///
/// `parse_length` is the single unit definition. Relative units in a media
/// query resolve against the INITIAL font size, not any element's — Media
/// Queries 4 §1.3 — so both `em` and `rem` are 16px here.
pub(crate) fn parse_media_px(s: &str) -> f32 {
    let s = s.trim();
    if s.is_empty() { return 0.0; }
    crate::css::value_parse::parse_length(s)
        .resolve_vp(16.0, 0.0, 16.0, viewport().0, viewport().1)
}

thread_local! {
    /// The viewport the media query is being evaluated against, so `vw`/`vh`
    /// inside one mean what they say.
    static MQ_VIEWPORT: std::cell::Cell<(f32, f32)> = std::cell::Cell::new((0.0, 0.0));
}

fn viewport() -> (f32, f32) { MQ_VIEWPORT.with(|v| v.get()) }

/// Record the viewport for the duration of a media-query evaluation.
pub(crate) fn set_media_viewport(w: f32, h: f32) {
    MQ_VIEWPORT.with(|v| v.set((w, h)));
}
