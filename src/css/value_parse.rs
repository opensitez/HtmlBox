//! CSS value parsers — lengths, colours, shorthands.

#![allow(unused_imports)]
use std::collections::{HashMap, HashSet};
use rayon::prelude::*;
use crate::types::*;
use super::*;

// ─── Value Parsers ────────────────────────────────────────────────────────────

/// Cache for parsed CSS length values — avoids re-parsing the same string
/// (e.g. "100%" or "calc(100% - 21.5rem)") thousands of times during cascade.
static LENGTH_CACHE: std::sync::LazyLock<std::sync::Mutex<HashMap<String, CssLength>>> =
    std::sync::LazyLock::new(|| std::sync::Mutex::new(HashMap::new()));

pub fn parse_length(v: &str) -> CssLength {
    let v = v.trim();
    if v == "auto"       { return CssLength::Auto; }
    if v == "0"          { return CssLength::Zero; }
    // Check cache for previously parsed values
    if let Ok(cache) = LENGTH_CACHE.lock() {
        if let Some(cached) = cache.get(v) {
            return cached.clone();
        }
    }
    let result = parse_length_inner(v);
    // Cache the result (only for non-trivial values)
    if v.len() > 4 {
        if let Ok(mut cache) = LENGTH_CACHE.lock() {
            if cache.len() < 50000 {
                cache.insert(v.to_string(), result.clone());
            }
        }
    }
    result
}

fn parse_length_inner(v: &str) -> CssLength {
    if let Some(inner) = v.strip_prefix("calc(").and_then(|s| s.strip_suffix(')')) {
        return parse_calc(inner);
    }
    // CSS min()/max()/clamp() — proper AST with lazy resolution at layout time
    if let Some(inner) = v.strip_prefix("min(").and_then(|s| s.strip_suffix(')')) {
        let args = split_top_level_commas(inner);
        if args.len() >= 2 {
            let vals: Vec<CssLength> = args.iter().map(|a| parse_length(a.trim())).collect();
            return CssLength::Min(Box::new(vals));
        }
        return parse_length(inner);
    }
    if let Some(inner) = v.strip_prefix("max(").and_then(|s| s.strip_suffix(')')) {
        let args = split_top_level_commas(inner);
        if args.len() >= 2 {
            let vals: Vec<CssLength> = args.iter().map(|a| parse_length(a.trim())).collect();
            return CssLength::Max(Box::new(vals));
        }
        return parse_length(inner);
    }
    if let Some(inner) = v.strip_prefix("clamp(").and_then(|s| s.strip_suffix(')')) {
        let args = split_top_level_commas(inner);
        if args.len() == 3 {
            let min = parse_length(args[0].trim());
            let val = parse_length(args[1].trim());
            let max = parse_length(args[2].trim());
            return CssLength::Clamp(Box::new([min, val, max]));
        }
        // Fallback: treat as calc
        return parse_length(inner);
    }
    if v.ends_with("px") { return CssLength::Px(v[..v.len()-2].parse().unwrap_or(0.0)); }
    if v.ends_with("rem") { return CssLength::Rem(v[..v.len()-3].parse().unwrap_or(0.0)); }
    if v.ends_with("em") { return CssLength::Em(v[..v.len()-2].parse().unwrap_or(0.0)); }
    if v.ends_with('%')  { return CssLength::Percent(v[..v.len()-1].parse().unwrap_or(0.0)); }
    if v.ends_with("pt") { return CssLength::Px(v[..v.len()-2].parse::<f32>().unwrap_or(0.0) * 4.0 / 3.0); }
    if v.ends_with("vw") { return CssLength::Vw(v[..v.len()-2].parse().unwrap_or(0.0)); }
    if v.ends_with("vh") { return CssLength::Vh(v[..v.len()-2].parse().unwrap_or(0.0)); }
    if v.ends_with("vmin") { let n = v[..v.len()-4].parse::<f32>().unwrap_or(0.0); return CssLength::Vw(n); } // approx
    if v.ends_with("vmax") { let n = v[..v.len()-4].parse::<f32>().unwrap_or(0.0); return CssLength::Vw(n); } // approx
    // Unitless number (treat as px for simplicity)
    if let Ok(n) = v.parse::<f32>() { return CssLength::Px(n); }
    CssLength::Auto
}












/// Find the index of the closing `)` that matches the opening `(` at position 0.
/// Split a string at top-level commas (not inside nested parentheses).
fn split_top_level_commas(s: &str) -> Vec<&str> {
    let mut parts = Vec::new();
    let mut depth = 0usize;
    let mut start = 0;
    for (i, c) in s.char_indices() {
        match c {
            '(' => depth += 1,
            ')' => { if depth > 0 { depth -= 1; } }
            ',' if depth == 0 => {
                parts.push(&s[start..i]);
                start = i + 1;
            }
            _ => {}
        }
    }
    parts.push(&s[start..]);
    parts
}

/// Split a CSS value on whitespace while keeping parenthesized expressions
/// (calc, var, min, max, clamp, rgb, etc.) intact as single tokens.
pub fn split_css_shorthand_values(s: &str) -> Vec<String> {
    split_css_values(s)
}
pub(crate) fn split_css_values(s: &str) -> Vec<String> {
    let mut parts = Vec::new();
    let mut depth = 0usize;
    let mut current = String::new();
    for c in s.chars() {
        match c {
            '(' => { depth += 1; current.push(c); }
            ')' => { if depth > 0 { depth -= 1; } current.push(c); }
            c if c.is_ascii_whitespace() && depth == 0 => {
                let trimmed = current.trim().to_string();
                if !trimmed.is_empty() { parts.push(trimmed); }
                current.clear();
            }
            _ => { current.push(c); }
        }
    }
    let trimmed = current.trim().to_string();
    if !trimmed.is_empty() { parts.push(trimmed); }
    parts
}

pub fn parse_length_or_none(v: &str) -> CssLength {
    if v == "none" { CssLength::None } else { parse_length(v) }
}

pub fn parse_font_size(v: &str) -> CssLength {
    match v {
        "xx-small" => CssLength::Px(9.0),
        "x-small"  => CssLength::Px(10.0),
        "small"    => CssLength::Px(13.0),
        "medium"   => CssLength::Px(16.0),
        "large"    => CssLength::Px(18.0),
        "x-large"  => CssLength::Px(24.0),
        "xx-large" => CssLength::Px(32.0),
        "smaller"  => CssLength::Em(0.83),
        "larger"   => CssLength::Em(1.17),
        _          => parse_length(v),
    }
}

pub fn parse_line_height(v: &str) -> CssLength {
    if v == "normal" { return CssLength::Em(1.2); }
    // Unitless number: treat as em
    if let Ok(n) = v.parse::<f32>() { return CssLength::Em(n); }
    parse_length(v)
}

pub fn parse_overflow(v: &str) -> Overflow {
    match v {
        "hidden" => Overflow::Hidden,
        "scroll" => Overflow::Scroll,
        "auto"   => Overflow::Auto,
        _        => Overflow::Visible,
    }
}

pub fn parse_overscroll(v: &str) -> OverscrollBehavior {
    match v.trim() {
        "none"    => OverscrollBehavior::None,
        "contain" => OverscrollBehavior::Contain,
        _         => OverscrollBehavior::Auto,
    }
}

pub fn parse_border_style(v: &str) -> BorderStyle {
    try_parse_border_style(v).unwrap_or(BorderStyle::None)
}

pub(crate) fn try_parse_border_style(v: &str) -> Option<BorderStyle> {
    Some(match v {
        "none"   => BorderStyle::None,
        "hidden" => BorderStyle::Hidden,
        "solid"  => BorderStyle::Solid,
        "dashed" => BorderStyle::Dashed,
        "dotted" => BorderStyle::Dotted,
        "double" => BorderStyle::Double,
        "groove" => BorderStyle::Groove,
        "ridge"  => BorderStyle::Ridge,
        "inset"  => BorderStyle::Inset,
        "outset" => BorderStyle::Outset,
        _        => return None,
    })
}

/// Parse a CSS color value into a `Color`.
pub fn parse_color(v: &str) -> Option<Color> {
    let v = v.trim();

    // light-dark(light, dark) — use light value (we render in light mode)
    if v.starts_with("light-dark(") {
        if let Some(inner) = v.strip_prefix("light-dark(").and_then(|s| s.strip_suffix(')')) {
            let comma = inner.find(',')?;
            return parse_color(inner[..comma].trim());
        }
    }

    // Named colors
    let named = match v {
        "black"    => Some(Color::rgb(0, 0, 0)),
        "white"    => Some(Color::rgb(255, 255, 255)),
        "red"            => Some(Color::rgb(255,   0,   0)),
        "green"          => Some(Color::rgb(  0, 128,   0)),
        "blue"           => Some(Color::rgb(  0,   0, 255)),
        "yellow"         => Some(Color::rgb(255, 255,   0)),
        "orange"         => Some(Color::rgb(255, 165,   0)),
        "purple"         => Some(Color::rgb(128,   0, 128)),
        "pink"           => Some(Color::rgb(255, 192, 203)),
        "gray" | "grey"  => Some(Color::rgb(128, 128, 128)),
        "darkgray" | "darkgrey"   => Some(Color::rgb(169, 169, 169)),
        "lightgray"| "lightgrey"  => Some(Color::rgb(211, 211, 211)),
        "darkslategray" | "darkslategrey" => Some(Color::rgb( 47,  79,  79)),
        "slategray" | "slategrey" => Some(Color::rgb(112, 128, 144)),
        "lightslategray" | "lightslategrey" => Some(Color::rgb(119, 136, 153)),
        "dimgray" | "dimgrey"     => Some(Color::rgb(105, 105, 105)),
        "gainsboro"      => Some(Color::rgb(220, 220, 220)),
        "whitesmoke"     => Some(Color::rgb(245, 245, 245)),
        "silver"         => Some(Color::rgb(192, 192, 192)),
        "navy"           => Some(Color::rgb(  0,   0, 128)),
        "teal"           => Some(Color::rgb(  0, 128, 128)),
        "aqua" | "cyan"  => Some(Color::rgb(  0, 255, 255)),
        "fuchsia" | "magenta" => Some(Color::rgb(255, 0, 255)),
        "maroon"         => Some(Color::rgb(128,   0,   0)),
        "olive"          => Some(Color::rgb(128, 128,   0)),
        "lime"           => Some(Color::rgb(  0, 255,   0)),
        "darkred"        => Some(Color::rgb(139,   0,   0)),
        "darkgreen"      => Some(Color::rgb(  0, 100,   0)),
        "darkblue"       => Some(Color::rgb(  0,   0, 139)),
        "darkcyan"       => Some(Color::rgb(  0, 139, 139)),
        "darkmagenta"    => Some(Color::rgb(139,   0, 139)),
        "darkorange"     => Some(Color::rgb(255, 140,   0)),
        "darkviolet"     => Some(Color::rgb(148,   0, 211)),
        "darkgoldenrod"  => Some(Color::rgb(184, 134,  11)),
        "darkkhaki"      => Some(Color::rgb(189, 183, 107)),
        "darkturquoise"  => Some(Color::rgb(  0, 206, 209)),
        "darkorchid"     => Some(Color::rgb(153,  50, 204)),
        "darkseagreen"   => Some(Color::rgb(143, 188, 143)),
        "darksalmon"     => Some(Color::rgb(233, 150, 122)),
        "indigo"         => Some(Color::rgb( 75,   0, 130)),
        "violet"         => Some(Color::rgb(238, 130, 238)),
        "orchid"         => Some(Color::rgb(218, 112, 214)),
        "plum"           => Some(Color::rgb(221, 160, 221)),
        "thistle"        => Some(Color::rgb(216, 191, 216)),
        "lavender"       => Some(Color::rgb(230, 230, 250)),
        "mediumpurple"   => Some(Color::rgb(147, 112, 219)),
        "blueviolet"     => Some(Color::rgb(138,  43, 226)),
        "rebeccapurple"  => Some(Color::rgb(102,  51, 153)),
        "mediumblue"     => Some(Color::rgb(  0,   0, 205)),
        "royalblue"      => Some(Color::rgb( 65, 105, 225)),
        "cornflowerblue" => Some(Color::rgb(100, 149, 237)),
        "deepskyblue"    => Some(Color::rgb(  0, 191, 255)),
        "dodgerblue"     => Some(Color::rgb( 30, 144, 255)),
        "lightblue"      => Some(Color::rgb(173, 216, 230)),
        "lightskyblue"   => Some(Color::rgb(135, 206, 250)),
        "skyblue"        => Some(Color::rgb(135, 206, 235)),
        "steelblue"      => Some(Color::rgb( 70, 130, 180)),
        "cadetblue"      => Some(Color::rgb( 95, 158, 160)),
        "powderblue"     => Some(Color::rgb(176, 224, 230)),
        "lightcyan"      => Some(Color::rgb(224, 255, 255)),
        "paleturquoise"  => Some(Color::rgb(175, 238, 238)),
        "mediumturquoise"=> Some(Color::rgb( 72, 209, 204)),
        "turquoise"      => Some(Color::rgb( 64, 224, 208)),
        "aquamarine"     => Some(Color::rgb(127, 255, 212)),
        "mediumaquamarine" => Some(Color::rgb(102, 205, 170)),
        "lightgreen"     => Some(Color::rgb(144, 238, 144)),
        "limegreen"      => Some(Color::rgb( 50, 205,  50)),
        "mediumseagreen" => Some(Color::rgb( 60, 179, 113)),
        "seagreen"       => Some(Color::rgb( 46, 139,  87)),
        "forestgreen"    => Some(Color::rgb( 34, 139,  34)),
        "olivedrab"      => Some(Color::rgb(107, 142,  35)),
        "yellowgreen"    => Some(Color::rgb(154, 205,  50)),
        "chartreuse"     => Some(Color::rgb(127, 255,   0)),
        "lawngreen"      => Some(Color::rgb(124, 252,   0)),
        "greenyellow"    => Some(Color::rgb(173, 255,  47)),
        "palegreen"      => Some(Color::rgb(152, 251, 152)),
        "springgreen"    => Some(Color::rgb(  0, 255, 127)),
        "mediumspringgreen" => Some(Color::rgb(0, 250, 154)),
        "mintcream"      => Some(Color::rgb(245, 255, 250)),
        "honeydew"       => Some(Color::rgb(240, 255, 240)),
        "lemonchiffon"   => Some(Color::rgb(255, 250, 205)),
        "gold"           => Some(Color::rgb(255, 215,   0)),
        "goldenrod"      => Some(Color::rgb(218, 165,  32)),
        "palegoldenrod"  => Some(Color::rgb(238, 232, 170)),
        "wheat"          => Some(Color::rgb(245, 222, 179)),
        "moccasin"       => Some(Color::rgb(255, 228, 181)),
        "navajowhite"    => Some(Color::rgb(255, 222, 173)),
        "peachpuff"      => Some(Color::rgb(255, 218, 185)),
        "bisque"         => Some(Color::rgb(255, 228, 196)),
        "blanchedalmond" => Some(Color::rgb(255, 235, 205)),
        "papayawhip"     => Some(Color::rgb(255, 239, 213)),
        "antiquewhite"   => Some(Color::rgb(250, 235, 215)),
        "cornsilk"       => Some(Color::rgb(255, 248, 220)),
        "oldlace"        => Some(Color::rgb(253, 245, 230)),
        "floralwhite"    => Some(Color::rgb(255, 250, 240)),
        "ivory"          => Some(Color::rgb(255, 255, 240)),
        "beige"          => Some(Color::rgb(245, 245, 220)),
        "khaki"          => Some(Color::rgb(240, 230, 140)),
        "tan"            => Some(Color::rgb(210, 180, 140)),
        "burlywood"      => Some(Color::rgb(222, 184, 135)),
        "sandybrown"     => Some(Color::rgb(244, 164,  96)),
        "peru"           => Some(Color::rgb(205, 133,  63)),
        "chocolate"      => Some(Color::rgb(210, 105,  30)),
        "sienna"         => Some(Color::rgb(160,  82,  45)),
        "saddlebrown"    => Some(Color::rgb(139,  69,  19)),
        "brown"          => Some(Color::rgb(165,  42,  42)),
        "firebrick"      => Some(Color::rgb(178,  34,  34)),
        "crimson"        => Some(Color::rgb(220,  20,  60)),
        "tomato"         => Some(Color::rgb(255,  99,  71)),
        "coral"          => Some(Color::rgb(255, 127,  80)),
        "salmon"         => Some(Color::rgb(250, 128, 114)),
        "lightsalmon"    => Some(Color::rgb(255, 160, 122)),
        "lightcoral"     => Some(Color::rgb(240, 128, 128)),
        "lightyellow"    => Some(Color::rgb(255, 255, 224)),
        "lightgoldenrodyellow" => Some(Color::rgb(250, 250, 210)),
        "lightseagreen"  => Some(Color::rgb( 32, 178, 170)),
        "lightsteelblue" => Some(Color::rgb(176, 196, 222)),
        "orangered"      => Some(Color::rgb(255,  69,   0)),
        "hotpink"        => Some(Color::rgb(255, 105, 180)),
        "deeppink"       => Some(Color::rgb(255,  20, 147)),
        "lightpink"      => Some(Color::rgb(255, 182, 193)),
        "palevioletred"  => Some(Color::rgb(219, 112, 147)),
        "mediumvioletred"=> Some(Color::rgb(199,  21, 133)),
        "rosybrown"      => Some(Color::rgb(188, 143, 143)),
        "mistyrose"      => Some(Color::rgb(255, 228, 225)),
        "lavenderblush"  => Some(Color::rgb(255, 240, 245)),
        "aliceblue"      => Some(Color::rgb(240, 248, 255)),
        "ghostwhite"     => Some(Color::rgb(248, 248, 255)),
        "azure"          => Some(Color::rgb(240, 255, 255)),
        "snow"           => Some(Color::rgb(255, 250, 250)),
        "seashell"       => Some(Color::rgb(255, 245, 238)),
        "linen"          => Some(Color::rgb(250, 240, 230)),
        "transparent"    => Some(Color::TRANSPARENT),
        "currentcolor"   => None,  // can't resolve without context
        _ => None,
    };
    if named.is_some() { return named; }

    // #rrggbb or #rgb
    if v.starts_with('#') {
        let hex = &v[1..];
        return match hex.len() {
            6 => {
                let r = u8::from_str_radix(&hex[0..2], 16).ok()?;
                let g = u8::from_str_radix(&hex[2..4], 16).ok()?;
                let b = u8::from_str_radix(&hex[4..6], 16).ok()?;
                Some(Color::rgb(r, g, b))
            }
            8 => {
                let r = u8::from_str_radix(&hex[0..2], 16).ok()?;
                let g = u8::from_str_radix(&hex[2..4], 16).ok()?;
                let b = u8::from_str_radix(&hex[4..6], 16).ok()?;
                let a = u8::from_str_radix(&hex[6..8], 16).ok()?;
                Some(Color::rgba(r, g, b, a))
            }
            3 => {
                let r = u8::from_str_radix(&hex[0..1], 16).ok()? * 17;
                let g = u8::from_str_radix(&hex[1..2], 16).ok()? * 17;
                let b = u8::from_str_radix(&hex[2..3], 16).ok()? * 17;
                Some(Color::rgb(r, g, b))
            }
            4 => {
                let r = u8::from_str_radix(&hex[0..1], 16).ok()? * 17;
                let g = u8::from_str_radix(&hex[1..2], 16).ok()? * 17;
                let b = u8::from_str_radix(&hex[2..3], 16).ok()? * 17;
                let a = u8::from_str_radix(&hex[3..4], 16).ok()? * 17;
                Some(Color::rgba(r, g, b, a))
            }
            _ => None,
        };
    }

    // rgb(r, g, b) or rgba(r, g, b, a)
    if v.starts_with("rgb") {
        let inner = v.trim_start_matches("rgba").trim_start_matches("rgb")
            .trim_start_matches('(').trim_end_matches(')');
        let parts: Vec<&str> = inner.split(',').collect();
        if parts.len() >= 3 {
            let parse_channel = |s: &str| -> u8 {
                let s = s.trim();
                if s.ends_with('%') {
                    (s[..s.len()-1].parse::<f32>().unwrap_or(0.0) / 100.0 * 255.0) as u8
                } else {
                    s.parse::<f32>().unwrap_or(0.0).round() as u8
                }
            };
            let r = parse_channel(parts[0]);
            let g = parse_channel(parts[1]);
            let b = parse_channel(parts[2]);
            let a = if parts.len() >= 4 {
                (parts[3].trim().parse::<f32>().unwrap_or(1.0) * 255.0) as u8
            } else {
                255
            };
            return Some(Color::rgba(r, g, b, a));
        }
    }

    // hsl (simplified: convert hsl to rgb)
    if v.starts_with("hsl") {
        let inner = v.trim_start_matches("hsla").trim_start_matches("hsl")
            .trim_start_matches('(').trim_end_matches(')');
        let parts: Vec<&str> = inner.split(',').collect();
        if parts.len() >= 3 {
            let h = parts[0].trim().parse::<f32>().unwrap_or(0.0) / 360.0;
            let s = parts[1].trim().trim_end_matches('%').parse::<f32>().unwrap_or(0.0) / 100.0;
            let l = parts[2].trim().trim_end_matches('%').parse::<f32>().unwrap_or(0.0) / 100.0;
            let a = if parts.len() >= 4 {
                (parts[3].trim().parse::<f32>().unwrap_or(1.0) * 255.0) as u8
            } else {
                255
            };
            let (r, g, b) = hsl_to_rgb(h, s, l);
            return Some(Color::rgba((r * 255.0) as u8, (g * 255.0) as u8, (b * 255.0) as u8, a));
        }
    }

    None
}

fn hsl_to_rgb(h: f32, s: f32, l: f32) -> (f32, f32, f32) {
    if s == 0.0 { return (l, l, l); }
    let q = if l < 0.5 { l * (1.0 + s) } else { l + s - l * s };
    let p = 2.0 * l - q;
    let r = hue_to_rgb(p, q, h + 1.0 / 3.0);
    let g = hue_to_rgb(p, q, h);
    let b = hue_to_rgb(p, q, h - 1.0 / 3.0);
    (r, g, b)
}

fn hue_to_rgb(p: f32, q: f32, mut t: f32) -> f32 {
    if t < 0.0 { t += 1.0; }
    if t > 1.0 { t -= 1.0; }
    if t < 1.0 / 6.0 { return p + (q - p) * 6.0 * t; }
    if t < 1.0 / 2.0 { return q; }
    if t < 2.0 / 3.0 { return p + (q - p) * (2.0 / 3.0 - t) * 6.0; }
    p
}
