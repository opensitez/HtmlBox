//! `@keyframes` extraction.

#![allow(unused_imports)]
use std::collections::{HashMap, HashSet};
use rayon::prelude::*;
use crate::types::*;
use super::*;

// ─── @keyframes extraction ────────────────────────────────────────────────────

/// Parse all `@keyframes` (and `@-webkit-keyframes`) blocks from a CSS string.
pub fn extract_keyframes(css: &str) -> HashMap<String, Vec<KeyframeStop>> {
    let cleaned = strip_css_comments(css);
    extract_keyframes_cleaned(&cleaned)
}

pub(crate) fn extract_keyframes_cleaned(css: &str) -> HashMap<String, Vec<KeyframeStop>> {
    let mut out: HashMap<String, Vec<KeyframeStop>> = HashMap::new();
    let mut s = css.trim();

    while !s.is_empty() {
        s = s.trim_start();
        if s.is_empty() { break; }

        if !s.starts_with('@') {
            // Skip regular rules without recursing
            if let Some(brace) = s.find('{') {
                let (_, rest) = consume_block(&s[brace..]);
                s = rest;
            } else { break; }
            continue;
        }

        // Only lowercase a small prefix to identify the @-rule type
        let prefix = &s[..s.len().min(30)];
        let at_lower: String = prefix.to_ascii_lowercase();

        // Handle no-block @ rules
        if at_lower.starts_with("@import") || at_lower.starts_with("@charset") {
            if let Some(semi) = s.find(';') { s = &s[semi + 1..]; } else { break; }
            continue;
        }

        let brace = match s.find('{') {
            Some(p) => p,
            None => { if let Some(semi) = s.find(';') { s = &s[semi+1..]; } else { break; } continue; }
        };
        let at_header = s[..brace].trim();
        let rest_from_brace = &s[brace..];
        let (inner_block, after_block) = consume_block(rest_from_brace);

        if at_lower.starts_with("@keyframes") || at_lower.starts_with("@-webkit-keyframes") {
            let prefix_len = if at_lower.starts_with("@-webkit-keyframes") {
                "@-webkit-keyframes".len()
            } else {
                "@keyframes".len()
            };
            let name = at_header[prefix_len..].trim().to_string();
            if !name.is_empty() {
                out.insert(name, parse_keyframe_stops(inner_block));
            }
        } else if at_lower.starts_with("@media") || at_lower.starts_with("@container") {
            // Recurse for nested @keyframes (rare but spec-valid)
            out.extend(extract_keyframes_cleaned(inner_block));
        }

        s = after_block;
    }
    out
}

/// Parse the body of a `@keyframes` block into a sorted list of stops.
fn parse_keyframe_stops(block: &str) -> Vec<KeyframeStop> {
    let mut stops: Vec<KeyframeStop> = Vec::new();
    let mut s = block.trim();

    while !s.is_empty() {
        s = s.trim_start();
        if s.is_empty() { break; }

        let brace = match s.find('{') { Some(p) => p, None => break };
        let selector = s[..brace].trim();
        let (decl_block, rest) = consume_block(&s[brace..]);
        s = rest;

        let props = parse_declarations(decl_block);
        let prop_vec: Vec<(String, String)> = props.into_iter().map(|(k, v)| {
            // Normalize color values to rgba() so interpolation works.
            let is_color_prop = matches!(k.as_str(),
                "color" | "background-color" | "border-color" |
                "border-top-color" | "border-right-color" |
                "border-bottom-color" | "border-left-color" |
                "outline-color" | "fill" | "stroke"
            );
            if is_color_prop {
                if let Some(c) = parse_color(&v) {
                    return (k, format!("rgba({},{},{},{})", c.r, c.g, c.b,
                        (c.a as f32 / 255.0 * 1000.0).round() / 1000.0));
                }
            }
            (k, v)
        }).collect();

        for sel in selector.split(',') {
            let sel = sel.trim();
            let offset: f32 = match sel {
                "from" => 0.0,
                "to"   => 1.0,
                s if s.ends_with('%') => {
                    s[..s.len()-1].trim().parse::<f32>().unwrap_or(0.0) / 100.0
                }
                _ => continue,
            };
            stops.push(KeyframeStop { offset, properties: prop_vec.clone() });
        }
    }

    stops.sort_by(|a, b| a.offset.partial_cmp(&b.offset).unwrap_or(std::cmp::Ordering::Equal));
    stops
}
