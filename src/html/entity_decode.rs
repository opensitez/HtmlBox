//! Decoding character references.
//!
//! ⛔ Separate from `entities.rs`, which is the NAMED-REFERENCE TABLE. The
//! table is data; this is the algorithm over it.

#![allow(unused_imports)]
use super::*;
use crate::types::*;
use crate::css::*;

// ─── HTML Entities ─────────────────────────────────────────────────────────

/// WHATWG HTML §13.5 "numeric character reference end state": the code points
/// that are NOT the character they name.
///
/// `&#128;` is not U+0080, it is `€`. The range 0x80–0x9F is Windows-1252
/// mistaken for Latin-1 by a generation of authoring tools, and the spec spells
/// out the substitution rather than let a page render control characters.
fn numeric_replacement(cp: u32) -> Option<char> {
    Some(match cp {
        0x00 | 0xD800..=0xDFFF => '\u{FFFD}', // null and lone surrogates
        0x80 => '\u{20AC}', 0x82 => '\u{201A}', 0x83 => '\u{0192}',
        0x84 => '\u{201E}', 0x85 => '\u{2026}', 0x86 => '\u{2020}',
        0x87 => '\u{2021}', 0x88 => '\u{02C6}', 0x89 => '\u{2030}',
        0x8A => '\u{0160}', 0x8B => '\u{2039}', 0x8C => '\u{0152}',
        0x8E => '\u{017D}', 0x91 => '\u{2018}', 0x92 => '\u{2019}',
        0x93 => '\u{201C}', 0x94 => '\u{201D}', 0x95 => '\u{2022}',
        0x96 => '\u{2013}', 0x97 => '\u{2014}', 0x98 => '\u{02DC}',
        0x99 => '\u{2122}', 0x9A => '\u{0161}', 0x9B => '\u{203A}',
        0x9C => '\u{0153}', 0x9E => '\u{017E}', 0x9F => '\u{0178}',
        _ if cp > 0x10FFFF => '\u{FFFD}',
        _ => return None,
    })
}

/// Decode character references in TEXT.
pub fn decode_entities(s: &str) -> String {
    decode_refs(s, false)
}

/// Decode character references in an ATTRIBUTE VALUE.
///
/// The one difference from text is the legacy rule: a semicolon-less name
/// followed by `=` or an alphanumeric is NOT a reference here, so a query
/// string like `?a=1&copy=2` keeps its literal `&copy`. In text the same
/// characters would resolve to `©`.
pub fn decode_entities_attr(s: &str) -> String {
    decode_refs(s, true)
}

fn decode_refs(s: &str, in_attribute: bool) -> String {
    let mut out = String::with_capacity(s.len());
    let bytes = s.as_bytes();
    let mut i = 0;
    while i < s.len() {
        if bytes[i] != b'&' {
            let start = i;
            i += 1;
            while i < s.len() && bytes[i] != b'&' { i += 1; }
            out.push_str(&s[start..i]);
            continue;
        }
        let after = i + 1;
        // Numeric: `&#123;` / `&#x1F600;`
        if after < s.len() && bytes[after] == b'#' {
            let mut j = after + 1;
            let hex = j < s.len() && (bytes[j] | 0x20) == b'x';
            if hex { j += 1; }
            let digits_start = j;
            while j < s.len()
                && (if hex { bytes[j].is_ascii_hexdigit() } else { bytes[j].is_ascii_digit() })
            { j += 1; }
            if j > digits_start {
                let radix = if hex { 16 } else { 10 };
                let cp = u32::from_str_radix(&s[digits_start..j], radix).unwrap_or(0xFFFD);
                let ch = numeric_replacement(cp)
                    .or_else(|| char::from_u32(cp))
                    .unwrap_or('\u{FFFD}');
                out.push(ch);
                // A missing `;` is a parse error, not a reason to keep the text.
                i = if j < s.len() && bytes[j] == b';' { j + 1 } else { j };
                continue;
            }
            out.push('&');
            i += 1;
            continue;
        }
        // Named: consume the LONGEST name in the table that matches here.
        // Longest-first is what makes `&notin;` the set operator and `&notit;`
        // the legacy `&not` followed by `it;` — a shortest match, or a match
        // that required the semicolon, gets both of those wrong.
        let window = (after + entities::MAX_NAME_LEN).min(s.len());
        let mut matched: Option<(usize, &'static str)> = None;
        let mut k = window;
        while k > after {
            if s.is_char_boundary(k) {
                if let Some(exp) = entities::lookup(&s[after..k]) {
                    matched = Some((k, exp));
                    break;
                }
            }
            k -= 1;
        }
        match matched {
            Some((end, exp)) => {
                let had_semi = bytes[end - 1] == b';';
                // Legacy (no semicolon) inside an attribute value: not a
                // reference when the next character could continue a name.
                let legacy_blocked = in_attribute
                    && !had_semi
                    && end < s.len()
                    && (bytes[end] == b'=' || bytes[end].is_ascii_alphanumeric());
                if legacy_blocked {
                    out.push('&');
                    i += 1;
                } else {
                    out.push_str(exp);
                    i = end;
                }
            }
            None => {
                out.push('&');
                i += 1;
            }
        }
    }
    out
}
