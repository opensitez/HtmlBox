//! Font-family and font-settings helpers.

#![allow(unused_imports)]
use std::collections::{HashMap, HashSet};
use rayon::prelude::*;
use crate::types::*;
use super::*;

// ─── Font utility helpers ──────────────────────────────────────────────────────

/// Split a CSS `font-family` value into individual family names.
/// Handles quoted names with spaces and strips surrounding quote characters.
/// `"Times New Roman", Arial, sans-serif` → `["Times New Roman", "Arial", "sans-serif"]`
pub fn split_font_families(raw: &str) -> Vec<String> {
    let mut families = Vec::new();
    let mut current  = String::new();
    let mut in_quotes = false;
    let mut quote_char = '"';

    for ch in raw.chars() {
        match ch {
            '"' | '\'' if !in_quotes => { in_quotes = true; quote_char = ch; }
            c if in_quotes && c == quote_char => { in_quotes = false; }
            ',' if !in_quotes => {
                let name = current.trim().to_string();
                if !name.is_empty() { families.push(name); }
                current.clear();
            }
            c => { current.push(c); }
        }
    }
    let name = current.trim().to_string();
    if !name.is_empty() { families.push(name); }
    families
}

/// Map CSS system-font keywords to a generic CSS family name.
/// Returns `None` for regular named fonts that should be kept as-is.
pub fn resolve_system_font_keyword(name: &str) -> Option<&'static str> {
    match name {
        "system-ui" | "-apple-system" | "BlinkMacSystemFont"
        | "ui-sans-serif" | "ui-rounded" => Some("sans-serif"),
        "ui-serif"     => Some("serif"),
        "ui-monospace" => Some("monospace"),
        _              => None,
    }
}

/// Parse a CSS `font-variation-settings` value into a list of `(axis-tag, value)` pairs.
/// Accepts `normal` (returns empty) or `"wght" 700, "wdth" 75`.
pub fn parse_variation_settings(v: &str) -> Vec<(String, f32)> {
    let v = v.trim();
    if v == "normal" { return Vec::new(); }
    let mut result = Vec::new();
    // Each entry: `"tag" value`, comma-separated.
    for entry in v.split(',') {
        let entry = entry.trim();
        // Find the quoted tag
        let (tag, rest) = if entry.starts_with('"') {
            let end = entry[1..].find('"').map(|i| i + 1);
            if let Some(end) = end {
                (&entry[1..end], entry[end + 1..].trim())
            } else { continue; }
        } else if entry.starts_with('\'') {
            let end = entry[1..].find('\'').map(|i| i + 1);
            if let Some(end) = end {
                (&entry[1..end], entry[end + 1..].trim())
            } else { continue; }
        } else { continue; };

        if let Ok(val) = rest.parse::<f32>() {
            result.push((tag.to_string(), val));
        }
    }
    result
}

/// Parse a CSS `font-feature-settings` value into `(feature-tag, value)` pairs.
/// Accepts `normal` (empty), `"kern"` (= 1), `"liga" on`, `"liga" off`, `"calt" 2`.
pub fn parse_feature_settings(v: &str) -> Vec<(String, u32)> {
    let v = v.trim();
    if v == "normal" { return Vec::new(); }
    let mut result = Vec::new();
    for entry in v.split(',') {
        let entry = entry.trim();
        let (tag, rest) = if entry.starts_with('"') {
            let end = entry[1..].find('"').map(|i| i + 1);
            if let Some(end) = end {
                (&entry[1..end], entry[end + 1..].trim())
            } else { continue; }
        } else if entry.starts_with('\'') {
            let end = entry[1..].find('\'').map(|i| i + 1);
            if let Some(end) = end {
                (&entry[1..end], entry[end + 1..].trim())
            } else { continue; }
        } else { continue; };

        let val = match rest {
            "" | "on"  => 1,
            "off"      => 0,
            s          => s.parse::<u32>().unwrap_or(1),
        };
        result.push((tag.to_string(), val));
    }
    result
}
