//! Applying a parsed declaration to a `ComputedStyle`.

#![allow(unused_imports)]
use std::collections::{HashMap, HashSet};
use rayon::prelude::*;
use crate::types::*;
use super::*;

// ─── CSS Property Application ─────────────────────────────────────────────────





pub fn apply_property(style: &mut ComputedStyle, prop: &str, value: &str) {
    // HTML attributes that aren't real CSS properties — handle before resolving
    match prop {
        "cellpadding" => {
            let v = value.trim();
            style.cell_padding = parse_length(v);
            return;
        }
        "cellspacing" => {
            let v = value.trim();
            style.border_spacing_h = parse_length(v);
            style.border_spacing_v = parse_length(v);
            return;
        }
        _ => {}
    }
    // CSS custom properties (--*) are not resolved by PropertyId
    let v = value.trim();
    if prop.starts_with("--") {
        style.custom_props.insert(prop.to_string(), v.to_string());
        return;
    }
    let id = properties::resolve(prop);
    apply_property_by_id(style, id, value);
}

/// Apply a pre-parsed CssValue to a computed style. This is the primary cascade
/// path — values are pre-parsed during stylesheet compilation.
pub fn apply_css_value(style: &mut ComputedStyle, id: properties::PropertyId, value: &crate::types::CssValue) {
    use crate::types::CssValue;
    match value {
        CssValue::Inherit => return,
        CssValue::Initial | CssValue::Unset => {
            // Reset property to its initial (default) value
            let default_style = ComputedStyle::default();
            (property_defs::get(id).copy)(style, &default_style);
            return;
        }
        CssValue::Length(l) => {
            if apply_length_value(style, id, l) { return; }
        }
        CssValue::Color(c) => {
            if apply_color_value(style, id, c) { return; }
        }
        CssValue::Number(n) => {
            if apply_number_value(style, id, *n) { return; }
        }
        CssValue::Integer(n) => {
            if apply_integer_value(style, id, *n) { return; }
        }
        // ── Keyword values — direct assignment, no parsing ──
        CssValue::Display(d) => { style.display = *d; return; }
        CssValue::Position(p) => { style.position = *p; return; }
        CssValue::Float(f) => { style.float = *f; return; }
        CssValue::Clear(c) => { style.clear = *c; return; }
        CssValue::BoxSizing(b) => { style.box_sizing = *b; return; }
        CssValue::Overflow(o) => {
            use properties::PropertyId::*;
            match id { OverflowX => style.overflow_x = *o, OverflowY => style.overflow_y = *o, _ => { style.overflow_x = *o; style.overflow_y = *o; } }
            return;
        }
        CssValue::Visible(v) => { style.visibility = *v; return; }
        CssValue::TextAlign(a) => { style.text_align = *a; return; }
        CssValue::TextTransform(t) => { style.text_transform = *t; return; }
        CssValue::WhiteSpace(w) => { style.white_space = *w; return; }
        CssValue::FontWeight(w) => { style.font_weight = *w; return; }
        CssValue::FontStyle(s) => { style.font_style = *s; return; }
        CssValue::FlexDirection(d) => { style.flex_direction = *d; return; }
        CssValue::FlexWrap(w) => { style.flex_wrap = *w; return; }
        CssValue::AlignItems(a) => { style.align_items = *a; return; }
        CssValue::AlignSelf(a) => { style.align_self = *a; return; }
        CssValue::AlignContent(a) => { style.align_content = *a; return; }
        CssValue::JustifyContent(j) => { style.justify_content = *j; return; }
        CssValue::ListStyleType(l) => { style.list_style_type = *l; return; }
        CssValue::ListStylePosition(l) => { style.list_style_position = *l; return; }
        CssValue::WordBreak(w) => { style.word_break = *w; return; }
        CssValue::BorderStyle(bs) => {
            use crate::types::BorderStyleValue as BSV;
            let s = match bs { BSV::None => crate::types::BorderStyle::None, BSV::Hidden => crate::types::BorderStyle::Hidden, BSV::Solid => crate::types::BorderStyle::Solid, BSV::Dashed => crate::types::BorderStyle::Dashed, BSV::Dotted => crate::types::BorderStyle::Dotted, BSV::Double => crate::types::BorderStyle::Double, BSV::Groove => crate::types::BorderStyle::Groove, BSV::Ridge => crate::types::BorderStyle::Ridge, BSV::Inset => crate::types::BorderStyle::Inset, BSV::Outset => crate::types::BorderStyle::Outset };
            use properties::PropertyId::*;
            match id { BorderTopStyle => style.border_top_style = s, BorderRightStyle => style.border_right_style = s, BorderBottomStyle => style.border_bottom_style = s, BorderLeftStyle => style.border_left_style = s, _ => {} }
            return;
        }
        CssValue::VerticalAlign(v) => { style.vertical_align = *v; return; }
        CssValue::Raw(s) => {
            apply_property_by_id_str(style, id, s);
            return;
        }
    }
    // Fallback: typed value wasn't handled — serialize back to string
    let s = match value {
        CssValue::Length(l) => crate::html::serializer::serialize_length(l),
        CssValue::Color(c) => format!("#{:02x}{:02x}{:02x}", c.r, c.g, c.b),
        CssValue::Number(n) => format!("{}", n),
        CssValue::Integer(n) => format!("{}", n),
        _ => return,
    };
    apply_property_by_id_str(style, id, &s);
}

/// Apply a raw string value — used for var() resolution, inline styles,
/// and properties that haven't been converted to typed CssValue yet.
pub fn apply_property_by_id_str(style: &mut ComputedStyle, id: properties::PropertyId, value: &str) {
    let v = value.trim();
    if v == "inherit" { return; }
    if matches!(v, "revert" | "revert-layer") { return; }
    if matches!(v, "initial" | "unset") {
        // Reset the property to its initial (default) value
        let default_style = ComputedStyle::default();
        (property_defs::get(id).copy)(style, &default_style);
        return;
    }
    (property_defs::get(id).apply)(style, v);
}

/// Backward-compat wrapper — accepts &str directly.
pub fn apply_property_by_id(style: &mut ComputedStyle, id: properties::PropertyId, value: &str) {
    apply_property_by_id_str(style, id, value);
}

// ── Typed value application (fast paths for pre-parsed values) ────────────

fn apply_length_value(style: &mut ComputedStyle, id: properties::PropertyId, l: &crate::types::CssLength) -> bool {
    use properties::PropertyId::*;
    match id {
        Width => style.width = l.clone(),
        Height => style.height = l.clone(),
        MinWidth => style.min_width = l.clone(),
        MinHeight => style.min_height = l.clone(),
        MaxWidth => style.max_width = l.clone(),
        MaxHeight => style.max_height = l.clone(),
        MarginTop => style.margin_top = l.clone(),
        MarginRight => style.margin_right = l.clone(),
        MarginBottom => style.margin_bottom = l.clone(),
        MarginLeft => style.margin_left = l.clone(),
        PaddingTop => style.padding_top = l.clone(),
        PaddingRight => style.padding_right = l.clone(),
        PaddingBottom => style.padding_bottom = l.clone(),
        PaddingLeft => style.padding_left = l.clone(),
        BorderTopWidth => style.border_top_width = l.clone(),
        BorderRightWidth => style.border_right_width = l.clone(),
        BorderBottomWidth => style.border_bottom_width = l.clone(),
        BorderLeftWidth => style.border_left_width = l.clone(),
        Top => style.top = l.clone(),
        Right => style.right = l.clone(),
        Bottom => style.bottom = l.clone(),
        Left => style.left = l.clone(),
        FlexBasis => style.flex_basis = l.clone(),
        // FontSize is NOT handled here — it needs special em/rem resolution at cascade time
        LineHeight => style.line_height = l.clone(),
        LetterSpacing => style.letter_spacing = l.clone(),
        WordSpacing => style.word_spacing = l.clone(),
        TextIndent => style.text_indent = l.clone(),
        Gap | RowGap => style.row_gap = l.clone(),
        ColumnGap => style.column_gap = l.clone(),
        OutlineWidth => if let crate::types::CssLength::Px(px) = l { style.outline_width = *px; },
        OutlineOffset => if let crate::types::CssLength::Px(px) = l { style.outline_offset = *px; },
        TextDecorationThickness => style.text_decoration_thickness = l.clone(),
        TextUnderlineOffset => style.text_underline_offset = l.clone(),
        _ => return false, // Unhandled — caller falls back to string path
    }
    true
}

fn apply_color_value(style: &mut ComputedStyle, id: properties::PropertyId, c: &crate::types::Color) -> bool {
    use properties::PropertyId::*;
    match id {
        Color => style.color = *c,
        BackgroundColor => style.background_color = *c,
        BorderTopColor => style.border_top_color = *c,
        BorderRightColor => style.border_right_color = *c,
        BorderBottomColor => style.border_bottom_color = *c,
        BorderLeftColor => style.border_left_color = *c,
        OutlineColor => style.outline_color = *c,
        CaretColor => style.caret_color = Some(*c),
        _ => return false
    }
    true
}

fn apply_number_value(style: &mut ComputedStyle, id: properties::PropertyId, n: f32) -> bool {
    use properties::PropertyId::*;
    match id {
        Opacity => style.opacity = n.clamp(0.0, 1.0),
        FlexGrow => style.flex_grow = n,
        FlexShrink => style.flex_shrink = n,
        FontStretch => style.font_stretch = n,
        _ => return false
    }
    true
}

fn apply_integer_value(style: &mut ComputedStyle, id: properties::PropertyId, n: i32) -> bool {
    use properties::PropertyId::*;
    match id {
        ZIndex => style.z_index = n,
        Order => style.order = n,
        _ => return false
    }
    true
}



/// Parse a CSS `filter` value string into `CssFilters`.
pub fn parse_css_filter(v: &str) -> crate::types::CssFilters {
    use crate::types::{CssFilters, FilterOp};
    let mut ops = Vec::new();
    if v.trim() == "none" { return CssFilters::default(); }
    let mut rest = v.trim();
    while !rest.is_empty() {
        rest = rest.trim_start();
        if rest.is_empty() { break; }
        let paren_pos = match rest.find('(') { Some(p) => p, None => break };
        let func = rest[..paren_pos].trim().to_ascii_lowercase();
        let after_paren = &rest[paren_pos + 1..];
        let close = after_paren.find(')').unwrap_or(after_paren.len());
        let arg_str = after_paren[..close].trim();
        rest = if close + 1 < after_paren.len() { &after_paren[close + 1..] } else { "" };
        // Parse arg as f32 (strip %, px, deg)
        let arg: f32 = arg_str.trim_end_matches('%').trim_end_matches("px")
                               .trim_end_matches("deg").parse().unwrap_or(0.0);
        // Normalize percent-based args to 0..1
        let arg_norm = if arg_str.ends_with('%') { arg / 100.0 } else { arg };
        match func.as_str() {
            "blur"        => ops.push(FilterOp::Blur(arg)),
            "brightness"  => ops.push(FilterOp::Brightness(arg_norm)),
            "contrast"    => ops.push(FilterOp::Contrast(arg_norm)),
            "grayscale"   => ops.push(FilterOp::Grayscale(arg_norm)),
            "hue-rotate"  => ops.push(FilterOp::HueRotate(arg)),
            "invert"      => ops.push(FilterOp::Invert(arg_norm)),
            "opacity"     => ops.push(FilterOp::Opacity(arg_norm)),
            "saturate"    => ops.push(FilterOp::Saturate(arg_norm)),
            "sepia"       => ops.push(FilterOp::Sepia(arg_norm)),
            "drop-shadow" => {
                // drop-shadow(dx dy blur color)
                let parts: Vec<&str> = arg_str.split_whitespace().collect();
                let pf = |i: usize| parts.get(i)
                    .and_then(|s| s.trim_end_matches("px").parse::<f32>().ok())
                    .unwrap_or(0.0);
                ops.push(FilterOp::DropShadow {
                    dx: pf(0), dy: pf(1), blur: pf(2),
                    color: parts.get(3).and_then(|s| parse_color(s))
                        .unwrap_or(crate::types::Color::BLACK),
                });
            }
            _ => {}
        }
    }
    CssFilters { ops }
}

/// Resolve `var(--name)` and `var(--name, fallback)` references in a CSS value.
/// Variables in the map are pre-resolved by `pre_resolve_variables`, so one pass suffices.
/// Any still-unresolved var() (unknown custom property with no fallback) is dropped.
pub fn resolve_var_references(val: &str, variables: &HashMap<String, String>) -> String {
    if !val.contains("var(") { return val.to_string(); }
    let mut result = resolve_var_pass(val, variables);
    // Iterate to resolve chained vars (var(--a) → var(--b) → value).
    // Max 10 iterations to prevent infinite loops from circular refs.
    for _ in 0..10 {
        if !result.contains("var(") { return result; }
        let next = resolve_var_pass(&result, variables);
        if next == result { break; } // no progress — unresolvable
        result = next;
    }
    // Drop any remaining unresolved var() by substituting with fallback or "".
    if result.contains("var(") { resolve_var_pass(&result, &HashMap::new()) } else { result }
}

pub(crate) fn resolve_var_pass(val: &str, variables: &HashMap<String, String>) -> String {
    if !val.contains("var(") {
        return val.to_string();
    }
    let mut out = String::new();
    let mut rest = val;
    while !rest.is_empty() {
        if let Some(start) = rest.find("var(") {
            out.push_str(&rest[..start]);
            rest = &rest[start + 4..]; // skip "var("
            // find matching closing paren
            let mut depth = 1usize;
            let mut end = 0;
            let bytes = rest.as_bytes();
            while end < bytes.len() {
                match bytes[end] {
                    b'(' => depth += 1,
                    b')' => { depth -= 1; if depth == 0 { break; } }
                    _ => {}
                }
                end += 1;
            }
            let inner = &rest[..end];
            rest = if end < rest.len() { &rest[end+1..] } else { "" };
            // inner = "--name" or "--name, fallback"
            let (name, fallback) = if let Some(comma) = inner.find(',') {
                (inner[..comma].trim(), Some(inner[comma+1..].trim()))
            } else {
                (inner.trim(), None)
            };
            if let Some(resolved) = variables.get(name) {
                if resolved.is_empty() {
                    if let Some(fb) = fallback {
                        out.push_str(fb);
                    }
                } else {
                    out.push_str(resolved);
                }
            } else if let Some(fb) = fallback {
                out.push_str(fb);
            }
        } else {
            out.push_str(rest);
            break;
        }
    }
    out
}

/// Resolve a CSS `content` property value string to a displayable string.
/// Handles string literals, open-quote/close-quote, and discards complex expressions.
/// Process CSS Unicode escapes in a string: `\e001` → U+E001, `\A` → newline, etc.
fn unescape_css_string(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut chars = s.chars().peekable();
    while let Some(c) = chars.next() {
        if c == '\\' {
            // Collect up to 6 hex digits
            let mut hex = String::new();
            while hex.len() < 6 {
                match chars.peek() {
                    Some(ch) if ch.is_ascii_hexdigit() => { hex.push(*ch); chars.next(); }
                    _ => break,
                }
            }
            if !hex.is_empty() {
                // Optional trailing whitespace is consumed after hex escape
                if let Some(&' ') = chars.peek() { chars.next(); }
                if let Ok(cp) = u32::from_str_radix(&hex, 16) {
                    if let Some(uc) = char::from_u32(cp) {
                        out.push(uc);
                        continue;
                    }
                }
                // Invalid code point — output replacement character
                out.push('\u{FFFD}');
            } else if let Some(next) = chars.next() {
                // Escaped literal character (e.g. \\ → \, \" → ")
                out.push(next);
            }
        } else {
            out.push(c);
        }
    }
    out
}

pub fn resolve_content_value(v: &str) -> String {
    let v = v.trim();
    match v {
        "none" | "normal" => String::new(),
        "open-quote"      => "\u{201C}".to_string(),  // "
        "close-quote"     => "\u{201D}".to_string(),  // "
        "no-open-quote" | "no-close-quote" => String::new(),
        _ => {
            // Quoted string: "text" or 'text'
            if (v.starts_with('"') && v.ends_with('"'))
                || (v.starts_with('\'') && v.ends_with('\''))
            {
                return unescape_css_string(&v[1..v.len()-1]);
            }
            // Multiple tokens (e.g. '"foo" open-quote'): concatenate resolved parts
            let mut out = String::new();
            let mut rest = v;
            while !rest.is_empty() {
                rest = rest.trim_start();
                if rest.starts_with('"') || rest.starts_with('\'') {
                    let q = &rest[..1];
                    if let Some(end) = rest[1..].find(q) {
                        out.push_str(&unescape_css_string(&rest[1..end+1]));
                        rest = &rest[end+2..];
                    } else {
                        out.push_str(&unescape_css_string(&rest[1..]));
                        break;
                    }
                } else {
                    // keyword token
                    let end = rest.find(|c: char| c.is_whitespace()).unwrap_or(rest.len());
                    let tok = &rest[..end];
                    match tok {
                        "open-quote"    => out.push('\u{201C}'),
                        "close-quote"   => out.push('\u{201D}'),
                        "no-open-quote" | "no-close-quote" => {}
                        _ => {
                            // counter(name) → emit placeholder for later resolution
                            if tok.starts_with("counter(") && tok.ends_with(')') {
                                out.push('\x01');
                                out.push_str(tok);
                                out.push('\x01');
                            }
                            // attr(), counters(), etc. — ignore for now
                        }
                    }
                    rest = &rest[end..];
                }
            }
            out
        }
    }
}

/// Resolve counter() and counters() function calls in ::before/::after content.
pub(crate) fn resolve_counters_in_content(content: &str, counters: &HashMap<String, Vec<i32>>) -> String {
    if !content.contains('\x01') { return content.to_string(); }
    // Placeholder \x01counter(name)\x01 was inserted by resolve_content_value
    let mut out = String::new();
    let mut rest = content;
    while let Some(start) = rest.find('\x01') {
        out.push_str(&rest[..start]);
        rest = &rest[start + 1..];
        if let Some(end) = rest.find('\x01') {
            let func = &rest[..end];
            rest = &rest[end + 1..];
            if let Some(inner) = func.strip_prefix("counter(").and_then(|s| s.strip_suffix(')')) {
                let name = inner.split(',').next().unwrap_or("").trim();
                let val = counters.get(name).and_then(|s| s.last()).copied().unwrap_or(0);
                out.push_str(&val.to_string());
            } else {
                out.push_str(func);
            }
        }
    }
    out.push_str(rest);
    out
}

/// Parse CSS counter list: "name1 3 name2 name3 -1" → [(name1,3),(name2,1),(name3,-1)]
pub fn parse_counter_list(v: &str) -> Vec<(String, i32)> {
    if v == "none" { return Vec::new(); }
    let mut result = Vec::new();
    let toks: Vec<&str> = v.split_whitespace().collect();
    let mut i = 0;
    while i < toks.len() {
        let name = toks[i].to_string();
        i += 1;
        let val = if i < toks.len() {
            if let Ok(n) = toks[i].parse::<i32>() { i += 1; n } else { 1 }
        } else { 1 };
        result.push((name, val));
    }
    result
}

/// Split a shorthand value into top-level tokens, respecting parentheses.
/// E.g. "rgb(200,200,200) red" → ["rgb(200,200,200)", "red"]
pub fn split_shorthand_values(v: &str) -> Vec<&str> {
    let mut parts = Vec::new();
    let mut depth = 0usize;
    let mut start = None;
    for (i, ch) in v.char_indices() {
        match ch {
            '(' => { depth += 1; if start.is_none() { start = Some(i); } }
            ')' => { depth = depth.saturating_sub(1); }
            _ if ch.is_whitespace() && depth == 0 => {
                if let Some(s) = start {
                    parts.push(&v[s..i]);
                    start = None;
                }
            }
            _ => { if start.is_none() { start = Some(i); } }
        }
    }
    if let Some(s) = start {
        parts.push(&v[s..]);
    }
    parts
}

pub fn apply_shorthand_4<F: Fn(&str) -> CssLength>(
    v: &str,
    top: &mut CssLength, right: &mut CssLength,
    bottom: &mut CssLength, left: &mut CssLength,
    parse: F,
) {
    // Split on whitespace but keep calc()/clamp()/var() intact
    let parts = split_css_values(v);
    match parts.len() {
        1 => { let x = parse(&parts[0]); *top = x.clone(); *right = x.clone(); *bottom = x.clone(); *left = x; }
        2 => { let tb = parse(&parts[0]); let rl = parse(&parts[1]); *top = tb.clone(); *bottom = tb; *right = rl.clone(); *left = rl; }
        3 => { *top = parse(&parts[0]); let rl = parse(&parts[1]); *right = rl.clone(); *left = rl; *bottom = parse(&parts[2]); }
        4 => { *top = parse(&parts[0]); *right = parse(&parts[1]); *bottom = parse(&parts[2]); *left = parse(&parts[3]); }
        _ => {}
    }
}

pub fn apply_border_shorthand(style: &mut ComputedStyle, v: &str) {
    // border: <width> <style> <color>
    // Split on whitespace but keep calc()/var() expressions intact
    for part in split_css_values(v).iter() {
        let part = part.as_str();
        if let Some(bs) = try_parse_border_style(part) {
            style.border_top_style    = bs;
            style.border_right_style  = bs;
            style.border_bottom_style = bs;
            style.border_left_style   = bs;
        } else if let Some(c) = parse_color(part) {
            style.border_top_color    = c;
            style.border_right_color  = c;
            style.border_bottom_color = c;
            style.border_left_color   = c;
        } else {
            let w = parse_length(part);
            if !matches!(w, CssLength::Auto) {
                style.border_top_width    = w.clone();
                style.border_right_width  = w.clone();
                style.border_bottom_width = w.clone();
                style.border_left_width   = w;
            }
        }
    }
}

pub fn apply_border_side_shorthand(
    v:     &str,
    width: &mut CssLength,
    style: &mut BorderStyle,
    color: &mut Color,
) {
    for part in split_css_values(v).iter() {
        let part = part.as_str();
        if let Some(bs) = try_parse_border_style(part) {
            *style = bs;
        } else if let Some(c) = parse_color(part) {
            *color = c;
        } else {
            let w = parse_length(part);
            if !matches!(w, CssLength::Auto) {
                *width = w;
            }
        }
    }
}

pub fn extract_url(v: &str) -> Option<String> {
    let lower = v.to_lowercase();
    let start = lower.find("url(")?;
    let inner = v[start + 4..].trim();
    let inner = inner.trim_start_matches('"').trim_start_matches('\'');
    let end = inner.find(|c| c == ')' || c == '"' || c == '\'')?;
    Some(inner[..end].to_string())
}

/// Resolve all `url()` references in CSS text relative to the CSS file's URL.
/// This ensures that `url('../image.jpg')` in an external stylesheet is resolved
/// relative to the stylesheet, not the HTML document.
pub fn resolve_css_urls(css: &str, css_base_url: &str) -> String {
    if css_base_url.is_empty() || !css_base_url.contains("://") {
        return css.to_string();
    }
    // Find the directory of the CSS file URL
    let css_dir = if let Some(last_slash) = css_base_url.rfind('/') {
        &css_base_url[..=last_slash]
    } else {
        css_base_url
    };

    let mut result = String::with_capacity(css.len());
    let mut remaining = css;
    while let Some(url_start) = remaining.to_lowercase().find("url(") {
        // Copy everything before url(
        result.push_str(&remaining[..url_start]);
        let after_url = &remaining[url_start + 4..];
        let _inner = after_url.trim_start();
        // Find the closing )
        let mut depth = 1;
        let mut end_idx = 0;
        for (i, ch) in after_url.char_indices() {
            if ch == '(' { depth += 1; }
            if ch == ')' { depth -= 1; if depth == 0 { end_idx = i; break; } }
        }
        if end_idx == 0 {
            // Malformed — just copy as-is
            result.push_str(&remaining[url_start..]);
            break;
        }
        let url_content = after_url[..end_idx].trim();
        let url_content = url_content.trim_matches('"').trim_matches('\'');

        // Only resolve relative URLs (not absolute, data:, or already-resolved)
        let resolved = if url_content.contains("://") || url_content.starts_with("data:") || url_content.starts_with('/') {
            url_content.to_string()
        } else {
            format!("{}{}", css_dir, url_content)
        };

        result.push_str(&format!("url('{}')", resolved));
        remaining = &after_url[end_idx + 1..];
    }
    result.push_str(remaining);
    result
}

/// Parse a CSS shadow value: `offset_x offset_y [blur] [color]`
/// Returns (offset_x, offset_y, blur_radius, color).
pub fn parse_shadow_value(v: &str) -> (f32, f32, f32, Color) {
    let mut nums: Vec<f32> = Vec::new();
    let mut color = Color { r: 0, g: 0, b: 0, a: 255 };
    // Use paren-aware splitting so rgba(r, g, b, a) stays as one token
    let tokens = split_paren_aware(v);
    for tok in &tokens {
        let t = tok.trim();
        if t.is_empty() { continue; }
        if let Some(c) = parse_color(t) {
            color = c;
        } else if let Ok(n) = t.trim_end_matches("px").parse::<f32>() {
            nums.push(n);
        }
    }
    let ox   = nums.first().copied().unwrap_or(0.0);
    let oy   = nums.get(1).copied().unwrap_or(0.0);
    let blur = nums.get(2).copied().unwrap_or(0.0);
    (ox, oy, blur, color)
}

/// Split a string on whitespace, but keep parenthesized groups together.
/// e.g. "2px 2px rgba(0, 0, 0, 0.5)" → ["2px", "2px", "rgba(0, 0, 0, 0.5)"]
fn split_paren_aware(s: &str) -> Vec<String> {
    let mut tokens = Vec::new();
    let mut current = String::new();
    let mut depth = 0usize;
    for c in s.chars() {
        match c {
            '(' => { depth += 1; current.push(c); }
            ')' => { if depth > 0 { depth -= 1; } current.push(c); }
            ' ' | '\t' if depth == 0 => {
                if !current.is_empty() {
                    tokens.push(std::mem::take(&mut current));
                }
            }
            _ => { current.push(c); }
        }
    }
    if !current.is_empty() {
        tokens.push(current);
    }
    tokens
}

/// Find the byte index of a space that is not nested inside parentheses.
pub fn find_split_space(v: &str) -> Option<usize> {
    let mut depth = 0usize;
    for (i, c) in v.char_indices() {
        match c {
            '(' => depth += 1,
            ')' => { if depth > 0 { depth -= 1; } }
            ' ' if depth == 0 => return Some(i),
            _ => {}
        }
    }
    None
}

pub fn apply_gradient(style: &mut ComputedStyle, v: &str) {
    let lower = v.to_lowercase();
    if lower.contains("linear-gradient") {
        style.gradient_type = GradientType::Linear;
        // Parse angle from "to bottom" or degrees
        if let Some(paren) = v.find('(') {
            let inner = &v[paren + 1..];
            let inner = inner.trim_end_matches(')');
            let first_comma = inner.find(',').unwrap_or(inner.len());
            let dir = inner[..first_comma].trim();
            if dir.ends_with("deg") {
                style.gradient_angle = dir[..dir.len()-3].parse().unwrap_or(180.0);
            } else if dir == "to bottom" || dir == "to top" {
                style.gradient_angle = if dir == "to bottom" { 180.0 } else { 0.0 };
            } else if dir == "to right" {
                style.gradient_angle = 90.0;
            } else if dir == "to left" {
                style.gradient_angle = 270.0;
            }
            // Parse color stops
            let stops_str = if first_comma < inner.len() { &inner[first_comma + 1..] } else { "" };
            style.rare_mut().gradient_stops.clear();
            let n_stops = stops_str.split(',').count().max(1) as f32;
            for (i, stop) in stops_str.split(',').enumerate() {
                let stop = stop.trim();
                // Each stop may be "color position%"
                let mut parts = stop.splitn(2, ' ');
                let color_str = parts.next().unwrap_or(stop);
                if let Some(c) = parse_color(color_str) {
                    let pos = parts.next()
                        .and_then(|p| p.trim_end_matches('%').parse::<f32>().ok())
                        .map(|p| p / 100.0)
                        .unwrap_or(i as f32 / (n_stops - 1.0).max(1.0));
                    style.rare_mut().gradient_stops.push(GradientStop { color: c, position: pos });
                }
            }
        }
    } else if lower.contains("radial-gradient") {
        style.gradient_type = GradientType::Radial;
        if let Some(paren) = v.find('(') {
            let inner = &v[paren + 1..];
            let inner = inner.trim_end_matches(')');
            style.rare_mut().gradient_stops.clear();
            // Skip the optional shape/size/position descriptor before the first comma
            // (e.g. "circle at 50% 50%", "ellipse farthest-corner", "closest-side").
            // If the first comma-delimited segment doesn't parse as a color, treat it as a descriptor.
            let first_comma = inner.find(',').unwrap_or(inner.len());
            let first_token = inner[..first_comma].trim();
            let stops_str = if parse_color(first_token).is_none() && first_comma < inner.len() {
                &inner[first_comma + 1..]
            } else {
                inner
            };
            let stops: Vec<&str> = stops_str.split(',').collect();
            let n_stops = stops.len() as f32;
            for (i, stop) in stops.iter().enumerate() {
                let stop = stop.trim();
                // Each stop is "color" or "color position%"
                let mut parts = stop.splitn(2, ' ');
                let color_str = parts.next().unwrap_or(stop);
                if let Some(c) = parse_color(color_str) {
                    let pos = parts.next()
                        .and_then(|p| p.trim().trim_end_matches('%').parse::<f32>().ok())
                        .map(|p| p / 100.0)
                        .unwrap_or(i as f32 / (n_stops - 1.0).max(1.0));
                    style.rare_mut().gradient_stops.push(GradientStop { color: c, position: pos });
                }
            }
        }
    }
}

/// Return true if a token looks like a font-size value (keyword or length unit).
fn is_font_size_token(tok: &str) -> bool {
    matches!(tok, "xx-small"|"x-small"|"small"|"medium"|"large"|"x-large"|"xx-large"|"smaller"|"larger")
    || tok.ends_with("px") || tok.ends_with("em") || tok.ends_with("rem")
    || tok.ends_with('%') || tok.ends_with("pt") || tok.ends_with("vw") || tok.ends_with("vh")
}

pub fn apply_font_shorthand(style: &mut ComputedStyle, v: &str) {
    // CSS font shorthand: [style] [variant] [weight] [stretch] size[/line-height] family-list
    // System font keywords (single-token):
    if matches!(v, "caption"|"icon"|"menu"|"message-box"|"small-caption"|"status-bar") {
        return; // Use UA defaults; no overrides.
    }

    // Tokenise the shorthand, respecting quoted strings in the family part.
    // Split by whitespace for the pre-family part; the family is everything after size.
    let v = v.trim();
    let mut size_found_at: Option<usize> = None; // byte offset in v
    let mut byte_pos = 0usize;

    // Walk tokens to find the font-size token (first length/keyword that can be a size).
    for tok in v.split_whitespace() {
        // Handle "size/line-height" as a single token.
        let size_tok = if tok.contains('/') {
            tok.splitn(2, '/').next().unwrap_or(tok)
        } else {
            tok
        };

        if is_font_size_token(size_tok) {
            // Parse size (and optional /line-height).
            if tok.contains('/') {
                let mut parts = tok.splitn(2, '/');
                style.font_size = parse_font_size(parts.next().unwrap_or(""));
                if let Some(lh) = parts.next() { style.line_height = parse_line_height(lh); }
            } else {
                style.font_size = parse_font_size(tok);
            }
            size_found_at = Some(byte_pos + tok.len());
            break;
        }

        // Pre-size tokens: style / variant / weight.
        match tok {
            "italic"  | "oblique"   => {
                style.font_style = if tok == "italic" { FontStyle::Italic } else { FontStyle::Oblique };
            }
            "small-caps"            => { style.small_caps = true; }
            "bold"                  => { style.font_weight = FontWeight::Bold; }
            "bolder"                => { style.font_weight = FontWeight::Value(700); }
            "lighter"               => { style.font_weight = FontWeight::Value(300); }
            "normal"                => {}
            s if s.parse::<u16>().is_ok() => {
                style.font_weight = FontWeight::Value(s.parse().unwrap());
            }
            _ => {}
        }
        byte_pos += tok.len() + 1; // +1 for the space
    }

    // Everything after the size (and optional /lh) token is the font-family list.
    if let Some(after) = size_found_at {
        let family_part = v[after..].trim();
        // Strip any leading /line-height that wasn't part of the size token.
        let family_part = if family_part.starts_with('/') {
            let rest = family_part[1..].trim();
            // The line-height value is the next whitespace-separated token.
            let mut it = rest.splitn(2, char::is_whitespace);
            let lh_tok = it.next().unwrap_or("");
            style.line_height = parse_line_height(lh_tok);
            it.next().unwrap_or("").trim()
        } else {
            family_part
        };
        if !family_part.is_empty() {
            // Normalize system-font keywords in the family list.
            let normalized = split_font_families(family_part)
                .into_iter()
                .map(|name| {
                    resolve_system_font_keyword(&name)
                        .map(|s| s.to_string())
                        .unwrap_or(name)
                })
                .collect::<Vec<_>>()
                .join(", ");
            style.font_family = normalized;
        }
    }
}
