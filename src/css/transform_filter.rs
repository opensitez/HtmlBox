// CSS Transform and Filter parsing functions
use crate::types::*;
use super::parse_color;

fn parse_angle_deg(s: &str) -> f32 {
    let s = s.trim();
    if let Some(v) = s.strip_suffix("deg") { v.parse::<f32>().unwrap_or(0.0) }
    else if let Some(v) = s.strip_suffix("rad") { v.parse::<f32>().unwrap_or(0.0) * 180.0 / std::f32::consts::PI }
    else if let Some(v) = s.strip_suffix("turn") { v.parse::<f32>().unwrap_or(0.0) * 360.0 }
    else if let Some(v) = s.strip_suffix("grad") { v.parse::<f32>().unwrap_or(0.0) * 0.9 }
    else { s.parse::<f32>().unwrap_or(0.0) }
}

pub(crate) fn parse_transform_length_px(s: &str) -> f32 {
    let s = s.trim();
    if let Some(v) = s.strip_suffix("px") { v.parse::<f32>().unwrap_or(0.0) }
    else if s.ends_with('%') { 0.0 }
    else if let Some(v) = s.strip_suffix("em") { v.parse::<f32>().unwrap_or(0.0) * 16.0 }
    else { s.parse::<f32>().unwrap_or(0.0) }
}

pub(crate) fn split_css_args(s: &str) -> Vec<&str> {
    let mut args = Vec::new();
    let mut depth = 0usize;
    let mut start = 0usize;
    let bytes = s.as_bytes();
    for (i, &b) in bytes.iter().enumerate() {
        match b {
            b'(' => depth += 1,
            b')' => { if depth > 0 { depth -= 1; } }
            b',' if depth == 0 => { args.push(s[start..i].trim()); start = i + 1; }
            _ => {}
        }
    }
    let last = s[start..].trim();
    if !last.is_empty() { args.push(last); }
    args
}

pub(crate) fn parse_css_function_token(token: &str) -> Option<(&str, &str)> {
    let paren = token.find('(')?;
    let name = token[..paren].trim();
    let inner = token[paren + 1..].trim_end_matches(')');
    Some((name, inner))
}

pub(crate) fn tokenize_css_functions(v: &str) -> Vec<String> {
    let mut tokens = Vec::new();
    let mut current = String::new();
    let mut depth = 0usize;
    for ch in v.chars() {
        match ch {
            '(' => { depth += 1; current.push(ch); }
            ')' => {
                current.push(ch);
                if depth > 0 { depth -= 1; }
                if depth == 0 {
                    let t = current.trim().to_string();
                    if !t.is_empty() { tokens.push(t); }
                    current = String::new();
                }
            }
            ' ' | '\t' | '\n' if depth == 0 => {
                let t = current.trim().to_string();
                if !t.is_empty() { tokens.push(t); }
                current = String::new();
            }
            _ => { current.push(ch); }
        }
    }
    let t = current.trim().to_string();
    if !t.is_empty() { tokens.push(t); }
    tokens
}

pub fn parse_css_transform(v: &str) -> CssTransform {
    if v.trim() == "none" { return CssTransform::default(); }
    let mut ops = Vec::new();
    for token in tokenize_css_functions(v) {
        if let Some((name, args_str)) = parse_css_function_token(&token) {
            let args: Vec<&str> = split_css_args(args_str);
            match name.to_ascii_lowercase().as_str() {
                "translate" => {
                    let tx = args.first().map(|s| parse_transform_length_px(s)).unwrap_or(0.0);
                    let ty = args.get(1).map(|s| parse_transform_length_px(s)).unwrap_or(0.0);
                    ops.push(TransformOp::Translate(tx, ty));
                }
                "translatex" => { ops.push(TransformOp::TranslateX(args.first().map(|s| parse_transform_length_px(s)).unwrap_or(0.0))); }
                "translatey" => { ops.push(TransformOp::TranslateY(args.first().map(|s| parse_transform_length_px(s)).unwrap_or(0.0))); }
                "scale" => {
                    let sx = args.first().and_then(|s| s.trim().parse::<f32>().ok()).unwrap_or(1.0);
                    let sy = args.get(1).and_then(|s| s.trim().parse::<f32>().ok()).unwrap_or(sx);
                    ops.push(TransformOp::Scale(sx, sy));
                }
                "scalex" => { ops.push(TransformOp::ScaleX(args.first().and_then(|s| s.trim().parse::<f32>().ok()).unwrap_or(1.0))); }
                "scaley" => { ops.push(TransformOp::ScaleY(args.first().and_then(|s| s.trim().parse::<f32>().ok()).unwrap_or(1.0))); }
                "rotate" => { ops.push(TransformOp::Rotate(args.first().map(|s| parse_angle_deg(s)).unwrap_or(0.0))); }
                "skewx" => { ops.push(TransformOp::SkewX(args.first().map(|s| parse_angle_deg(s)).unwrap_or(0.0))); }
                "skewy" => { ops.push(TransformOp::SkewY(args.first().map(|s| parse_angle_deg(s)).unwrap_or(0.0))); }
                "skew" => {
                    let ax = args.first().map(|s| parse_angle_deg(s)).unwrap_or(0.0);
                    let ay = args.get(1).map(|s| parse_angle_deg(s)).unwrap_or(0.0);
                    ops.push(TransformOp::SkewX(ax));
                    if ay != 0.0 { ops.push(TransformOp::SkewY(ay)); }
                }
                "matrix" => {
                    let ns: Vec<f32> = args.iter().map(|s| s.trim().parse::<f32>().unwrap_or(0.0)).collect();
                    ops.push(TransformOp::Matrix(
                        ns.first().copied().unwrap_or(1.0), ns.get(1).copied().unwrap_or(0.0),
                        ns.get(2).copied().unwrap_or(0.0), ns.get(3).copied().unwrap_or(1.0),
                        ns.get(4).copied().unwrap_or(0.0), ns.get(5).copied().unwrap_or(0.0),
                    ));
                }
                _ => {}
            }
        }
    }
    CssTransform { ops }
}

pub fn parse_transform_origin(v: &str) -> (f32, f32) {
    let parts: Vec<&str> = v.split_whitespace().collect();
    let kx = |s: &str| -> Option<f32> { match s { "left" => Some(0.0), "center" => Some(0.5), "right" => Some(1.0), _ => None } };
    let ky = |s: &str| -> Option<f32> { match s { "top" => Some(0.0), "center" => Some(0.5), "bottom" => Some(1.0), _ => None } };
    let pv = |s: &str| -> f32 {
        if let Some(p) = s.strip_suffix('%') { p.parse::<f32>().unwrap_or(50.0) / 100.0 }
        else { s.parse::<f32>().unwrap_or(50.0) / 100.0 }
    };
    match parts.as_slice() {
        [] => (0.5, 0.5),
        [s] => { if let Some(x) = kx(s) { (x, 0.5) } else if let Some(y) = ky(s) { (0.5, y) } else { (pv(s), 0.5) } }
        [first, second, ..] => {
            let ox = kx(first).unwrap_or_else(|| pv(first));
            let oy = ky(second).or_else(|| kx(second).map(|_| 0.5)).unwrap_or_else(|| pv(second));
            (ox, oy)
        }
    }
}

pub fn parse_css_filter(v: &str) -> CssFilters {
    if v.trim() == "none" { return CssFilters::default(); }
    let mut ops = Vec::new();
    for token in tokenize_css_functions(v) {
        if let Some((name, args_str)) = parse_css_function_token(&token) {
            let args: Vec<&str> = split_css_args(args_str);
            let first_f = args.first().map(|s| {
                let s = s.trim();
                if let Some(pct) = s.strip_suffix('%') { pct.parse::<f32>().unwrap_or(0.0) / 100.0 }
                else { s.parse::<f32>().unwrap_or(0.0) }
            }).unwrap_or(0.0);
            match name.to_ascii_lowercase().as_str() {
                "blur" => {
                    let px = args.first().map(|s| {
                        let s = s.trim();
                        if let Some(v2) = s.strip_suffix("px") { v2.parse::<f32>().unwrap_or(0.0) }
                        else { s.parse::<f32>().unwrap_or(0.0) }
                    }).unwrap_or(0.0);
                    if px > 0.0 { ops.push(FilterOp::Blur(px)); }
                }
                "brightness" => { ops.push(FilterOp::Brightness(first_f)); }
                "contrast"   => { ops.push(FilterOp::Contrast(first_f)); }
                "grayscale"  => { ops.push(FilterOp::Grayscale(first_f.min(1.0))); }
                "hue-rotate" => { ops.push(FilterOp::HueRotate(args.first().map(|s| parse_angle_deg(s)).unwrap_or(0.0))); }
                "invert"     => { ops.push(FilterOp::Invert(first_f.min(1.0))); }
                "opacity"    => { ops.push(FilterOp::Opacity(first_f.min(1.0))); }
                "saturate"   => { ops.push(FilterOp::Saturate(first_f)); }
                "sepia"      => { ops.push(FilterOp::Sepia(first_f.min(1.0))); }
                "drop-shadow" => {
                    let dx   = args.first().map(|s| parse_transform_length_px(s)).unwrap_or(0.0);
                    let dy   = args.get(1).map(|s| parse_transform_length_px(s)).unwrap_or(0.0);
                    let blur = args.get(2).map(|s| parse_transform_length_px(s)).unwrap_or(0.0);
                    let color = args.get(3).and_then(|s| parse_color(s)).unwrap_or(Color::BLACK);
                    ops.push(FilterOp::DropShadow { dx, dy, blur, color });
                }
                _ => {}
            }
        }
    }
    CssFilters { ops }
}
