pub mod forms;
pub mod serializer;
pub mod streaming;
pub mod entities;

use std::collections::HashMap;
use crate::types::{WebCore, Document, Display, ListStyleType};
use crate::css::{Stylesheet, apply_property, apply_cascade, ua_stylesheet};

// ─── SVG extraction ────────────────────────────────────────────────────────

/// Pre-pass: extract `<svg>…</svg>` blocks and replace with `<img>` placeholders.
/// Returns (processed_html, map_of_key→svg_markup).
// SVG blocks are now handled inline by the tokenizer (collected as raw text)
// and rasterized by the parser when building the DOM tree.

/// Parse leading integer pixels from a string like "20px", "512", "20px;height:10px".
fn parse_px(s: &str) -> Option<u32> {
    let num: String = s.trim().chars().take_while(|c| c.is_ascii_digit()).collect();
    if num.is_empty() { None } else { num.parse().ok() }
}

/// Extract a CSS pixel value for `prop` from an inline style string (e.g. "width:20px;height:20px").
fn style_px(style: &str, prop: &str) -> Option<u32> {
    let lower = style.to_ascii_lowercase();
    let needle = format!("{}:", prop);
    let idx = lower.find(&needle)?;
    let after = style[idx + needle.len()..].trim_start();
    parse_px(after)
}

/// Parse a viewBox attribute value "min-x min-y width height" → (width, height).
fn parse_viewbox_value(val: Option<&str>) -> Option<(u32, u32)> {
    let val = val?;
    let parts: Vec<f32> = val
        .split(|c: char| c == ',' || c.is_whitespace())
        .filter(|s| !s.is_empty())
        .filter_map(|s| s.parse().ok())
        .collect();
    if parts.len() >= 4 {
        Some((parts[2].round() as u32, parts[3].round() as u32))
    } else {
        None
    }
}

/// Post-pass: walk the box tree, find `<img>` placeholders with `__svg_N__` src,
/// rasterize the SVG to RGBA pixel data, and store it on the box.
/// Post-cascade pass: load background images for elements whose
/// background-image URL was set by CSS rules (not just inline styles).
pub fn load_background_images(node: &mut WebCore, base_url: &str) {
    if node.bg_image_data.is_none() && !node.style.background_image_url.is_empty() {
        let url = node.style.background_image_url.clone();
        if let Some((data, w, h)) = load_image_from_src(&url, base_url) {
            node.bg_image_data   = Some(data);
            node.bg_image_width  = w;
            node.bg_image_height = h;
        }
    }
    // Load mask-image (CSS masking for icons etc.)
    if node.mask_image_data.is_none() && !node.style.mask_image_url.is_empty() {
        let url = node.style.mask_image_url.clone();
        if let Some((data, w, h)) = load_image_from_src(&url, base_url) {
            node.mask_image_data   = Some(data);
            node.mask_image_width  = w;
            node.mask_image_height = h;
        }
    }
    for child in &mut node.children {
        load_background_images(child, base_url);
    }
}


/// Rasterize an SVG string to RGBA pixel data using resvg.
pub fn rasterize_svg_to_rgba(svg: &str, width: u32, height: u32) -> Option<Vec<u8>> {
    use resvg::usvg;
    let opt = usvg::Options::default();
    let tree = match usvg::Tree::from_str(svg, &opt) {
        Ok(t) => t,
        Err(_) => return None,
    };
    let size = tree.size();
    let sx = width as f32 / size.width();
    let sy = height as f32 / size.height();
    let transform = resvg::tiny_skia::Transform::from_scale(sx, sy);
    let mut pixmap = match resvg::tiny_skia::Pixmap::new(width, height) {
        Some(p) => p,
        None => return None,
    };
    resvg::render(&tree, transform, &mut pixmap.as_mut());
    // resvg outputs premultiplied RGBA — same format tiny_skia expects.
    // Return the data directly without un-premultiplying.
    Some(pixmap.data().to_vec())
}

/// Rasterize an SVG at its intrinsic (viewBox) size. Returns (rgba, w, h).
pub fn rasterize_svg_intrinsic(svg: &str) -> Option<(Vec<u8>, u32, u32)> {
    use resvg::usvg;
    let opt = usvg::Options::default();
    let tree = usvg::Tree::from_str(svg, &opt).ok()?;
    let size = tree.size();
    let w = (size.width().ceil() as u32).max(1);
    let h = (size.height().ceil() as u32).max(1);
    rasterize_svg_to_rgba(svg, w, h).map(|rgba| (rgba, w, h))
}

// ─── Charset detection ─────────────────────────────────────────────────────

/// Detect charset from `<meta>` charset declarations and BOM in raw bytes.
fn detect_charset(data: &[u8]) -> &'static str {
    let scan_len = data.len().min(1024);
    let head = &data[..scan_len];

    // Search for charset= in the first 1024 bytes
    let lower: Vec<u8> = head.iter().map(|&b| b.to_ascii_lowercase()).collect();
    if let Some(pos) = find_subsequence(&lower, b"charset") {
        let mut p = pos + 7;
        // Skip whitespace and '='
        while p < scan_len && (lower[p] == b' ' || lower[p] == b'=') { p += 1; }
        // Skip quote
        let quote = if p < scan_len && (head[p] == b'"' || head[p] == b'\'') {
            let q = head[p];
            p += 1;
            Some(q)
        } else {
            None
        };
        let start = p;
        while p < scan_len {
            if let Some(q) = quote {
                if head[p] == q { break; }
            } else if head[p] == b'"' || head[p] == b'\'' || head[p] == b';'
                   || head[p] == b'>' || head[p] == b' ' { break; }
            p += 1;
        }
        if p > start {
            let charset_raw = std::str::from_utf8(&head[start..p]).unwrap_or("");
            let stripped: String = charset_raw.chars()
                .filter(|&c| c != '-' && c != '_')
                .flat_map(|c| c.to_lowercase())
                .collect();
            return match stripped.as_str() {
                "utf8" => "UTF-8",
                "iso88591" | "latin1" => "windows-1252",  // web compat
                "iso88592" => "ISO-8859-2",
                "iso88595" => "ISO-8859-5",
                "iso88596" => "ISO-8859-6",
                "iso88597" => "ISO-8859-7",
                "iso88598" => "ISO-8859-8",
                "iso88599" => "windows-1254",  // web compat
                "iso885915" => "ISO-8859-15",
                "windows1250" => "windows-1250",
                "windows1251" => "windows-1251",
                "windows1252" => "windows-1252",
                "windows1253" => "windows-1253",
                "windows1254" => "windows-1254",
                "windows1255" => "windows-1255",
                "windows1256" => "windows-1256",
                "shiftjis" | "shift_jis" => "Shift_JIS",
                "eucjp" => "EUC-JP",
                "euckr" => "EUC-KR",
                "gb2312" | "gbk" => "GBK",
                "gb18030" => "gb18030",
                "big5" => "Big5",
                "koi8r" => "KOI8-R",
                "usascii" | "ascii" => "UTF-8",
                _ => "UTF-8",
            };
        }
    }

    // Check for UTF-8 BOM
    if data.len() >= 3 && data[0] == 0xEF && data[1] == 0xBB && data[2] == 0xBF {
        return "UTF-8";
    }

    "UTF-8"
}

fn find_subsequence(haystack: &[u8], needle: &[u8]) -> Option<usize> {
    haystack.windows(needle.len()).position(|w| w == needle)
}

/// Parse raw bytes with charset auto-detection into a Document.
pub fn parse_html_bytes(data: &[u8]) -> Document {
    parse_html_bytes_with_base(data, "")
}

/// Like `parse_html_bytes` but with a base URL for resolving relative resources.
pub fn parse_html_bytes_with_base(data: &[u8], base_url: &str) -> Document {
    let charset = detect_charset(data);
    let html = if charset == "UTF-8" {
        // Skip BOM if present
        let start = if data.len() >= 3 && data[0] == 0xEF && data[1] == 0xBB && data[2] == 0xBF {
            3
        } else {
            0
        };
        String::from_utf8_lossy(&data[start..]).into_owned()
    } else {
        let encoding = encoding_rs::Encoding::for_label(charset.as_bytes())
            .unwrap_or(encoding_rs::UTF_8);
        let (cow, _, _) = encoding.decode(data);
        cow.into_owned()
    };
    parse_html_with_base(&html, base_url)
}

// ─── Image loading ─────────────────────────────────────────────────────────

/// Resolve a URL against a base URL.
pub fn resolve_url(src: &str, base_url: &str) -> String {
    if src.is_empty() { return base_url.to_string(); }
    if src.starts_with("data:") { return src.to_string(); }
    if src.contains("://") { return src.to_string(); }

    // Strip "./" prefix
    let src = src.strip_prefix("./").unwrap_or(src);

    if base_url.starts_with("http://") || base_url.starts_with("https://") {
        let scheme_end = base_url.find("://").unwrap(); // safe: starts_with guarantees this
        let after_scheme = &base_url[scheme_end + 3..];

        // Origin = scheme + host (no path)
        let origin = match after_scheme.find('/') {
            Some(i) => &base_url[..scheme_end + 3 + i],
            None => base_url,
        };

        if src.starts_with("//") {
            let scheme = &base_url[..scheme_end + 1]; // "https:" or "http:"
            return format!("{}{}", scheme, src);
        }
        if src.starts_with('/') {
            return format!("{}{}", origin, src);
        }
        // Relative path — resolve against directory of base URL
        let dir = match after_scheme.rfind('/') {
            Some(i) => &base_url[..scheme_end + 3 + i + 1], // include trailing slash
            None => {
                // Base has no path (e.g. "https://example.com") — treat as root
                return format!("{}/{}", origin, src);
            }
        };
        return format!("{}{}", dir, src);
    }
    // File or local path
    if src.starts_with('/') || base_url.is_empty() {
        return src.to_string();
    }
    let base_dir = std::path::Path::new(base_url)
        .parent()
        .map(|p| p.to_string_lossy().to_string())
        .unwrap_or_default();
    if base_dir.is_empty() { src.to_string() }
    else { format!("{}/{}", base_dir, src) }
}

/// Set decoded image data on a node.
/// Dimensions are NOT baked into the style here — the layout engine handles
/// aspect-ratio sizing after the CSS cascade has set any explicit width/height.
pub fn set_image_on_node(node: &mut WebCore, data: Vec<u8>, w: u32, h: u32) {
    node.image_data   = Some(data);
    node.image_width  = w;
    node.image_height = h;
}

/// Set decoded image (raster or SVG) on an img node.
/// SVGs are stored as markup for deferred rasterization at the correct display size.
pub fn set_decoded_image_on_node(node: &mut WebCore, decoded: DecodedImage) {
    match decoded {
        DecodedImage::Raster(data, w, h) => {
            node.image_data   = Some(data);
            node.image_width  = w;
            node.image_height = h;
        }
        DecodedImage::Svg(markup, iw, ih) => {
            // Store SVG for paint-time rasterization at the layout-determined size
            node.svg_markup    = Some(markup);
            node.svg_viewbox_w = iw;
            node.svg_viewbox_h = ih;
            // Set intrinsic dimensions so layout can compute aspect ratio
            node.image_width   = iw.ceil() as u32;
            node.image_height  = ih.ceil() as u32;
        }
    }
}

/// Try to load an image from a file path or data URL.
/// Returns (rgba_bytes, width, height) or None on failure.
pub(crate) fn load_image_from_src(src: &str, base_url: &str) -> Option<(Vec<u8>, u32, u32)> {
    // Data URL: data:image/xxx;base64,...
    if src.starts_with("data:") {
        return load_image_data_url(src);
    }

    let path = resolve_url(src, base_url);

    // Fetch remote images via HTTP
    if path.starts_with("http://") || path.starts_with("https://") {
        let bytes = crate::http_client()
            .get(&path)
            .header("Sec-Fetch-Dest", "image")
            .send().ok()
            .and_then(|r| r.bytes().ok())
            .map(|b| b.to_vec())?;
        return decode_image_bytes(&bytes);
    }

    let bytes = std::fs::read(&path).ok()?;
    decode_image_bytes(&bytes)
}

fn load_image_data_url(src: &str) -> Option<(Vec<u8>, u32, u32)> {
    // data:image/png;base64,<data>
    let comma = src.find(',')?;
    let header = &src[5..comma]; // strip "data:"
    let encoded = &src[comma + 1..];
    let is_base64 = header.contains("base64");
    if !is_base64 { return None; }
    // Decode base64
    let bytes = base64_decode(encoded)?;
    // SVG: the image crate can't decode SVGs, so extract dimensions from XML
    if header.contains("svg") {
        return parse_svg_dimensions(&bytes);
    }
    decode_image_bytes(&bytes)
}

/// Extract width/height from an SVG's root element attributes or viewBox.
/// Returns a 1×1 transparent RGBA pixel with the SVG's declared dimensions.
fn parse_svg_dimensions(bytes: &[u8]) -> Option<(Vec<u8>, u32, u32)> {
    let text = std::str::from_utf8(bytes).ok()?;
    // Find the <svg ...> opening tag
    let svg_start = text.find("<svg")?;
    let svg_end = text[svg_start..].find('>')? + svg_start;
    let svg_tag = &text[svg_start..=svg_end];

    // Try to extract width="N" and height="N" attributes
    let mut w: Option<f32> = None;
    let mut h: Option<f32> = None;

    for attr in ["width", "height"] {
        let pattern = format!("{}=\"", attr);
        if let Some(pos) = svg_tag.find(&pattern) {
            let val_start = pos + pattern.len();
            if let Some(val_end) = svg_tag[val_start..].find('"') {
                let val_str = &svg_tag[val_start..val_start + val_end];
                // Strip units like "px"
                let num_str = val_str.trim_end_matches("px").trim();
                if let Ok(n) = num_str.parse::<f32>() {
                    if attr == "width" { w = Some(n); }
                    else { h = Some(n); }
                }
            }
        }
    }

    // Fallback: try viewBox="minX minY width height"
    if w.is_none() || h.is_none() {
        if let Some(pos) = svg_tag.find("viewBox=\"") {
            let val_start = pos + 9;
            if let Some(val_end) = svg_tag[val_start..].find('"') {
                let vb = &svg_tag[val_start..val_start + val_end];
                let parts: Vec<f32> = vb.split_whitespace()
                    .filter_map(|s| s.parse().ok())
                    .collect();
                if parts.len() == 4 {
                    if w.is_none() { w = Some(parts[2]); }
                    if h.is_none() { h = Some(parts[3]); }
                }
            }
        }
    }

    let wi = w? as u32;
    let hi = h? as u32;
    if wi == 0 || hi == 0 { return None; }
    // Return a 1×1 transparent pixel — we only need the dimensions
    Some((vec![0u8; 4], wi, hi))
}

/// Result of decoding image bytes: either rasterized RGBA or SVG markup to rasterize later.
pub enum DecodedImage {
    Raster(Vec<u8>, u32, u32),
    Svg(String, f32, f32), // markup, intrinsic_w, intrinsic_h
}

pub fn decode_image_bytes_ex(bytes: &[u8]) -> Option<DecodedImage> {
    // Try raster formats first (PNG, JPEG, GIF, WebP, BMP)
    if let Ok(img) = image::load_from_memory(bytes) {
        {
            let rgba = img.to_rgba8();
            let (w, h) = rgba.dimensions();
            // Premultiply alpha — tiny_skia expects premultiplied RGBA.
            let mut raw = rgba.into_raw();
            for pixel in raw.chunks_exact_mut(4) {
                let a = pixel[3] as u16;
                if a == 0 {
                    pixel[0] = 0; pixel[1] = 0; pixel[2] = 0;
                } else if a < 255 {
                    pixel[0] = ((pixel[0] as u16 * a) / 255) as u8;
                    pixel[1] = ((pixel[1] as u16 * a) / 255) as u8;
                    pixel[2] = ((pixel[2] as u16 * a) / 255) as u8;
                }
            }
            return Some(DecodedImage::Raster(raw, w, h));
        }
    }
    // SVG: return the markup for deferred rasterization at paint time
    if let Ok(svg_str) = std::str::from_utf8(bytes) {
        let trimmed = svg_str.trim_start();
        if trimmed.starts_with('<') && (trimmed.contains("<svg") || trimmed.starts_with("<?xml")) {
            let (iw, ih) = svg_intrinsic_size(svg_str);
            return Some(DecodedImage::Svg(svg_str.to_string(), iw, ih));
        }
    }
    None
}

/// Extract intrinsic dimensions from SVG markup without rasterizing.
fn svg_intrinsic_size(svg: &str) -> (f32, f32) {
    use resvg::usvg;
    let opt = usvg::Options::default();
    match usvg::Tree::from_str(svg, &opt) { Ok(tree) => {
        let size = tree.size();
        (size.width(), size.height())
    } _ => {
        (0.0, 0.0)
    }}
}

pub fn decode_image_bytes(bytes: &[u8]) -> Option<(Vec<u8>, u32, u32)> {
    match decode_image_bytes_ex(bytes)? {
        DecodedImage::Raster(data, w, h) => Some((data, w, h)),
        DecodedImage::Svg(svg, _, _) => rasterize_svg_intrinsic(&svg),
    }
}

/// Minimal base64 decoder (no external dependency needed for this).
fn base64_decode(s: &str) -> Option<Vec<u8>> {
    const TABLE: &[u8; 128] = b"\xff\xff\xff\xff\xff\xff\xff\xff\xff\xff\xff\xff\xff\xff\xff\xff\
\xff\xff\xff\xff\xff\xff\xff\xff\xff\xff\xff\xff\xff\xff\xff\xff\
\xff\xff\xff\xff\xff\xff\xff\xff\xff\xff\xff\x3e\xff\xff\xff\x3f\
\x34\x35\x36\x37\x38\x39\x3a\x3b\x3c\x3d\xff\xff\xff\xff\xff\xff\
\xff\x00\x01\x02\x03\x04\x05\x06\x07\x08\x09\x0a\x0b\x0c\x0d\x0e\
\x0f\x10\x11\x12\x13\x14\x15\x16\x17\x18\x19\xff\xff\xff\xff\xff\
\xff\x1a\x1b\x1c\x1d\x1e\x1f\x20\x21\x22\x23\x24\x25\x26\x27\x28\
\x29\x2a\x2b\x2c\x2d\x2e\x2f\x30\x31\x32\x33\xff\xff\xff\xff\xff";
    let clean: Vec<u8> = s.bytes().filter(|&b| b != b'\n' && b != b'\r' && b != b' ').collect();
    let mut out = Vec::with_capacity(clean.len() * 3 / 4);
    let mut i = 0;
    while i + 3 < clean.len() {
        let b0 = clean[i] as usize;
        let b1 = clean[i+1] as usize;
        let b2 = clean[i+2] as usize;
        let b3 = clean[i+3] as usize;
        if b0 >= 128 || b1 >= 128 { return None; }
        let v0 = TABLE[b0]; let v1 = TABLE[b1];
        let v2 = if clean[i+2] == b'=' { 0 } else if b2 < 128 { TABLE[b2] } else { return None; };
        let v3 = if clean[i+3] == b'=' { 0 } else if b3 < 128 { TABLE[b3] } else { return None; };
        if v0 == 0xff || v1 == 0xff { return None; }
        out.push((v0 << 2) | (v1 >> 4));
        if clean[i+2] != b'=' { out.push(((v1 & 0xf) << 4) | (v2 >> 2)); }
        if clean[i+3] != b'=' { out.push(((v2 & 0x3) << 6) | v3); }
        i += 4;
    }
    Some(out)
}

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

// ─── Tokenizer ─────────────────────────────────────────────────────────────

#[derive(Debug, Clone)]
enum Token {
    Text(String),
    OpenTag  { tag: String, attrs: HashMap<String, String>, self_closing: bool },
    CloseTag { tag: String },
    /// Comment DATA, `<!--` and `-->` already stripped.
    Comment(String),
    Doctype,
}

/// `html[start..end]`, clamped so a truncated tag cannot build a range that
/// runs backwards.
///
/// `<` and `</` at end-of-input are the cases that bite: the tag "ends" at the
/// end of the string, so `end - 1` lands BEFORE the start of the name and the
/// slice panicked. A browser meets a document cut off mid-tag constantly and
/// must simply stop tokenizing there.
fn tag_slice(html: &str, start: usize, end: usize) -> &str {
    let start = start.min(html.len());
    let end = end.max(start).min(html.len());
    // Both ends must sit on a char boundary or the slice panics on UTF-8.
    let mut s = start;
    while s < html.len() && !html.is_char_boundary(s) { s += 1; }
    let mut e = end.max(s);
    while e < html.len() && !html.is_char_boundary(e) { e += 1; }
    &html[s..e]
}

fn tokenize(html: &str) -> Vec<Token> {
    let mut tokens = Vec::new();
    let mut i = 0;
    let bytes = html.as_bytes();

    while i < bytes.len() {
        if bytes[i] == b'<' {
            if html[i..].starts_with("<!--") {
                // Carry the DATA. It used to be dropped at the token, which is
                // why no comment could ever reach the tree however the parser
                // was fixed downstream.
                // Search AFTER the `<!--`. Searching the whole slice found the
                // `-->` INSIDE the opener for `<!-->` — the `-` `-` `>` at
                // offset 2 — and built a byte range that ran backwards, which
                // panicked the tokenizer on markup a browser accepts.
                //
                // `<!-->` and `<!--->` are empty comments: §13.2.5.43-45 end
                // the comment on a `>` met in the comment-start states rather
                // than treating it as data.
                let rest = &html[i + 4..];
                let (data, end) = if let Some(r) = rest.strip_prefix('>') {
                    let _ = r;
                    (String::new(), i + 5)
                } else if rest.starts_with("->") {
                    (String::new(), i + 6)
                } else {
                    match rest.find("-->") {
                        Some(e) => (rest[..e].to_string(), i + 4 + e + 3),
                        None    => (rest.to_string(), html.len()),
                    }
                };
                tokens.push(Token::Comment(data));
                i = end;
                continue;
            }
            if i + 9 <= bytes.len() && bytes[i..i+9].eq_ignore_ascii_case(b"<!doctype") {
                let end = html[i..].find('>').map(|e| i + e + 1).unwrap_or(html.len());
                tokens.push(Token::Doctype);
                i = end;
                continue;
            }
            // BOGUS COMMENT (§13.2.5.41). `<!` that is not a comment or a
            // doctype, and `<?`, are comments — not tags. They used to fall
            // through to the generic open-tag path, so `<div><!bogus><p>x</p>`
            // built an ELEMENT named `!bogus` that then swallowed the `<p>` as
            // its child. Everything after a stray `<!` was reparented.
            if i + 1 < bytes.len() && (bytes[i + 1] == b'!' || bytes[i + 1] == b'?') {
                let found = html[i..].find('>').map(|e| i + e + 1);
                let end = found.unwrap_or(html.len());
                // `<!` drops both bytes, `<?` keeps the `?` as data, per spec.
                let data_start = if bytes[i + 1] == b'!' { i + 2 } else { i + 1 };
                let data_end = if found.is_some() { end - 1 } else { end };
                tokens.push(Token::Comment(html[data_start..data_end].to_string()));
                i = end;
                continue;
            }
            if i + 1 < bytes.len() && bytes[i + 1] == b'/' {
                let end = html[i..].find('>').map(|e| i + e + 1).unwrap_or(html.len());
                let inner = tag_slice(html, i + 2, end.saturating_sub(1));
                let tag = inner.trim().to_ascii_lowercase();
                // `</br>` is a `<br>` (HTML §13.2.6.4.7 spells this out: the end
                // tag is a parse error and is HANDLED as the start tag). Pages
                // in the wild write it, and dropping it lost the line break.
                if tag == "br" {
                    tokens.push(Token::OpenTag {
                        tag, attrs: HashMap::new(), self_closing: true,
                    });
                } else {
                    tokens.push(Token::CloseTag { tag });
                }
                i = end;
                continue;
            }
            let end = find_tag_end(html, i);
            let tag_src = tag_slice(html, i + 1, end.saturating_sub(1));
            let had_slash = tag_src.trim_end().ends_with('/');
            let tag_src = tag_src.trim_end_matches('/').trim();
            let (tag, attrs) = parse_tag_attrs(tag_src);
            // HTML §13.2.6.4.7 in full: "A start tag whose tag name is
            // 'image' — change the token's tag name to 'img' and reprocess."
            // It is not an alias, it is a rename, and pages rely on it.
            let tag = if tag == "image" { "img".to_string() } else { tag };
            let is_void = is_void_element(&tag);
            // A trailing `/` on a non-void HTML element is IGNORED — `<div/>x`
            // opens a div and `x` goes INSIDE it. Only foreign content (SVG,
            // MathML) honours self-closing syntax, and only a void element is
            // self-closing on its own. Treating the slash as authoritative made
            // `<div/>` an empty element and put the rest of the document beside
            // it instead of within.
            let self_closing = is_void || (had_slash && is_foreign_content_tag(&tag));
            tokens.push(Token::OpenTag { tag: tag.clone(), attrs, self_closing });
            i = end;
            // Raw text / foreign content elements: content must not be parsed as HTML.
            // <svg> is foreign content — collect everything until </svg> as raw text
            // so inner SVG elements (path, circle, etc.) don't interfere with HTML parsing.
            // RAWTEXT and RCDATA elements: the content is TEXT. `<title>a<b>c`
            // has no `<b>` element in it, and `<textarea><p>x</p>` holds the
            // literal markup as its value — parsing them as HTML built elements
            // no browser has and lost the characters that made up the tags.
            if matches!(tag.as_str(),
                        "style" | "script" | "noscript" | "svg" | "title" | "textarea")
                && !(self_closing || is_void) {
                let close_pat = format!("</{}", tag);
                let raw_end = crate::css::find_case_insensitive(&html[i..], &close_pat)
                    .map(|e| i + e)
                    .unwrap_or(html.len());
                let raw_text = &html[i..raw_end];
                if !raw_text.is_empty() {
                    tokens.push(Token::Text(raw_text.to_string()));
                }
                // Consume the close tag too
                i = raw_end;
                if i < html.len() {
                    let close_end = html[i..].find('>').map(|e| i + e + 1).unwrap_or(html.len());
                    let close_inner = &html[i + 2..close_end.saturating_sub(1)];
                    let close_tag = close_inner.trim().to_ascii_lowercase();
                    tokens.push(Token::CloseTag { tag: close_tag });
                    i = close_end;
                }
            }
        } else {
            let start = i;
            while i < bytes.len() && bytes[i] != b'<' { i += 1; }
            let text = &html[start..i];
            if !text.is_empty() {
                tokens.push(Token::Text(decode_entities(text)));
            }
        }
    }
    tokens
}

fn find_tag_end(html: &str, start: usize) -> usize {
    let bytes = html.as_bytes();
    let mut i = start + 1;
    let mut in_q: Option<u8> = None;
    let mut prev_meaningful: u8 = 0; // last non-whitespace byte
    while i < bytes.len() {
        let b = bytes[i];
        if let Some(q) = in_q {
            if b == q { in_q = None; }
        } else if (b == b'"' || b == b'\'') && prev_meaningful == b'=' {
            // Only enter quoted mode for attribute values (after '=').
            // Stray quotes (e.g. <div " class="...">) must not toggle
            // quote tracking, which would hide the closing '>'.
            in_q = Some(b);
        } else if b == b'>' {
            return i + 1;
        }
        if !b.is_ascii_whitespace() { prev_meaningful = b; }
        i += 1;
    }
    html.len()
}

fn parse_tag_attrs(s: &str) -> (String, HashMap<String, String>) {
    let mut iter = s.splitn(2, |c: char| c.is_whitespace());
    let tag = iter.next().unwrap_or("").to_ascii_lowercase();
    let rest = iter.next().unwrap_or("").trim();
    let attrs = parse_attrs(rest);
    (tag, attrs)
}

fn parse_attrs(s: &str) -> HashMap<String, String> {
    let mut map = HashMap::new();
    let mut i = 0;
    let bytes = s.as_bytes();
    while i < bytes.len() {
        while i < bytes.len() && bytes[i].is_ascii_whitespace() { i += 1; }
        if i >= bytes.len() { break; }
        let name_start = i;
        while i < bytes.len() && bytes[i] != b'=' && !bytes[i].is_ascii_whitespace() && bytes[i] != b'/' { i += 1; }
        let name = s[name_start..i].to_ascii_lowercase();
        if name.is_empty() { i += 1; continue; }
        while i < bytes.len() && bytes[i].is_ascii_whitespace() { i += 1; }
        if i >= bytes.len() || bytes[i] != b'=' {
            // FIRST occurrence wins. The tokenizer's attribute-name state drops
            // a duplicate rather than overwriting, so `<div CLASS=x class=y>`
            // is `class="x"`. `insert` gave the last one, which is the opposite.
            map.entry(name).or_default();
            continue;
        }
        i += 1;
        while i < bytes.len() && bytes[i].is_ascii_whitespace() { i += 1; }
        let value = if i < bytes.len() && (bytes[i] == b'"' || bytes[i] == b'\'') {
            let q = bytes[i];
            i += 1;
            let start = i;
            while i < bytes.len() && bytes[i] != q { i += 1; }
            let v = decode_entities_attr(&s[start..i]);
            if i < bytes.len() { i += 1; }
            v
        } else {
            let start = i;
            while i < bytes.len() && !bytes[i].is_ascii_whitespace() && bytes[i] != b'>' { i += 1; }
            decode_entities_attr(&s[start..i])
        };
        map.entry(name).or_insert(value);
    }
    map
}

fn is_void_element(tag: &str) -> bool {
    matches!(tag, "area" | "base" | "br" | "col" | "embed" | "hr" | "img" | "input"
        | "link" | "meta" | "param" | "source" | "track" | "wbr"
        // SVG void elements — never have child content
        | "path" | "circle" | "rect" | "line" | "polygon" | "polyline"
        | "ellipse" | "use" | "image" | "stop")
}

/// Elements whose content should be completely suppressed (no box, no text)
/// HTML implicit closing rules (HTML spec §12.2.6.4).
/// Returns true if seeing `new_tag` as an open tag should auto-close `current`.
/// The formatting elements of HTML §13.2.4.3.
///
/// What sets them apart is that closing an ANCESTOR does not end them: when
/// `</b>` in `<b>1<i>2</b>3` implicitly closes the `<i>`, the `<i>` is
/// reconstructed afterwards so `3` is still italic. Nothing else in the parser
/// behaves that way — `<section><span>x</section>y` leaves `y` unwrapped,
/// because `span` is not on this list.
fn is_formatting_element(tag: &str) -> bool {
    matches!(tag,
        "a" | "b" | "big" | "code" | "em" | "font" | "i" | "nobr"
        | "s" | "small" | "strike" | "strong" | "tt" | "u")
}

/// HTML §13.2.4.2's "special" category — the elements that break out of a
/// formatting element rather than nest inside it. Used to find the adoption
/// agency's furthest block.
fn is_special_element(tag: &str) -> bool {
    matches!(tag,
        "address" | "applet" | "area" | "article" | "aside" | "base" | "basefont"
        | "bgsound" | "blockquote" | "body" | "br" | "button" | "caption" | "center"
        | "col" | "colgroup" | "dd" | "details" | "dir" | "div" | "dl" | "dt"
        | "embed" | "fieldset" | "figcaption" | "figure" | "footer" | "form"
        | "frame" | "frameset" | "h1" | "h2" | "h3" | "h4" | "h5" | "h6" | "head"
        | "header" | "hgroup" | "hr" | "html" | "iframe" | "img" | "input"
        | "keygen" | "li" | "link" | "listing" | "main" | "marquee" | "menu"
        | "meta" | "nav" | "noembed" | "noframes" | "noscript" | "object" | "ol"
        | "p" | "param" | "plaintext" | "pre" | "script" | "search" | "section"
        | "select" | "source" | "style" | "summary" | "table" | "tbody" | "td"
        | "template" | "textarea" | "tfoot" | "th" | "thead" | "title" | "tr"
        | "track" | "ul" | "wbr" | "xmp")
}

/// Elements whose subtree is FOREIGN content, where XML self-closing syntax
/// (`<circle/>`) is real. HTML elements ignore a trailing slash.
/// Start tags that "in select" handles as `</select>` (HTML §13.2.6.4.16).
/// Everything else stays inside the select.
fn closes_select(tag: &str) -> bool {
    matches!(tag, "input" | "keygen" | "textarea" | "select")
}

fn is_foreign_content_tag(tag: &str) -> bool {
    matches!(tag, "svg" | "math")
}

fn should_auto_close(current: &str, new_tag: &str) -> bool {
    match current {
        // <p> closes when a block-level element opens
        "p" => matches!(new_tag,
            "address" | "article" | "aside" | "blockquote" | "center" |
            "details" | "dialog" | "dir" | "div" | "dl" | "fieldset" |
            "figcaption" | "figure" | "footer" | "form" | "h1" | "h2" |
            "h3" | "h4" | "h5" | "h6" | "header" | "hgroup" | "hr" |
            "li" | "listing" | "main" | "menu" | "nav" | "ol" | "p" |
            "plaintext" | "pre" | "search" | "section" | "summary" |
            "table" | "ul" | "xmp"
        ),
        // A heading closes on another heading (HTML §13.2.6.4.7: an h1-h6 start
        // tag while one is open is a parse error that pops it).
        "h1" | "h2" | "h3" | "h4" | "h5" | "h6" =>
            matches!(new_tag, "h1" | "h2" | "h3" | "h4" | "h5" | "h6"),
        // <li> closes on another <li>
        "li" => new_tag == "li",
        // <dt> closes on <dt> or <dd>
        "dt" => matches!(new_tag, "dt" | "dd"),
        // <dd> closes on <dt> or <dd>
        "dd" => matches!(new_tag, "dt" | "dd"),
        // <td> closes on <td>, <th>, or end-of-row tags
        "td" => matches!(new_tag, "td" | "th"),
        // <th> closes on <td>, <th>
        "th" => matches!(new_tag, "td" | "th"),
        // <tr> closes on <tr>
        "tr" => new_tag == "tr",
        // <thead>/<tbody>/<tfoot> close on each other
        "thead" | "tbody" | "tfoot" => matches!(new_tag, "thead" | "tbody" | "tfoot"),
        // <option> closes on <option> or <optgroup>
        "option" => matches!(new_tag, "option" | "optgroup"),
        // <optgroup> closes on <optgroup>
        "optgroup" => new_tag == "optgroup",
        // <rt>/<rp> close on <rt>/<rp>
        "rt" | "rp" => matches!(new_tag, "rt" | "rp"),
        // <colgroup> closes on non-col content
        "colgroup" => new_tag != "col",
        // <head> closes when <body> opens
        "head" => new_tag == "body",
        _ => false,
    }
}

fn is_non_visual(tag: &str) -> bool {
    // script/noscript are handled separately (content passed to host hook).
    matches!(tag, "head" | "meta" | "link")
}

// ─── Default display ────────────────────────────────────────────────────────

pub fn default_display(tag: &str) -> &'static str {
    match tag {
        "div" | "p" | "h1" | "h2" | "h3" | "h4" | "h5" | "h6"
        | "ul" | "ol" | "dl" | "dt" | "dd" | "pre" | "blockquote" | "hr"
        | "section" | "article" | "aside" | "nav" | "header" | "footer" | "main"
        | "address" | "figure" | "figcaption" | "details" | "center"
        | "form" | "fieldset" | "legend" | "hgroup" | "search" | "dialog"
            => "block",
        // A projection point, not a box — its assigned nodes lay out as if they
        // were children of the slot's parent (HTML §15.3.4).
        "slot" => "contents",
        "summary" => "list-item",
        "li"    => "list-item",
        "table" => "table",
        "tr"    => "table-row",
        "td"    => "table-cell",
        "th"    => "table-cell",
        "thead" | "tbody" | "tfoot" => "table-row-group",
        "col"      => "table-column",
        "colgroup" => "table-column-group",
        "caption"  => "table-caption",
        "img" | "svg" | "canvas" | "video" | "audio" => "inline-block",
        "input" | "select" | "textarea" => "inline-block",
        "button" => "inline-flex",
        "ruby" => "ruby",
        "rt"   => "ruby-text",
        // Non-visual: display:none
        "head" | "style" | "script" | "title" | "meta" | "link" | "noscript"
        | "option" | "optgroup" | "datalist" | "track" => "none",
        // Everything else is inline
        _ => "inline",
    }
}

// ─── Table structure (HTML §13.2.6.4.9 "in table") ─────────────────────────

/// May this node sit directly inside a `<table>`?
///
/// The spec's "in table" insertion mode handles exactly these; anything else is
/// foster-parented out. Whitespace-only text is kept — it is the newline
/// between `<table>` and `<tr>` that every hand-written table has, and moving
/// it would put a stray text node before every table on the web.
fn allowed_in_table(node: &WebCore) -> bool {
    match node.tag.as_str() {
        "caption" | "colgroup" | "col" | "thead" | "tbody" | "tfoot" | "tr"
        | "form" | "script" | "template" | "style" | "#comment" => true,
        // A bare cell is table content: the row it needs is IMPLIED, not
        // missing (see `wrap_bare_cells_in_row`). Fostering it out instead
        // emptied `<table><td>x</td></table>` of everything it had.
        "td" | "th" => true,
        "#text" => node.text.trim().is_empty(),
        _ => false,
    }
}

/// Give bare `<td>`/`<th>` children of a table an implied `<tr>`.
///
/// `<table><td>x</td></table>` is `table > tbody > tr > td` in every browser —
/// the "in table" mode inserts the row the author left out, the same way it
/// inserts the `<tbody>`. Without it the cells were not table content, so they
/// were foster-parented out and the table came back empty.
fn wrap_bare_cells_in_row(table: &mut WebCore) {
    if !table.children.iter().any(|c| c.tag == "td" || c.tag == "th") { return; }
    let children = std::mem::take(&mut table.children);
    let mut out: Vec<WebCore> = Vec::new();
    let mut row: Option<WebCore> = None;
    for child in children {
        if child.tag == "td" || child.tag == "th" {
            let r = row.get_or_insert_with(|| {
                let mut n = WebCore::new("tr");
                apply_property(&mut n.style, "display", default_display("tr"));
                n
            });
            r.children.push(child);
        } else {
            if let Some(r) = row.take() { out.push(r); }
            out.push(child);
        }
    }
    if let Some(r) = row.take() { out.push(r); }
    table.children = out;
}

/// Group a table's stray `<tr>` children into implied `<tbody>` elements.
///
/// `<table><tr>` parses to `table > tbody > tr` in every browser: the tree
/// builder inserts the `<tbody>` the author left out. Without it the DOM is a
/// shape no real page ever sees — `table > tr` — so `tbody` selectors,
/// `:nth-child`, and `HTMLTableElement.tBodies` all disagree with a browser.
///
/// Consecutive runs are grouped into ONE `<tbody>`, and a run is not broken by
/// the whitespace between rows, so `<table>\n<tr>…\n<tr>…\n</table>` yields a
/// single tbody like it should rather than one per row.
fn group_rows_into_tbody(table: &mut WebCore) {
    wrap_bare_cells_in_row(table);
    if !table.children.iter().any(|c| c.tag == "tr") { return; }
    let children = std::mem::take(&mut table.children);
    let mut out: Vec<WebCore> = Vec::new();
    let mut current: Option<WebCore> = None;
    // Whitespace held back, so it only joins a run that actually continues.
    let mut pending_ws: Vec<WebCore> = Vec::new();
    for child in children {
        let is_ws = child.is_text_node() && child.text.trim().is_empty();
        if child.tag == "tr" {
            let tbody = current.get_or_insert_with(|| {
                let mut b = WebCore::new("tbody");
                apply_property(&mut b.style, "display", default_display("tbody"));
                b
            });
            tbody.children.append(&mut pending_ws);
            tbody.children.push(child);
        } else if is_ws && current.is_some() {
            pending_ws.push(child);
        } else {
            if let Some(tbody) = current.take() { out.push(tbody); }
            out.append(&mut pending_ws);
            out.push(child);
        }
    }
    if let Some(tbody) = current.take() { out.push(tbody); }
    out.append(&mut pending_ws);
    table.children = out;
}

/// Valid parents for an element that only belongs inside a table.
/// `None` when the tag is not table-only and may appear anywhere.
fn table_part_parents(tag: &str) -> Option<&'static [&'static str]> {
    match tag {
        "caption" | "colgroup" | "thead" | "tbody" | "tfoot" => Some(&["table"]),
        "tr" => Some(&["table", "thead", "tbody", "tfoot"]),
        "td" | "th" => Some(&["tr"]),
        "col" => Some(&["colgroup"]),
        _ => None,
    }
}

/// Drop table parts that are not inside a table, keeping their content.
///
/// `<div><td>orphan</td></div>` has no table anywhere, and the "in body"
/// insertion mode ignores a `<td>` start tag outright — so a browser keeps the
/// text and no cell. We were building the element, which put a `display:
/// table-cell` box in the middle of a block flow and made `querySelector("td")`
/// answer on a document with no table in it.
fn unwrap_misplaced_table_parts(node: &mut WebCore) {
    if node.tag == "template" { return; }
    for child in &mut node.children {
        unwrap_misplaced_table_parts(child);
    }
    let parent_tag = node.tag.clone();
    // Index-based and IN PLACE. A `WebCore` is ~4KB, so moving one into a local
    // costs 4KB of STACK per recursion level — a page 80 elements deep
    // overflowed the stack before it finished parsing. Nothing here holds a
    // node by value.
    let mut i = 0;
    while i < node.children.len() {
        let misplaced = table_part_parents(&node.children[i].tag)
            .map(|ok| !ok.contains(&parent_tag.as_str()))
            .unwrap_or(false);
        if misplaced {
            // The element is ignored and its CONTENT takes its place — then the
            // loop re-examines that content against this parent, because
            // promoting a `<tr>`'s children leaves `<td>`s somewhere that
            // cannot hold them either. `i` deliberately does not advance.
            let promoted = std::mem::take(&mut node.children[i].children);
            node.children.splice(i..=i, promoted);
        } else {
            i += 1;
        }
    }
}

/// Move a table-level `<form>`'s children out into the table.
///
/// HTML §13.2.6.4.9 keeps the `<form>` (the form element pointer is set) but
/// inserts nothing into it — the rows that follow are inserted into the TABLE.
/// Chrome gives `table > [form, tbody > tr]`; we were nesting the rows inside
/// the form, which put a block box between the table and its rows.
fn hoist_table_form_children(table: &mut WebCore) {
    if !table.children.iter().any(|c| c.tag == "form" && !c.children.is_empty()) { return; }
    let children = std::mem::take(&mut table.children);
    let mut out = Vec::with_capacity(children.len());
    for mut child in children {
        if child.tag == "form" {
            let inner = std::mem::take(&mut child.children);
            out.push(child);
            out.extend(inner);
        } else {
            out.push(child);
        }
    }
    table.children = out;
}

/// Apply the table fix-ups to `node`'s subtree.
///
/// Foster parenting first: content that may not sit in a table is moved out to
/// just BEFORE the table, in order, as a sibling. `<div><table>stray<tr>…`
/// becomes `<div>stray<table><tbody><tr>…` — which is what Chrome produces, and
/// why the text used to render inside the table box instead of above it.
fn normalize_tables(node: &mut WebCore) {
    for child in &mut node.children {
        normalize_tables(child);
    }
    if !node.children.iter().any(|c| c.tag == "table") { return; }
    // In place, by index: a `WebCore` is ~4KB and this recurses once per level,
    // so moving nodes through locals put kilobytes on the stack per element and
    // a deep page overflowed before it finished parsing.
    let mut i = 0;
    while i < node.children.len() {
        if node.children[i].tag != "table" {
            i += 1;
            continue;
        }
        // Foster parenting: content that may not sit in a table moves out to
        // just BEFORE the table, in order, as a sibling.
        let mut fostered: Vec<WebCore> = Vec::new();
        let mut k = 0;
        while k < node.children[i].children.len() {
            if allowed_in_table(&node.children[i].children[k]) {
                k += 1;
            } else {
                fostered.push(node.children[i].children.remove(k));
            }
        }
        hoist_table_form_children(&mut node.children[i]);
        group_rows_into_tbody(&mut node.children[i]);
        let moved = fostered.len();
        node.children.splice(i..i, fostered);
        i += moved + 1;
    }
}

// ─── Apply presentational attributes ───────────────────────────────────────

/// Apply a presentational hint through the element's `style` attribute,
/// WITHOUT overwriting a declaration the author already wrote there.
///
/// The style attribute is the only place a parse-time value survives: the
/// cascade rebuilds `node.style` from scratch, so writing the hint directly
/// onto the computed style loses it to the UA sheet. Writing the attribute is
/// therefore how the hint has to travel — but it must be written ONCE.
/// Appending unconditionally made `rows="3"` add `height:4.2em` on every
/// serialize → reparse cycle, so a saved-and-reloaded page grew
/// `style="height:4.2em;height:4.2em;…"` without bound.
///
/// Skipping when the property is already present also gives the hint the right
/// PRECEDENCE for free: an author's own `style="height:10px"` stays.
fn add_presentational_style(node: &mut WebCore, prop: &str, value: &str) {
    let existing = node.attributes.get("style").cloned().unwrap_or_default();
    let already = existing.split(';').any(|d| {
        d.split(':').next().map(|k| k.trim().eq_ignore_ascii_case(prop)).unwrap_or(false)
    });
    if already { return; }
    let decl = format!("{}:{}", prop, value);
    node.attributes.insert(
        "style".into(),
        if existing.trim().is_empty() { decl } else { format!("{};{}", existing, decl) },
    );
}

fn apply_presentational_attrs(node: &mut WebCore) {
    let attrs = node.attributes.clone();
    let tag = node.tag.clone();

    // Translate body `text` attribute to `color` attribute so the cascade picks it up
    if tag == "body" {
        if let Some(text_color) = attrs.get("text") {
            let text_color = text_color.clone();
            node.attributes.entry("color".to_string()).or_insert(text_color);
        }
    }

    // Sorted, because `attrs` is a HashMap and two presentational attributes on
    // one element can map to the SAME property — `bgcolor` and
    // `background-color` both set the background, `size` and `width` both size
    // an `<input>`. Applied in hash order, which element won was decided by the
    // process's hash seed, the same way declaration blocks were before
    // `css::Declarations`. Sorting is not the spec's order, but it is an order:
    // the same document renders the same way twice.
    let mut ordered_attrs: Vec<(&String, &String)> = attrs.iter().collect();
    ordered_attrs.sort_by(|a, b| a.0.cmp(b.0));
    for (attr, val) in ordered_attrs {
        match attr.as_str() {
            "align" => match val.as_str() {
                "center"  => apply_property(&mut node.style, "text-align", "center"),
                "right"   => apply_property(&mut node.style, "text-align", "right"),
                "left"    => apply_property(&mut node.style, "text-align", "left"),
                "justify" => apply_property(&mut node.style, "text-align", "justify"),
                _ => {}
            },
            "valign" => match val.as_str() {
                "top"    => apply_property(&mut node.style, "vertical-align", "top"),
                "middle" => apply_property(&mut node.style, "vertical-align", "middle"),
                "bottom" => apply_property(&mut node.style, "vertical-align", "bottom"),
                _ => {}
            },
            "bgcolor" | "background-color" => {
                apply_property(&mut node.style, "background-color", val);
            }
            "color" => {
                apply_property(&mut node.style, "color", val);
            }
            "text" if tag == "body" => {
                // handled above by translating to `color` attribute
            }
            "width" => {
                if val.ends_with('%') {
                    apply_property(&mut node.style, "width", val);
                } else if let Ok(n) = val.parse::<f32>() {
                    apply_property(&mut node.style, "width", &format!("{}px", n));
                }
            }
            "height" => {
                if val.ends_with('%') {
                    apply_property(&mut node.style, "height", val);
                } else if let Ok(n) = val.parse::<f32>() {
                    apply_property(&mut node.style, "height", &format!("{}px", n));
                }
            }
            "border" if tag == "table" => {
                if val == "0" {
                    apply_property(&mut node.style, "border", "0px solid transparent");
                } else if let Ok(n) = val.parse::<f32>() {
                    if n > 0.0 {
                        apply_property(&mut node.style, "border", &format!("{}px solid black", n));
                    }
                }
            }
            // FONT legacy attributes
            "face" if tag == "font" => {
                apply_property(&mut node.style, "font-family", val);
            }
            "size" if tag == "input" => {
                // HTML input size attribute: number of characters wide
                // Inject as inline style so it overrides UA width
                if let Ok(n) = val.parse::<f32>() {
                    let w = n * 0.6;
                    let style_str = format!("width:{}em", w);
                    let existing = node.attributes.get("style").cloned().unwrap_or_default();
                    node.attributes.insert("style".into(), if existing.is_empty() { style_str } else { format!("{};{}", existing, style_str) });
                }
            }
            // `rows`/`cols` are presentational HINTS. They apply to the
            // computed style directly and must NOT be written into the `style`
            // ATTRIBUTE: an author's inline style is a document fact, and
            // appending to it made the hint reappear on every serialize →
            // reparse cycle (`style="height:4.2em;height:4.2em;…"`), growing
            // without bound. It also silently outranked the author's own CSS,
            // which is the opposite of what a hint does.
            "rows" if tag == "textarea" => {
                if let Ok(n) = val.parse::<f32>() {
                    add_presentational_style(node, "height", &format!("{}em", n * 1.4));
                }
            }
            "cols" if tag == "textarea" => {
                if let Ok(n) = val.parse::<f32>() {
                    add_presentational_style(node, "width", &format!("{}em", n * 0.6));
                }
            }
            "size" if tag == "font" => {
                // HTML font size 1-7 → approximate px sizes
                let px: f32 = match val.trim() {
                    "1" => 10.0, "2" => 13.0, "3" => 16.0,
                    "4" => 18.0, "5" => 24.0, "6" => 32.0, "7" => 48.0,
                    _ => 16.0,
                };
                apply_property(&mut node.style, "font-size", &format!("{}px", px));
            }
            // TABLE legacy attributes
            "cellpadding" if tag == "table" => {
                apply_property(&mut node.style, "cellpadding", val);
            }
            "cellspacing" if tag == "table" => {
                apply_property(&mut node.style, "cellspacing", val);
            }
            // COL attributes
            "span" if tag == "col" => {
                // Stored in attributes; layout reads it directly
            }
            _ => {}
        }
    }

    // Inline style
    if let Some(style_val) = attrs.get("style") {
        let style_val = style_val.clone();
        apply_inline_style(node, &style_val);
    }

    // dir attribute
    if let Some(dir) = attrs.get("dir") {
        match dir.to_ascii_lowercase().as_str() {
            "rtl" => apply_property(&mut node.style, "direction", "rtl"),
            "ltr" => apply_property(&mut node.style, "direction", "ltr"),
            _ => {}
        }
    }
}

fn apply_inline_style(node: &mut WebCore, css: &str) {
    for decl in css.split(';') {
        let decl = decl.trim();
        if decl.is_empty() { continue; }
        if let Some(colon) = decl.find(':') {
            let prop = decl[..colon].trim();
            let val  = decl[colon+1..].trim();
            if !prop.is_empty() && !val.is_empty() {
                let normalized = normalize_css_value(val);
                apply_property(&mut node.style, prop, &normalized);
            }
        }
    }
}

/// Convert pt units to px (1pt = 4/3 px at 96dpi), since the CSS parser
/// doesn't handle `pt` directly.
fn normalize_css_value(v: &str) -> String {
    if v.ends_with("pt") {
        if let Ok(n) = v[..v.len() - 2].trim().parse::<f32>() {
            return format!("{}px", n * 4.0 / 3.0);
        }
    }
    v.to_string()
}

/// Normalize a CSS text block, converting pt to px so the CSS parser handles it.
fn normalize_css_text(css: &str) -> String {
    // Simple token replacement: find number+pt and replace with number*4/3 px
    let mut out = String::with_capacity(css.len());
    let mut i = 0;
    let bytes = css.as_bytes();
    while i < bytes.len() {
        // Try to match number followed by "pt" (with word boundary)
        // Find digits (possibly with decimal) followed by "pt" not followed by another alpha
        if bytes[i].is_ascii_digit() || (bytes[i] == b'.' && i + 1 < bytes.len() && bytes[i+1].is_ascii_digit()) {
            let start = i;
            if bytes[i] == b'.' { i += 1; }
            while i < bytes.len() && (bytes[i].is_ascii_digit() || bytes[i] == b'.') { i += 1; }
            // Check if followed by "pt" and then non-alpha
            if i + 1 < bytes.len() && bytes[i] == b'p' && bytes[i+1] == b't' {
                let after = i + 2;
                let boundary = after >= bytes.len()
                    || !bytes[after].is_ascii_alphanumeric() && bytes[after] != b'_';
                if boundary {
                    if let Ok(n) = css[start..i].parse::<f32>() {
                        let px = n * 4.0 / 3.0;
                        out.push_str(&format!("{:.4}px", px));
                        i += 2; // skip "pt"
                        continue;
                    }
                }
            }
            // Not a pt value, emit original
            out.push_str(&css[start..i]);
        } else {
            out.push(bytes[i] as char);
            i += 1;
        }
    }
    out
}

// ─── Parser ─────────────────────────────────────────────────────────────────

struct HtmlParser {
    tokens:             Vec<Token>,
    pos:                usize,
    stylesheet:         Stylesheet,
    title:              String,
    base_url:           String,
    linked_stylesheets: Vec<(String, String)>,  // (href, media)
    /// Monotonically increasing counter for assigning stable node_ids.
    next_node_id:       u32,
    /// Arena-based DOM being built in parallel with the WebCore tree.
    arena:              crate::dom::arena::DomArena,
    /// Optional host-registered hook, fired for every open tag as it is parsed.
    /// Receives the tag name and its attribute map.
    on_open_tag: Option<Box<dyn FnMut(&str, &HashMap<String, String>) + 'static>>,
    /// Optional host callback for `<script>` and `<noscript>` tags.
    /// Receives (tag, attrs, raw_content) and returns true if the host handled it.
    /// If None or returns false: `<noscript>` content is parsed as HTML (shown to
    /// the user as fallback), `<script>` is discarded.
    on_script: Option<Box<dyn FnMut(&str, &HashMap<String, String>, &str) -> bool + 'static>>,
    /// Whether the head has been closed — the one bit of HTML §13.2.6's
    /// insertion-mode state this parser needs.
    ///
    /// The spec runs "before head" exactly ONCE. After the head closes, a
    /// `<head>` start tag is a parse error and the TOKEN is ignored; what
    /// follows keeps being parsed as body content. Without this the parser
    /// would happily re-enter head parsing halfway down a page and swallow
    /// markup that belongs in the body.
    head_closed: bool,
    /// The head's ELEMENT children, in source order.
    ///
    /// The head's contents were consumed and thrown away — the title lifted
    /// into `Document.title`, the stylesheets into the cascade — so `<head>`
    /// was an empty element and `querySelector("title")` answered nothing in a
    /// document that plainly had one. Consuming and keeping are not exclusive:
    /// the title still lands in `Document.title` and the CSS still reaches the
    /// cascade, and the nodes exist so the DOM can name them. Nothing here
    /// renders — `head` is `display: none` in the UA sheet, so its subtree
    /// never reaches layout.
    head_children: Vec<WebCore>,
    /// Formatting elements closed implicitly and waiting to be re-opened —
    /// HTML §13.2.4.3's "list of active formatting elements", reconstructed
    /// lazily when content arrives.
    ///
    /// On the PARSER rather than in `parse_children_into`, because a formatting
    /// element can be closed by an end tag that also ends the element that
    /// function was collecting children for. `<section><b>x</section>y` closes
    /// `<b>` and `<section>` together; the pending `<b>` has to survive the
    /// return so `y` is still bold at the level above.
    pending_format: Vec<WebCore>,
}

impl HtmlParser {
    fn new(tokens: Vec<Token>) -> Self {
        Self {
            tokens,
            pos: 0,
            stylesheet: Stylesheet::default(),
            title: String::new(),
            base_url: String::new(),
            linked_stylesheets: Vec::new(),
            next_node_id: 1, // 0 = NodeId::NONE (reserved)
            arena: crate::dom::arena::DomArena::new(),
            on_open_tag: None,
            on_script: None,
            head_closed: false,
            head_children: Vec::new(),
            pending_format: Vec::new(),
        }
    }

    /// Record a head element (`title`/`meta`/`link`/`style`/`base`) as a node.
    /// `text` is the element's text content, empty for the void ones.
    /// Wrap freshly-appended nodes in the formatting elements waiting to be
    /// reconstructed, outermost first.
    ///
    /// The element stack in `parse_children_into` re-opens a pending formatting
    /// element by pushing a FRAME; a document-level loop has no stack, so it
    /// wraps instead. Same rule, same result for the shape that reaches here:
    /// `<section><b>x</section>y` leaves `y` inside a fresh `<b>`.
    /// `had_pending` must be sampled BEFORE the token was processed: a token
    /// can CREATE the pending entry itself — `</section>` in
    /// `<section><b>x</section>` closes the `<b>` — and the element it closed
    /// must not then be wrapped in a copy of it.
    fn reconstruct_into(&mut self, children: &mut Vec<WebCore>, from: usize, had_pending: bool) {
        if !had_pending || self.pending_format.is_empty() || children.len() <= from { return; }
        let inner: Vec<WebCore> = children.drain(from..).collect();
        let mut wrapped = inner;
        for tpl in std::mem::take(&mut self.pending_format).into_iter().rev() {
            let mut node = tpl;
            node.children = wrapped;
            wrapped = vec![node];
        }
        children.extend(wrapped);
    }

    fn push_head_node(&mut self, tag: &str, attrs: HashMap<String, String>, text: String) {
        let mut node = self.new_box(tag);
        node.attributes = attrs;
        node.text = text;
        apply_property(&mut node.style, "display", "none");
        self.head_children.push(node);
    }

    /// Create an WebCore with a fresh sequential node_id.
    /// Also creates the corresponding node in the arena.
    #[inline]
    fn new_box(&mut self, tag: &str) -> WebCore {
        let mut b = WebCore::new(tag);
        let arena_id = if tag == "#text" {
            self.arena.create_text("")
        } else {
            self.arena.create_element(tag)
        };
        // The arena assigns NodeId sequentially starting from 1, matching our counter.
        b.node_id = arena_id.0;
        // Keep next_node_id in sync (for non-parser code that may need to allocate)
        self.next_node_id = self.next_node_id.max(arena_id.0 + 1);
        b
    }

    /// Fire the host hook (if any) for an open tag.
    #[inline]
    fn fire_hook(&mut self, tag: &str, attrs: &HashMap<String, String>) {
        if let Some(ref mut f) = self.on_open_tag {
            f(tag, attrs);
        }
    }

    /// Parse children until close tag matching `parent_tag` or EOF.
    /// If `parent_tag` is empty, parse until EOF.
    /// All resulting boxes are appended to the provided `children` vec.
    ///
    /// Iterative implementation using an explicit stack — handles arbitrarily
    /// deep nesting without risking a stack overflow.
    fn parse_children_into(
        &mut self,
        parent_tag: &str,
        children: &mut Vec<WebCore>,
        ol_counter: &mut i32,
    ) {
        // Each frame represents one nesting level.
        struct Frame {
            parent_tag: String,
            node:       WebCore,   // the element whose children we're collecting
            ol_counter: i32,
        }

        /// Re-open the formatting elements an end tag closed out from under.
        ///
        /// HTML §13.2.4.3's "reconstruct the active formatting elements", run
        /// LAZILY like the spec does: the templates sit in `pending` until real
        /// content arrives, so `<p><b>1<i>2</b></p>` does not gain a stray empty
        /// `<i>`. Outermost first, which is the order `pending` is built in.
        fn reconstruct(stack: &mut Vec<Frame>, pending: &mut Vec<WebCore>) {
            for tpl in pending.drain(..) {
                stack.push(Frame { parent_tag: tpl.tag.clone(), node: tpl, ol_counter: 0 });
            }
        }

        // Bottom frame collects the top-level children that will be returned.
        let mut stack: Vec<Frame> = Vec::new();
        stack.push(Frame {
            parent_tag: parent_tag.to_string(),
            node:       WebCore::new("__root__"),  // temporary container, no arena node needed
            ol_counter: *ol_counter,
        });

        loop {
            let cur_tag = &stack.last().unwrap().parent_tag;
            let preserve_ws = matches!(cur_tag.as_str(), "pre" | "textarea" | "listing" | "xmp" | "plaintext");

            match self.tokens.get(self.pos).cloned() {
                None => break, // EOF

                Some(Token::CloseTag { tag }) => {
                    // Find matching frame in the stack (like a browser's
                    // "adoption agency" — pop implicit close tags up to
                    // the matching ancestor, or ignore if truly stray).
                    let match_idx = stack.iter().rposition(|f| f.parent_tag == tag);

                    // The adoption agency's "furthest block" case: a BLOCK was
                    // opened inside a formatting element and is still open when
                    // the formatting element ends — `<b>1<p>2</b>3</p>`.
                    //
                    // Reconstruction is not enough here, because the block has
                    // to come OUT of the formatting element: the answer is
                    // `<b>1</b><p><b>2</b>3</p>`, where the block is now a
                    // SIBLING of `<b>` and carries a copy of it around the
                    // content it already had. Closing normally instead nested
                    // the block inside `<b>` and left `3` outside the `<p>`
                    // entirely.
                    let furthest_block = match_idx.filter(|_| is_formatting_element(&tag))
                        .and_then(|idx| {
                            stack.iter().enumerate().skip(idx + 1)
                                .find(|(_, f)| is_special_element(&f.parent_tag))
                                .map(|(i, _)| i)
                        });

                    if let (Some(idx), Some(fb_idx)) = (match_idx, furthest_block) {
                        // Everything above the block closes into it as usual.
                        while stack.len() > fb_idx + 1 {
                            let frame = stack.pop().unwrap();
                            if is_formatting_element(&frame.parent_tag) {
                                let mut fresh = self.new_box(&frame.parent_tag);
                                fresh.attributes = frame.node.attributes.clone();
                                fresh.style = frame.node.style.clone();
                                self.pending_format.insert(0, fresh);
                            }
                            let mut node = frame.node;
                            Self::post_process_node(&mut node, &self.base_url);
                            stack.last_mut().unwrap().node.children.push(node);
                        }
                        self.pos += 1;
                        let mut fb = stack.pop().unwrap();
                        // The block keeps its content, wrapped in a copy of the
                        // formatting element it used to be inside.
                        if !fb.node.children.is_empty() {
                            let fmt = &stack[idx].node;
                            let mut wrapper = self.new_box(&tag);
                            wrapper.attributes = fmt.attributes.clone();
                            wrapper.style = fmt.style.clone();
                            wrapper.children = std::mem::take(&mut fb.node.children);
                            fb.node.children = vec![wrapper];
                        }
                        // Anything between the formatting element and the block
                        // closes normally; then the formatting element itself.
                        while stack.len() > idx {
                            let frame = stack.pop().unwrap();
                            let mut node = frame.node;
                            Self::post_process_node(&mut node, &self.base_url);
                            if stack.is_empty() {
                                // The formatting element was the bottom frame —
                                // nothing to reparent into, so fall back to the
                                // ordinary close.
                                *children = node.children;
                                return;
                            }
                            stack.last_mut().unwrap().node.children.push(node);
                        }
                        // The block stays OPEN, now a sibling of what closed.
                        stack.push(fb);
                        continue;
                    }

                    if let Some(idx) = match_idx {
                        // Pop frames from top down to (and including) the match.
                        // Non-matching frames above the match are implicitly closed.
                        while stack.len() > idx + 1 {
                            let frame = stack.pop().unwrap();
                            // A FORMATTING element closed this way is not really
                            // over — it is re-opened after the end tag so the
                            // text that follows keeps its formatting. Popped
                            // innermost-first, so each goes at the FRONT to keep
                            // the original nesting order on the way back in.
                            if is_formatting_element(&frame.parent_tag) {
                                let mut fresh = self.new_box(&frame.parent_tag);
                                fresh.attributes = frame.node.attributes.clone();
                                fresh.style = frame.node.style.clone();
                                self.pending_format.insert(0, fresh);
                            }
                            let mut node = frame.node;
                            Self::post_process_node(&mut node, &self.base_url);
                            stack.last_mut().unwrap().node.children.push(node);
                        }
                        // Now pop the matching frame itself.
                        self.pos += 1;
                        let frame = stack.pop().unwrap();
                        if stack.is_empty() {
                            *children = frame.node.children;
                            *ol_counter = frame.ol_counter;
                            return;
                        }
                        let mut node = frame.node;
                        Self::post_process_node(&mut node, &self.base_url);
                        stack.last_mut().unwrap().node.children.push(node);
                    } else if tag == "p" {
                        // `</p>` with no open `<p>` is not ignored: the spec
                        // inserts an element for a `<p>` START tag and then
                        // closes it, so `<p><div>x</div></p>` ends with an
                        // EMPTY paragraph after the div. Every browser has it.
                        self.pos += 1;
                        let mut node = self.new_box("p");
                        apply_property(&mut node.style, "display", default_display("p"));
                        stack.last_mut().unwrap().node.children.push(node);
                    } else {
                        // Stray close tag with no matching open — ignore it.
                        self.pos += 1;
                    }
                }

                Some(Token::Comment(data)) => {
                    // A comment is a NODE. It was dropped at the token, so
                    // `<div>a<!--x-->b</div>` reached the DOM as `ab` and no
                    // comment could ever be read back or serialised.
                    self.pos += 1;
                    let mut node = self.new_box("#comment");
                    node.text = data;
                    apply_property(&mut node.style, "display", "none");
                    stack.last_mut().unwrap().node.children.push(node);
                }

                Some(Token::Doctype) => {
                    self.pos += 1;
                }

                Some(Token::Text(t)) => {
                    self.pos += 1;
                    let text_val = if preserve_ws {
                        if t.starts_with('\n') { t[1..].to_string() } else { t }
                    } else if t.trim().is_empty() && t.contains('\n') {
                        "\n".to_string()
                    } else {
                        collapse_whitespace(&t)
                    };
                    let keep = !text_val.trim().is_empty()
                        || text_val == " "
                        || text_val == "\n";
                    if keep {
                        // Content arriving is what makes a pending formatting
                        // element real — see `reconstruct`.
                        reconstruct(&mut stack, &mut self.pending_format);
                        let mut text_node = self.new_box("#text");
                        text_node.text = text_val;
                        stack.last_mut().unwrap().node.children.push(text_node);
                    }
                }

                Some(Token::OpenTag { tag, attrs, self_closing }) => {
                    // The BOTTOM frame is the element the CALLER already opened,
                    // so this call cannot pop it — closing it means returning and
                    // letting the caller finish the node and re-read this token at
                    // its own level. The auto-close loop further down only reaches
                    // frames this call pushed (`stack.len() > 1`), so without this
                    // the implicit close silently did not happen for the element a
                    // recursion started on: `<p>a<div>b` nested the div inside the
                    // p, and `<p>a<p>b` nested p in p. `<body><p>a<div>` was right
                    // only because the body arm gives the stack a second frame.
                    // That makes the bug fire on exactly the shape `set_inner_html`
                    // and every generated fragment produce — markup with no <body>.
                    if stack.len() == 1
                        && (should_auto_close(&stack[0].parent_tag, &tag)
                            || (stack[0].parent_tag == "select" && closes_select(&tag)))
                    {
                        break; // token NOT consumed; the caller sees it next
                    }
                    self.pos += 1;

                    // Script/noscript: give the host first chance to handle it.
                    // If the host doesn't handle it: noscript content is shown
                    // as fallback HTML, script is discarded.
                    if matches!(tag.as_str(), "script" | "noscript") {
                        let content = if !self_closing { self.collect_raw_text_until(&tag) } else { String::new() };
                        // The ELEMENT stays in the DOM with its source as a text
                        // child, whatever the host does with the content.
                        // Dropping it meant `document.scripts` was empty and a
                        // `<script>` could not be found, moved or re-read — and
                        // `<script>` is `display: none`, so nothing is drawn.
                        let mut script_node = self.new_box(&tag);
                        script_node.attributes = attrs.clone();
                        script_node.text = content.clone();
                        apply_property(&mut script_node.style, "display", "none");
                        stack.last_mut().unwrap().node.children.push(script_node);
                        let host_handled = if let Some(ref mut f) = self.on_script {
                            f(&tag, &attrs, &content)
                        } else { false };
                        // Scripting is ENABLED, so `<noscript>` is RAWTEXT: its
                        // content is the text above and is NOT parsed. Parsing
                        // it as fallback markup is the scripting-DISABLED
                        // behaviour, and doing both put the same content in the
                        // tree twice — once as text, once as elements.
                        let parse_noscript_fallback = false;
                        if parse_noscript_fallback && !host_handled && tag == "noscript" && !content.is_empty() {
                            // Parse noscript content as HTML and insert into current frame
                            let inner_tokens = tokenize(&content);
                            let mut inner_parser = HtmlParser::new(inner_tokens);
                            inner_parser.base_url = self.base_url.clone();
                            let mut inner_children = Vec::new();
                            let mut inner_ol = 0i32;
                            inner_parser.parse_children_into("", &mut inner_children, &mut inner_ol);
                            for child in inner_children {
                                stack.last_mut().unwrap().node.children.push(child);
                            }
                            // Merge any stylesheets found inside noscript
                            for rule in inner_parser.stylesheet.rules {
                                self.stylesheet.rules.push(rule);
                            }
                        }
                        continue;
                    }

                    // SVG: collect raw markup and rasterize to an <img> node.
                    if tag == "svg" {
                        let svg_body = if !self_closing { self.collect_raw_text_until("svg") } else { String::new() };
                        // Rebuild full SVG markup with attributes
                        let mut svg_tag_str = String::from("<svg");
                        for (k, v) in &attrs {
                            svg_tag_str.push_str(&format!(" {}=\"{}\"", k, v));
                        }
                        if !svg_tag_str.contains("xmlns=") {
                            svg_tag_str.push_str(" xmlns=\"http://www.w3.org/2000/svg\"");
                        }
                        if svg_body.contains("xlink:") && !svg_tag_str.contains("xmlns:xlink") {
                            svg_tag_str.push_str(" xmlns:xlink=\"http://www.w3.org/1999/xlink\"");
                        }
                        svg_tag_str.push('>');
                        let svg_markup = format!("{}{}</svg>", svg_tag_str, svg_body);

                        // Parse viewBox (case-insensitive lookup)
                        let vb_str = attrs.get("viewBox").or_else(|| attrs.get("viewbox"));
                        let vb = parse_viewbox_value(vb_str.map(|s| s.as_str()));
                        let (vb_w, vb_h) = vb.unwrap_or((0, 0));

                        // Check for explicit dimensions from HTML attributes or inline style
                        let explicit_w = attrs.get("style").and_then(|s| style_px(s, "width")).or_else(|| attrs.get("width").and_then(|s| parse_px(s)));
                        let explicit_h = attrs.get("style").and_then(|s| style_px(s, "height")).or_else(|| attrs.get("height").and_then(|s| parse_px(s)));

                        let mut node = self.new_box("svg");
                        node.attributes = attrs;
                        apply_property(&mut node.style, "display", "inline-block");
                        node.svg_markup = Some(svg_markup);
                        node.svg_viewbox_w = vb_w as f32;
                        node.svg_viewbox_h = vb_h as f32;

                        // Only bake explicit HTML-attribute dimensions into the style.
                        // CSS cascade will override these. If no explicit dimensions,
                        // the layout engine uses svg_viewbox_w/h.
                        if let Some(w) = explicit_w {
                            apply_property(&mut node.style, "width", &format!("{}px", w));
                        }
                        if let Some(h) = explicit_h {
                            apply_property(&mut node.style, "height", &format!("{}px", h));
                        }

                        // Don't rasterize here — deferred to render time at the correct display size.
                        stack.last_mut().unwrap().node.children.push(node);
                        continue;
                    }

                    self.fire_hook(&tag, &attrs);

                    // Skip non-visual tags entirely
                    if is_non_visual(&tag) {
                        if !self_closing { self.skip_until_close(&tag); }
                        continue;
                    }
                    // Style block: the CSS goes to the cascade AND the element
                    // stays in the tree.
                    //
                    // Only a `<style>` inside `<template shadowrootmode>` used
                    // to become a node, so `querySelector("style")` answered
                    // something in a shadow template and nothing on an ordinary
                    // page — the same element, present or absent depending on
                    // where it was written. It is `display: none` in the UA
                    // sheet, so keeping it changes nothing that is drawn.
                    //
                    // The template case still skips the cascade: those rules
                    // are scoped to the shadow root and `post_process_node`
                    // lifts them into its stylesheet.
                    if tag == "style" {
                        // The NODE keeps the author's source; only the CASCADE
                        // sees the normalized copy. Storing the normalized text
                        // on the node meant `styleEl.textContent` came back
                        // rewritten — an author's `18pt` read as `24.0000px` —
                        // and a serialize/reparse round-trip rewrote the page's
                        // own stylesheet.
                        let css = self.collect_raw_text_until("style");
                        let cur_parent = stack.last().map(|f| f.parent_tag.as_str()).unwrap_or(parent_tag);
                        if cur_parent != "template" {
                            self.stylesheet.parse_and_add(&normalize_css_text(&css));
                        }
                        let mut style_node = self.new_box("style");
                        style_node.text = css;
                        apply_property(&mut style_node.style, "display", "none");
                        stack.last_mut().unwrap().node.children.push(style_node);
                        continue;
                    }
                    // Title. A `<title>` met outside the head is a parse error
                    // that the spec processes "using the rules for the in head
                    // insertion mode" — so it belongs to the head element, not
                    // to wherever it was written.
                    if tag == "title" {
                        let text = self.collect_raw_text_until("title");
                        self.title = text.trim().to_string();
                        let title = self.title.clone();
                        self.push_head_node("title", attrs, title);
                        continue;
                    }

                    // Build the node
                    let mut node = self.new_box(&tag);
                    node.attributes = attrs;
                    apply_property(&mut node.style, "display", default_display(&tag));
                    apply_presentational_attrs(&mut node);

                    // <img> handling
                    if tag == "img" {
                        // If srcset is present, pick the best URL and override src
                        if let Some(srcset) = node.attributes.get("srcset").cloned() {
                            if let Some(best) = parse_srcset_url(&srcset) {
                                node.attributes.insert("src".to_string(), best);
                            }
                        }
                        if let Some(src) = node.attributes.get("src").cloned() {
                            let resolved = resolve_url(&src, &self.base_url);
                            let is_remote = resolved.starts_with("http://") || resolved.starts_with("https://");
                            if !is_remote {
                                if let Some((data, w, h)) = load_image_from_src(&src, &self.base_url) {
                                    set_image_on_node(&mut node, data, w, h);
                                }
                            }
                            node.resolved_src = resolved;
                        }
                    }
                    // Background image
                    if !node.style.background_image_url.is_empty() {
                        let url = node.style.background_image_url.clone();
                        if let Some((data, w, h)) = load_image_from_src(&url, &self.base_url) {
                            node.bg_image_data   = Some(data);
                            node.bg_image_width  = w;
                            node.bg_image_height = h;
                        }
                    }

                    // List counter (uses the CURRENT frame's counter)
                    {
                        let frame = stack.last_mut().unwrap();
                        if tag == "ol" { frame.ol_counter = 0; }
                        if tag == "li" {
                            frame.ol_counter += 1;
                            node.style.list_index = frame.ol_counter;
                        }
                    }

                    // Summary: always list-item + Disclosure marker
                    if tag == "summary" {
                        node.style.display = Display::ListItem;
                        node.style.list_style_type = ListStyleType::Disclosure;
                    }

                    // `<a>` and `<nobr>` close a still-open element of their own
                    // kind (HTML §13.2.6.4.7 runs the adoption agency for them
                    // on the start tag). `<a href=1>1<a href=2>2</a>` is two
                    // sibling links in every browser; nesting them made the
                    // whole rest of the document a child of the first one.
                    //
                    // Formatting elements between the two are reconstructed, so
                    // `<a>1<b>2<a>3` keeps `3` bold — the same rule as an end
                    // tag, because that is what this effectively is.
                    // "in select" (HTML §13.2.6.4.16): `<input>`, `<keygen>`,
                    // `<textarea>` and a nested `<select>` are parse errors
                    // handled as `</select>`, so the element lands AFTER the
                    // select rather than inside it. Only those four — a `<div>`
                    // in a select stays where it was written.
                    if stack.last().map(|f| f.parent_tag == "select").unwrap_or(false)
                        && closes_select(&tag)
                    {
                        // The `stack.len() == 1` case was handled before the
                        // token was consumed, above.
                        let frame = stack.pop().unwrap();
                        let mut closed = frame.node;
                        Self::post_process_node(&mut closed, &self.base_url);
                        stack.last_mut().unwrap().node.children.push(closed);
                    }

                    // `<html>` and `<body>` start tags met again are parse
                    // errors whose ATTRIBUTES merge onto the existing element;
                    // they never create a second one. Building an element made
                    // `<body class=a>…<body class=b>` give a document two
                    // bodies, one nested in the other.
                    if matches!(tag.as_str(), "html" | "body") {
                        continue;
                    }

                    // A `<form>` while a form is open is IGNORED — the form
                    // element pointer is already set, and the spec inserts
                    // nothing (HTML §13.2.6.4.7). Nesting them gave a document
                    // two form owners for the same controls.
                    if tag == "form" && stack.iter().any(|f| f.parent_tag == "form") {
                        continue;
                    }

                    // `<button>` joins `<a>`/`<nobr>`: a second `<button>` ends
                    // the first rather than nesting inside it.
                    //
                    // `<table>` belongs here too — the "in table" mode closes an
                    // open table on a nested `<table>` start tag — but only when
                    // the open table is not the element this call was started
                    // for. Handing that case back to the caller mid-stack needs
                    // the token to be re-dispatched by the same insertion mode
                    // that opened it, which this parser does not model yet; see
                    // KNOWN_GAPS in `test_tree_construction`.
                    if matches!(tag.as_str(), "a" | "nobr" | "button") {
                        if let Some(idx) = stack.iter().rposition(|f| f.parent_tag == tag) {
                            if idx > 0 {
                                while stack.len() > idx {
                                    let frame = stack.pop().unwrap();
                                    if stack.len() > idx && is_formatting_element(&frame.parent_tag) {
                                        let mut fresh = self.new_box(&frame.parent_tag);
                                        fresh.attributes = frame.node.attributes.clone();
                                        fresh.style = frame.node.style.clone();
                                        self.pending_format.insert(0, fresh);
                                    }
                                    let mut closed = frame.node;
                                    Self::post_process_node(&mut closed, &self.base_url);
                                    stack.last_mut().unwrap().node.children.push(closed);
                                }
                            }
                        }
                    }

                    // HTML implicit closing: certain tags auto-close the
                    // current open element before opening a new one.
                    // Without this, unclosed <li>/<p>/<td>/etc. nest inside
                    // each other, creating absurdly deep DOM trees that
                    // overflow the stack during cascade/layout.
                    while stack.len() > 1 {
                        let cur = stack.last().unwrap().parent_tag.as_str();
                        if should_auto_close(cur, &tag) {
                            let frame = stack.pop().unwrap();
                            let mut closed = frame.node;
                            Self::post_process_node(&mut closed, &self.base_url);
                            stack.last_mut().unwrap().node.children.push(closed);
                        } else {
                            break;
                        }
                    }

                    // After the implicit closes, before the element goes in:
                    // an element start tag is content too, so it re-opens any
                    // formatting element still waiting.
                    reconstruct(&mut stack, &mut self.pending_format);

                    if self_closing {
                        // Void element — post-process then push to current frame.
                        Self::post_process_node(&mut node, &self.base_url);
                        stack.last_mut().unwrap().node.children.push(node);
                    } else {
                        // Non-void — push a new frame; children will be collected
                        // into node.children until the matching close tag.
                        stack.push(Frame {
                            parent_tag: tag,
                            node,
                            ol_counter: 0,
                        });
                    }
                }
            }
        }

        // EOF reached — collapse remaining frames.
        while let Some(frame) = stack.pop() {
            if stack.is_empty() {
                *children = frame.node.children;
                *ol_counter = frame.ol_counter;
            } else {
                let mut node = frame.node;
                Self::post_process_node(&mut node, &self.base_url);
                stack.last_mut().unwrap().node.children.push(node);
            }
        }
    }

    /// Post-processing applied to a node after its children have been parsed.
    fn post_process_node(node: &mut WebCore, base_url: &str) {
        // Declarative Shadow DOM: <template shadowrootmode="open|closed">
        // Convert the template's children into a shadow root on the parent.
        let has_shadow_template = node.children.iter().any(|c|
            c.tag == "template" && c.attributes.contains_key("shadowrootmode"));
        if has_shadow_template {
            let mut shadow_children = Vec::new();
            let mut shadow_css = String::new();
            let mut shadow_mode = crate::types::ShadowMode::Open;
            // Extract the template with shadowrootmode
            node.children.retain(|c| {
                if c.tag == "template" {
                    if let Some(mode) = c.attributes.get("shadowrootmode") {
                        shadow_mode = if mode == "closed" {
                            crate::types::ShadowMode::Closed
                        } else {
                            crate::types::ShadowMode::Open
                        };
                        // Collect template children as shadow tree
                        for child in &c.children {
                            if child.tag == "style" {
                                // Extract style text for scoped stylesheet
                                shadow_css.push_str(&child.text);
                                for tc in &child.children {
                                    if tc.tag == "#text" { shadow_css.push_str(&tc.text); }
                                }
                            } else {
                                shadow_children.push(child.clone());
                            }
                        }
                        return false; // remove the template from light DOM
                    }
                }
                true
            });
            if !shadow_children.is_empty() || !shadow_css.is_empty() {
                // Start with UA stylesheet so shadow tree gets default styles
                let mut stylesheet = crate::css::ua_stylesheet();
                if !shadow_css.is_empty() {
                    // Author origin: a shadow root's own `<style>` outranks the
                    // UA sheet it is layered on, the same as a document's.
                    stylesheet.parse_and_add_author(&shadow_css);
                }
                node.shadow_root = Some(Box::new(crate::types::ShadowRoot {
                    children: shadow_children,
                    stylesheet,
                    mode: shadow_mode,
                }));
            }
        }

        if node.tag == "picture" {
            resolve_picture_source(node, base_url, 0.0, 0.0);
        }
        // <form> inside <table>: browsers treat form as transparent (display:contents)
        // so it doesn't break table row structure.
        if matches!(node.tag.as_str(), "table" | "thead" | "tbody" | "tfoot") {
            for child in &mut node.children {
                if child.tag == "form" {
                    child.style.display = Display::Contents;
                }
            }
        }
        // <select>: keep option children in the DOM for CSS styling.
        // The selected option's text is shown inline; others are display:none.
        // When the dropdown opens, all options are rendered as a popup.
        if node.tag == "select" {
            // `<option selected>` in the markup seeds SELECTEDNESS, exactly as
            // `<input checked>` seeds checkedness, and the attribute stays put
            // as the default a form reset restores to.
            //
            // Then the selectedness setting algorithm decides what a document
            // with no `selected` anywhere shows. ⛔ Its auto-select step is
            // guarded on a display size of 1, so a DROP-DOWN lands on its first
            // enabled option and a LIST BOX is left with nothing selected —
            // which is the state HTML says it starts in. This used to default
            // an index to 0 unconditionally and every list box opened with a
            // highlighted first row.
            crate::html::forms::for_each_option_mut(node, &mut |option, _| {
                option.selectedness = option.attributes.contains_key("selected");
                option.dirty_selectedness = false;
            });
            crate::html::forms::run_selectedness_setting_algorithm(node);


            // The options are hidden either way: a drop-down shows one label,
            // and a list box's rows are painted by the control itself rather
            // than laid out as boxes.
            fn hide_options(node: &mut WebCore) {
                for child in &mut node.children {
                    if matches!(child.tag.as_str(), "option" | "optgroup") {
                        apply_property(&mut child.style, "display", "none");
                        hide_options(child);
                    }
                }
            }
            hide_options(node);

            // ⛔ NO display text node. A drop-down's label is not a child of
            // the select — the author never wrote it, and inventing one put a
            // text node in `childNodes` that doubled `textContent` and came
            // back duplicated through every serialize/reparse round
            // (`<option>Thin</option>Thin` became `ThinThin`).
            //
            // Nothing needed it: the painter reads the label straight off the
            // option whose selectedness is set (`display_list_builder`), which
            // is also the only reading that tracks a selection the user has
            // changed since parse.
            // Set overflow hidden so options don't leak
            apply_property(&mut node.style, "overflow", "hidden");
        }
        // <input>: seed the control's state from its content attributes.
        if node.tag == "input" {
            // `<input checked>` in the markup seeds CHECKEDNESS, and the
            // attribute stays as the default a reset restores to. The
            // `defaultChecked` attribute this used to invent was never a
            // content attribute — `defaultChecked` is the IDL name for the
            // `checked` attribute, which is right here.
            if node.attributes.contains_key("checked") {
                node.checkedness = true;
            }
            // "Invoke the value sanitization algorithm, if one is defined for
            // the type attribute's state." For a range that is what turns a
            // step-mismatched or out-of-bounds `value` into the number the
            // control actually holds, before anything paints or reads it.
            crate::html::forms::seed_input_value(node);
            let input_type = node.attributes.get("type").map(|s| s.as_str()).unwrap_or("text");
            match input_type {
                "submit" | "button" | "reset" => {
                    let label = node.attributes.get("value")
                        .cloned()
                        .unwrap_or_else(|| match input_type {
                            "submit" => "Submit".to_string(),
                            "reset"  => "Reset".to_string(),
                            _ => String::new(),
                        });
                    if !label.is_empty() {
                        node.children.clear();
                        let mut text_node = WebCore::new("#text");
                        text_node.text = label;
                        node.children.push(text_node);
                    }
                }
                "image" => {
                    // Image input: treat src like <img src>
                    if let Some(src) = node.attributes.get("src").cloned() {
                        let resolved = resolve_url(&src, base_url);
                        node.resolved_src = resolved;
                    }
                }
                _ => {}
            }
        }
        if node.tag == "details" {
            let is_open = node.attributes.contains_key("open");
            for child in &mut node.children {
                if child.tag == "summary" {
                    // summary always visible
                } else if !is_open {
                    apply_property(&mut child.style, "display", "none");
                }
            }
        }
    }

    /// Handle a single open tag: create its node, parse its children (iteratively),
    /// apply post-processing, and push the finished node to `children`.
    /// Called from the top-level html/head/body skeleton parser for stray tags.
    fn handle_tag(
        &mut self,
        tag: String,
        attrs: HashMap<String, String>,
        self_closing: bool,
        children: &mut Vec<WebCore>,
        ol_counter: &mut i32,
    ) {
        // Reaching here at all means the token was not `<html>`, `<head>` or
        // `<body>` — it is content, and content closes the head (§13.2.6:
        // "anything else" in "before head" / "in head" pops out to the body).
        // Both of this method's callers are the skeleton parser's fallback
        // arm, so this is the one place that has to say it.
        self.head_closed = true;

        // Script/noscript: give host first chance, else show noscript as HTML.
        if matches!(tag.as_str(), "script" | "noscript") {
            let content = if !self_closing { self.collect_raw_text_until(&tag) } else { String::new() };
            let host_handled = if let Some(ref mut f) = self.on_script {
                f(&tag, &attrs, &content)
            } else { false };
            if !host_handled && tag == "noscript" && !content.is_empty() {
                let inner_tokens = tokenize(&content);
                let mut inner_parser = HtmlParser::new(inner_tokens);
                inner_parser.base_url = self.base_url.clone();
                let mut inner_children = Vec::new();
                let mut inner_ol = *ol_counter;
                inner_parser.parse_children_into("", &mut inner_children, &mut inner_ol);
                children.extend(inner_children);
                for rule in inner_parser.stylesheet.rules {
                    self.stylesheet.rules.push(rule);
                }
            }
            return;
        }
        if is_non_visual(&tag) {
            if !self_closing { self.skip_until_close(&tag); }
            return;
        }
        if tag == "style" {
            let css = self.collect_raw_text_until("style");
            self.stylesheet.parse_and_add(&normalize_css_text(&css));
            // The element stays in the tree — see the sibling arm in
            // `parse_children_into`.
            let mut style_node = self.new_box("style");
            style_node.text = css;
            apply_property(&mut style_node.style, "display", "none");
            children.push(style_node);
            return;
        }
        if tag == "title" {
            let text = self.collect_raw_text_until("title");
            self.title = text.trim().to_string();
            let title = self.title.clone();
            self.push_head_node("title", attrs, title);
            return;
        }

        // SVG: collect raw markup and parse viewBox for intrinsic sizing
        if tag == "svg" {
            let svg_body = if !self_closing { self.collect_raw_text_until("svg") } else { String::new() };
            let mut svg_tag_str = String::from("<svg");
            for (k, v) in &attrs { svg_tag_str.push_str(&format!(" {}=\"{}\"", k, v)); }
            if !svg_tag_str.contains("xmlns=") { svg_tag_str.push_str(" xmlns=\"http://www.w3.org/2000/svg\""); }
            if svg_body.contains("xlink:") && !svg_tag_str.contains("xmlns:xlink") {
                svg_tag_str.push_str(" xmlns:xlink=\"http://www.w3.org/1999/xlink\"");
            }
            svg_tag_str.push('>');
            let svg_markup = format!("{}{}</svg>", svg_tag_str, svg_body);
            let vb_str = attrs.get("viewBox").or_else(|| attrs.get("viewbox"));
            let vb = parse_viewbox_value(vb_str.map(|s| s.as_str()));
            let (vb_w, vb_h) = vb.unwrap_or((0, 0));
            let explicit_w = attrs.get("style").and_then(|s| style_px(s, "width")).or_else(|| attrs.get("width").and_then(|s| parse_px(s)));
            let explicit_h = attrs.get("style").and_then(|s| style_px(s, "height")).or_else(|| attrs.get("height").and_then(|s| parse_px(s)));
            let mut node = self.new_box("svg");
            node.attributes = attrs;
            apply_property(&mut node.style, "display", "inline-block");
            node.svg_markup = Some(svg_markup);
            node.svg_viewbox_w = vb_w as f32;
            node.svg_viewbox_h = vb_h as f32;
            if let Some(w) = explicit_w { apply_property(&mut node.style, "width", &format!("{}px", w)); }
            if let Some(h) = explicit_h { apply_property(&mut node.style, "height", &format!("{}px", h)); }
            apply_presentational_attrs(&mut node);
            children.push(node);
            return;
        }

        let mut node = self.new_box(&tag);
        node.attributes = attrs;
        apply_property(&mut node.style, "display", default_display(&tag));
        apply_presentational_attrs(&mut node);

        if tag == "img" {
            // If srcset is present, pick the best URL from it and override src
            if let Some(srcset) = node.attributes.get("srcset").cloned() {
                if let Some(best) = parse_srcset_url(&srcset) {
                    node.attributes.insert("src".to_string(), best);
                }
            }
            if let Some(src) = node.attributes.get("src").cloned() {
                let resolved = resolve_url(&src, &self.base_url);
                let is_remote = resolved.starts_with("http://") || resolved.starts_with("https://");
                if !is_remote {
                    if let Some((data, w, h)) = load_image_from_src(&src, &self.base_url) {
                        set_image_on_node(&mut node, data, w, h);
                    }
                }
                node.resolved_src = resolved;
            }
        }
        // Canvas/video/audio: set default dimensions from width/height attributes
        if matches!(tag.as_str(), "canvas" | "video" | "audio") {
            let default_w: u32 = if tag == "canvas" { 300 } else { 300 };
            let default_h: u32 = if tag == "canvas" { 150 } else { 150 };
            let w = node.attributes.get("width").and_then(|s| s.parse::<u32>().ok()).unwrap_or(default_w);
            let h = node.attributes.get("height").and_then(|s| s.parse::<u32>().ok()).unwrap_or(default_h);
            if node.style.width.is_auto() {
                node.style.width = crate::types::CssLength::Px(w as f32);
            }
            if node.style.height.is_auto() {
                node.style.height = crate::types::CssLength::Px(h as f32);
            }
            if tag == "canvas" {
                node.image_width = w;
                node.image_height = h;
                // Transparent pixel buffer — ready for drawing
                node.image_data = Some(vec![0u8; (w * h * 4) as usize]);
            }
        }
        if !node.style.background_image_url.is_empty() {
            let url = node.style.background_image_url.clone();
            if let Some((data, w, h)) = load_image_from_src(&url, &self.base_url) {
                node.bg_image_data   = Some(data);
                node.bg_image_width  = w;
                node.bg_image_height = h;
            }
        }
        if tag == "ol" { *ol_counter = 0; }
        if tag == "li" {
            *ol_counter += 1;
            node.style.list_index = *ol_counter;
        }
        if tag == "summary" {
            node.style.display = Display::ListItem;
            node.style.list_style_type = ListStyleType::Disclosure;
        }

        if !self_closing {
            let mut inner_ol = 0i32;
            self.parse_children_into(&tag, &mut node.children, &mut inner_ol);
        }
        Self::post_process_node(&mut node, &self.base_url);

        children.push(node);
    }

    fn collect_raw_text_until(&mut self, end_tag: &str) -> String {
        let mut out = String::new();
        loop {
            match self.tokens.get(self.pos).cloned() {
                None => break,
                Some(Token::CloseTag { tag }) if tag == end_tag => {
                    self.pos += 1;
                    break;
                }
                Some(Token::Text(t)) => {
                    out.push_str(&t);
                    self.pos += 1;
                }
                _ => { self.pos += 1; }
            }
        }
        out
    }

    fn skip_until_close(&mut self, end_tag: &str) {
        let mut depth = 1usize;
        loop {
            match self.tokens.get(self.pos).cloned() {
                None => break,
                Some(Token::OpenTag { tag, .. }) => {
                    if tag == end_tag { depth += 1; }
                    self.pos += 1;
                }
                Some(Token::CloseTag { tag }) => {
                    if tag == end_tag {
                        depth -= 1;
                        self.pos += 1;
                        if depth == 0 { break; }
                    } else {
                        self.pos += 1;
                    }
                }
                _ => { self.pos += 1; }
            }
        }
    }
}

fn collapse_whitespace(s: &str) -> String {
    // Collapse any run of ASCII whitespace (including newlines) to a single space.
    // This is correct for normal (non-pre) HTML content. Whitespace-only text
    // nodes that need to act as line-breaks in `white-space: pre` parents are
    // handled by the call sites, not here.
    let mut out = String::with_capacity(s.len());
    let mut in_ws = false;
    for ch in s.chars() {
        if ch.is_ascii_whitespace() {
            if !in_ws { in_ws = true; }
        } else {
            if in_ws { out.push(' '); in_ws = false; }
            out.push(ch);
        }
    }
    if in_ws { out.push(' '); }
    out
}

/// Parse a `srcset` attribute and return the best URL.
/// For `w` descriptors, picks the smallest available (conservative choice when display size unknown).
/// For `x` descriptors, picks the 1x version (or closest).
/// Falls back to the first entry.
fn parse_srcset_url(srcset: &str) -> Option<String> {
    let mut best_url: Option<String> = None;
    let mut best_w: f32 = f32::MAX;
    let mut best_x: f32 = 0.0;
    let mut has_w = false;
    let mut has_x = false;

    for entry in srcset.split(',') {
        let entry = entry.trim();
        if entry.is_empty() { continue; }
        let mut parts = entry.split_whitespace();
        let url = match parts.next() {
            Some(u) if !u.is_empty() => u,
            _ => continue,
        };
        if let Some(descriptor) = parts.next() {
            if let Some(w_str) = descriptor.strip_suffix('w') {
                has_w = true;
                if let Ok(w) = w_str.parse::<f32>() {
                    if w < best_w {
                        best_w = w;
                        best_url = Some(url.to_string());
                    }
                }
            } else if let Some(x_str) = descriptor.strip_suffix('x') {
                has_x = true;
                if let Ok(x) = x_str.parse::<f32>() {
                    // Prefer 1x, but take largest if no 1x
                    if (x - 1.0).abs() < (best_x - 1.0).abs() || best_url.is_none() {
                        best_x = x;
                        best_url = Some(url.to_string());
                    }
                }
            }
        } else {
            // No descriptor — this is the default candidate
            if !has_w && !has_x {
                best_url = Some(url.to_string());
            }
        }
    }

    // If we had w descriptors but all were webp (skipped), fall back to first parseable
    if best_url.is_none() {
        let entry = srcset.split(',').next()?.trim();
        let url = entry.split_whitespace().next()?;
        if !url.is_empty() { return Some(url.to_string()); }
    }

    best_url
}

/// Resolve the best `<source>` for a `<picture>` element and set it on the child `<img>`.
fn resolve_picture_source(picture: &mut WebCore, base_url: &str, vw: f32, vh: f32) {
    // Find the best matching <source>
    let mut best_url: Option<String> = None;
    let mut best_width: Option<String> = None;
    let mut best_height: Option<String> = None;
    for child in &picture.children {
        if child.tag != "source" { continue; }
        // Skip image/webp — our image decoder may not support it
        if let Some(typ) = child.attributes.get("type") {
            if typ.contains("webp") { continue; }
        }
        // Check media query if present
        if let Some(media) = child.attributes.get("media") {
            if vw > 0.0 || vh > 0.0 {
                if !crate::css::evaluate_media(media, vw, vh) {
                    continue;
                }
            } else {
                // Viewport unknown — skip conditional sources
                continue;
            }
        }
        // Extract URL from srcset
        if let Some(srcset) = child.attributes.get("srcset") {
            if let Some(url) = parse_srcset_url(srcset) {
                best_url = Some(url);
                best_width = child.attributes.get("width").cloned();
                best_height = child.attributes.get("height").cloned();
                break; // First matching source wins
            }
        }
    }

    if let Some(url) = best_url {
        // Find the child <img> and set its src + dimensions from the source
        for child in &mut picture.children {
            if child.tag == "img" {
                // Only the RESOLVED url changes. `src` is the author's content
                // attribute and picking a `<source>` does not rewrite it —
                // `img.src` still reads back what the markup said, and the
                // chosen candidate is what `currentSrc` reports. Overwriting
                // the attribute made `<picture>` mutate the document.
                child.resolved_src = resolve_url(&url, base_url);
                // Transfer width/height from the matched <source> so the image
                // is sized correctly (the <source> often has larger dimensions
                // than the fallback <img>). Applied to the STYLE, not to the
                // width/height content attributes, for the same reason.
                if let Some(ref w) = best_width {
                    crate::css::apply_property(&mut child.style, "width", &format!("{}px", w));
                }
                if let Some(ref h) = best_height {
                    crate::css::apply_property(&mut child.style, "height", &format!("{}px", h));
                }
                break;
            }
        }
    }
}

/// Post-pass: re-resolve `<picture>` elements with real viewport dimensions.
pub fn resolve_picture_elements(node: &mut WebCore, base_url: &str, vw: f32, vh: f32) {
    if node.tag == "picture" {
        resolve_picture_source(node, base_url, vw, vh);
    }
    for child in &mut node.children {
        resolve_picture_elements(child, base_url, vw, vh);
    }
}

fn number_lists(node: &mut WebCore) {
    if node.tag == "ol" {
        let mut idx = 1i32;
        for child in &mut node.children {
            if child.tag == "li" {
                child.style.list_index = idx;
                idx += 1;
            }
        }
    }
    for child in &mut node.children {
        number_lists(child);
    }
}

// ─── Head content parser ─────────────────────────────────────────────────────

fn parse_head_content(parser: &mut HtmlParser) {
    loop {
        match parser.tokens.get(parser.pos).cloned() {
            None => break,
            Some(Token::CloseTag { tag }) if tag == "head" => {
                parser.pos += 1;
                break;
            }
            Some(Token::OpenTag { tag, attrs, self_closing }) => {
                parser.pos += 1;
                if !handle_head_tag(parser, &tag, attrs, self_closing) {
                    // Not head content, but we are inside an explicit <head>:
                    // the spec's "in head" mode ignores it. Skip its subtree so
                    // it does not leak into the head's children.
                    if !self_closing { parser.skip_until_close(&tag); }
                }
            }
            _ => { parser.pos += 1; }
        }
    }
}

/// Is this a tag the "in head" insertion mode owns?
///
/// Used both inside an explicit `<head>` and by the implied-head path — a
/// document that opens with a bare `<style>` and never writes `<head>` still
/// puts that style in the head, which is where `document.head.children` and
/// every browser expect to find it.
fn is_head_content_tag(tag: &str) -> bool {
    matches!(tag, "script" | "noscript" | "style" | "title" | "link" | "meta"
                | "base" | "template")
}

/// Process one "in head" start tag. Returns false if `tag` is not head content.
fn handle_head_tag(
    parser: &mut HtmlParser,
    tag: &str,
    attrs: HashMap<String, String>,
    self_closing: bool,
) -> bool {
    match tag {
        "script" | "noscript" => {
            let content = if !self_closing { parser.collect_raw_text_until(tag) } else { String::new() };
            if let Some(ref mut f) = parser.on_script {
                f(tag, &attrs, &content);
            }
            // In <head>, noscript fallback content is not rendered.
        }
        "style" => {
            parser.fire_hook(tag, &attrs);
            let css = parser.collect_raw_text_until("style");
            parser.stylesheet.parse_and_add(&normalize_css_text(&css));
            parser.push_head_node("style", attrs, css);
        }
        "title" => {
            parser.fire_hook(tag, &attrs);
            let text = parser.collect_raw_text_until("title");
            parser.title = text.trim().to_string();
            let title = parser.title.clone();
            parser.push_head_node("title", attrs, title);
        }
        "link" => {
            let rel  = attrs.get("rel").map(|s| s.as_str()).unwrap_or("");
            let media = attrs.get("media").map(|s| s.as_str()).unwrap_or("");
            let is_print_only = media.eq_ignore_ascii_case("print");
            // Don't fire hook for print-only stylesheets — they
            // shouldn't be fetched/applied in screen rendering.
            if !(rel == "stylesheet" && is_print_only) {
                parser.fire_hook(tag, &attrs);
            }
            let href = attrs.get("href").cloned().unwrap_or_default();
            let media_owned = media.to_string();
            let want_sheet = rel == "stylesheet" && !href.is_empty();
            parser.push_head_node("link", attrs, String::new());
            if want_sheet {
                parser.linked_stylesheets.push((href, media_owned));
            }
        }
        "meta" | "base" => {
            parser.fire_hook(tag, &attrs);
            parser.push_head_node(tag, attrs, String::new());
        }
        "template" => {
            parser.fire_hook(tag, &attrs);
            // The content is parsed and KEPT. `<template>` is head content, and
            // its children are a staged fragment — skipping the subtree the way
            // an unknown head tag is skipped threw the fragment away and let its
            // contents leak into the body.
            let mut node = parser.new_box("template");
            node.attributes = attrs;
            apply_property(&mut node.style, "display", "none");
            if !self_closing {
                let mut children = Vec::new();
                let mut ol = 0i32;
                parser.parse_children_into("template", &mut children, &mut ol);
                node.children = children;
            }
            parser.head_children.push(node);
        }
        _ => return false,
    }
    true
}

// ─── html-level children parser ──────────────────────────────────────────────

fn parse_html_children(
    parser: &mut HtmlParser,
    html_box: &mut WebCore,
    body_box: &mut WebCore,
    body_children: &mut Vec<WebCore>,
    ol_counter: &mut i32,
) {
    loop {
        match parser.tokens.get(parser.pos).cloned() {
            None => break,
            Some(Token::CloseTag { tag }) if tag == "html" => {
                parser.pos += 1;
                break;
            }
            Some(Token::Comment(data)) => {
                parser.pos += 1;
                // A comment BEFORE `<html>` is a child of the Document, not of
                // the body — and this tree has no document node to hold it, so
                // it is dropped rather than moved somewhere it does not belong.
                if parser.head_closed {
                    let mut node = parser.new_box("#comment");
                    node.text = data;
                    apply_property(&mut node.style, "display", "none");
                    body_children.push(node);
                }
            }
            Some(Token::CloseTag { .. }) | Some(Token::Doctype) => {
                parser.pos += 1;
            }
            Some(Token::Text(t)) => {
                parser.pos += 1;
                let collapsed = collapse_whitespace(&t);
                if !collapsed.trim().is_empty() {
                    // Non-blank text is content, so it closes the head too.
                    // Blank text does NOT — whitespace is ignored in "before
                    // head", which is why this sits inside the guard.
                    parser.head_closed = true;
                    let mut node = parser.new_box("#text");
                    node.text = collapsed;
                    let had_pending = !parser.pending_format.is_empty();
                    let from = body_children.len();
                    body_children.push(node);
                    parser.reconstruct_into(body_children, from, had_pending);
                }
            }
            Some(Token::OpenTag { tag, attrs, self_closing }) => {
                parser.pos += 1;
                match tag.as_str() {
                    "head" => {
                        if !parser.head_closed {
                            if !self_closing {
                                parse_head_content(parser);
                            }
                            parser.head_closed = true;
                        }
                        // else: parse error, token ignored — see the sibling
                        // arm in `parse_html_document`.
                    }
                    "body" => {
                        parser.head_closed = true;
                        body_box.attributes = attrs;
                        apply_property(&mut body_box.style, "display", "block");
                        apply_presentational_attrs(body_box);
                        if !self_closing {
                            parser.parse_children_into("body", body_children, ol_counter);
                        }
                    }
                    // Head content met before any body content belongs to the
                    // IMPLIED head (§13.2.6.4.3 "before head" → "in head"),
                    // even though the document never wrote `<head>`. A page
                    // that opens with a bare `<style>` — which most of the
                    // example pages do — was putting it in the body, so
                    // `document.head` was empty and the element sat in the
                    // flow. Chrome puts it in the head; so does this now.
                    _ if !parser.head_closed && is_head_content_tag(&tag) => {
                        handle_head_tag(parser, &tag, attrs, self_closing);
                    }
                    _ => {
                        // Anything else is body content, and body content ends
                        // the implied head.
                        parser.head_closed = true;
                        let had_pending = !parser.pending_format.is_empty();
                        let from = body_children.len();
                        parser.handle_tag(tag, attrs, self_closing, body_children, ol_counter);
                        parser.reconstruct_into(body_children, from, had_pending);
                    }
                }
                // Suppress unused warning for html_box (it's passed by mut for possible future use)
                let _ = &html_box;
            }
        }
    }
}

// ─── Post-cascade fixup ──────────────────────────────────────────────────────

/// After the CSS cascade runs, fix up `<summary>` display and `<details>` open/closed hiding.
/// The UA stylesheet sets `details, summary { display: block }` which overwrites our
/// parse-time settings, so we re-apply them here.
fn apply_details_summary_post_cascade(node: &mut WebCore) {
    if node.tag == "details" {
        let is_open = node.attributes.contains_key("open");
        for child in &mut node.children {
            if child.tag == "summary" {
                child.style.display = Display::ListItem;
                child.style.list_style_type = ListStyleType::Disclosure;
            } else if !is_open {
                child.style.display = Display::None;
            }
        }
    }

    for child in &mut node.children {
        apply_details_summary_post_cascade(child);
    }
}

// ─── Public API ─────────────────────────────────────────────────────────────

/// Parse an HTML string into a Document.
/// Always produces: root = `html` with `body` child.
/// `base_url` is used to resolve relative image paths (pass "" for embedded HTML).
pub fn parse_html(html: &str) -> Document {
    parse_html_with_base(html, "")
}

/// Like `parse_html` but also accepts a base URL/path for resolving relative resources.
pub fn parse_html_with_base(html: &str, base_url: &str) -> Document {
    parse_html_with_hooks(html, base_url, |_, _| {})
}

/// Parse HTML with a host-registered tag hook.
///
/// `hook` is called for **every open tag** as it is parsed, receiving the tag
/// name (lowercase) and its attribute map.  The hook fires before the engine
/// processes the tag, so it is safe to kick off background work (e.g. fetching
/// a stylesheet referenced by a `<link>` tag) while the rest of the document
/// continues to parse.
///
/// ```ignore
/// let doc = parse_html_with_hooks(html, base_url, |tag, attrs| {
///     if tag == "link" && attrs.get("rel").map(|s| s == "stylesheet").unwrap_or(false) {
///         if let Some(href) = attrs.get("href") {
///             start_css_fetch(href);
///         }
///     }
/// });
/// ```
pub fn parse_html_with_hooks<F>(html: &str, base_url: &str, hook: F) -> Document
where
    F: FnMut(&str, &HashMap<String, String>) + 'static,
{
    parse_html_full(html, base_url, Some(Box::new(hook)), None)
}

/// Parse HTML with both a tag hook and a script/noscript callback.
///
/// `on_script` is called for every `<script>` and `<noscript>` tag with
/// `(tag_name, attrs, raw_content)`.  Return `true` if your host handled it
/// (e.g. executed the script).  Return `false` to let the engine apply the
/// default: `<noscript>` fallback content is parsed as HTML and shown,
/// `<script>` is discarded.
///
/// ```ignore
/// let doc = parse_html_with_scripts(html, base_url,
///     |tag, attrs| { /* open-tag hook */ },
///     |tag, attrs, content| {
///         if tag == "script" { my_js_engine.eval(content); true }
///         else { false } // let engine show noscript fallback
///     },
/// );
/// ```
pub fn parse_html_with_scripts<F, S>(html: &str, base_url: &str, hook: F, on_script: S) -> Document
where
    F: FnMut(&str, &HashMap<String, String>) + 'static,
    S: FnMut(&str, &HashMap<String, String>, &str) -> bool + 'static,
{
    parse_html_full(html, base_url, Some(Box::new(hook)), Some(Box::new(on_script)))
}

fn parse_html_full(
    html: &str,
    base_url: &str,
    on_open_tag: Option<Box<dyn FnMut(&str, &HashMap<String, String>) + 'static>>,
    on_script: Option<Box<dyn FnMut(&str, &HashMap<String, String>, &str) -> bool + 'static>>,
) -> Document {
    // SVG blocks are now handled inline by the tokenizer/parser — no pre-pass needed.
    let tokens = tokenize(html);
    let mut parser = HtmlParser::new(tokens);
    parser.base_url = base_url.to_string();
    parser.on_open_tag = on_open_tag;
    parser.on_script = on_script;

    // Always create html > body structure
    let mut html_box = parser.new_box("html");
    apply_property(&mut html_box.style, "display", "block");

    let mut body_box = parser.new_box("body");
    apply_property(&mut body_box.style, "display", "block");

    let mut body_children: Vec<WebCore> = Vec::new();
    let mut ol_counter = 0i32;

    while parser.pos < parser.tokens.len() {
        match parser.tokens.get(parser.pos).cloned() {
            None => break,
            Some(Token::Doctype) => { parser.pos += 1; }
            Some(Token::Comment(data)) => {
                // A comment at document level is a NODE like any other. It was
                // dropped here even after comments became nodes elsewhere, so
                // `<p>x</p><!--c-->` lost its trailing comment.
                parser.pos += 1;
                // A comment BEFORE `<html>` is a child of the Document, not of
                // the body — and this tree has no document node to hold it, so
                // it is dropped rather than moved somewhere it does not belong.
                if parser.head_closed {
                    let mut node = parser.new_box("#comment");
                    node.text = data;
                    apply_property(&mut node.style, "display", "none");
                    body_children.push(node);
                }
            }
            Some(Token::Text(t)) => {
                parser.pos += 1;
                let collapsed = collapse_whitespace(&t);
                if !collapsed.trim().is_empty() {
                    // Non-blank text is content, so it closes the head too.
                    // Blank text does NOT — whitespace is ignored in "before
                    // head", which is why this sits inside the guard.
                    parser.head_closed = true;
                    let mut node = parser.new_box("#text");
                    node.text = collapsed;
                    let had_pending = !parser.pending_format.is_empty();
                    let from = body_children.len();
                    body_children.push(node);
                    parser.reconstruct_into(&mut body_children, from, had_pending);
                }
            }
            Some(Token::CloseTag { tag }) => {
                parser.pos += 1;
                if tag == "html" || tag == "body" { break; }
                // `</p>` with nothing open inserts an empty paragraph — the
                // same rule as inside an element, and documents hit it at top
                // level too (`<p>a</p></p>` ends with an empty `<p>`).
                if tag == "p" {
                    parser.head_closed = true;
                    let mut node = parser.new_box("p");
                    apply_property(&mut node.style, "display", default_display("p"));
                    body_children.push(node);
                }
            }
            Some(Token::OpenTag { tag, attrs, self_closing }) => {
                parser.pos += 1;
                match tag.as_str() {
                    "html" => {
                        // Apply attrs to html box
                        html_box.attributes = attrs;
                        apply_property(&mut html_box.style, "display", "block");
                        apply_presentational_attrs(&mut html_box);
                        // Continue parsing html children
                        if !self_closing {
                            parse_html_children(
                                &mut parser,
                                &mut html_box,
                                &mut body_box,
                                &mut body_children,
                                &mut ol_counter,
                            );
                        }
                    }
                    "head" => {
                        if !parser.head_closed {
                            if !self_closing {
                                parse_head_content(&mut parser);
                            }
                            parser.head_closed = true;
                        }
                        // else: parse error, token ignored. Whatever follows
                        // is body content and falls through to the arms below
                        // on the next iteration.
                    }
                    "body" => {
                        parser.head_closed = true;
                        // Apply attrs to body box
                        body_box.attributes = attrs;
                        apply_property(&mut body_box.style, "display", "block");
                        apply_presentational_attrs(&mut body_box);
                        // Parse body children
                        if !self_closing {
                            parser.parse_children_into("body", &mut body_children, &mut ol_counter);
                        }
                    }
                    // Head content before any body content belongs to the
                    // IMPLIED head — see the matching arm in
                    // `parse_html_children`. Most of the example pages open
                    // with a bare `<style>` and no `<head>` at all, and it was
                    // landing in the body.
                    _ if !parser.head_closed && is_head_content_tag(&tag) => {
                        handle_head_tag(&mut parser, &tag, attrs, self_closing);
                    }
                    _ => {
                        // Content outside html/body goes into body, and ends
                        // the implied head.
                        parser.head_closed = true;
                        let had_pending = !parser.pending_format.is_empty();
                        let from = body_children.len();
                        parser.handle_tag(tag, attrs, self_closing, &mut body_children, &mut ol_counter);
                        parser.reconstruct_into(&mut body_children, from, had_pending);
                    }
                }
            }
        }
    }

    body_box.children = body_children;

    // `<head>` is optional in the MARKUP and mandatory in the TREE. HTML
    // §13.2.6's "before head" insertion mode inserts one whether or not the
    // source wrote it, which is why `document.head` always answers an element
    // in a browser — and why it answered `None` here for every document.
    //
    // It takes its display from the UA table like every other element — head
    // is already listed there as `none`, so nothing new is being decided here.
    // The element exists so the DOM can name it; the box draws nothing and
    // takes no space. Its CONTENTS are still consumed by the parser as before
    // — the title is lifted into `Document.title`, stylesheets into the
    // cascade — so this adds the container the spec requires without changing
    // what is rendered.
    let mut head_box = parser.new_box("head");
    apply_property(&mut head_box.style, "display", default_display("head"));
    head_box.children = std::mem::take(&mut parser.head_children);

    // §13.2.6.4.19 "in frameset" — a `<frameset>` REPLACES the body: the
    // document element gets `head` and `frameset`, and there is no body at all.
    // The frameset was landing inside a body, so `document.body` answered an
    // element a frameset document does not have and the frames were nested a
    // level too deep.
    let frameset = body_box.children.iter().position(|c| c.tag == "frameset");
    if let Some(at) = frameset {
        let mut fs = body_box.children.remove(at);
        apply_property(&mut fs.style, "display", "block");
        html_box.children = vec![head_box, fs];
    } else {
        html_box.children = vec![head_box, body_box];
    }

    // §13.2.6.4.9 — table structure. Runs on the finished tree rather than
    // inside the element stack because both halves need a table's PARENT:
    // foster parenting moves a node out to become the table's sibling, and it
    // has to happen before the rows are grouped or the stray content would be
    // sealed inside the `<tbody>` that grouping creates.
    normalize_tables(&mut html_box);
    unwrap_misplaced_table_parts(&mut html_box);

    // Wire arena parent-child relationships to mirror the WebCore tree.
    wire_arena_children(&mut parser.arena, &html_box);

    // Build combined stylesheet (UA + author)
    let mut stylesheet = ua_stylesheet();
    // Author rules must always win over UA rules regardless of selector
    // specificity — see `css::AUTHOR_ORIGIN_BOOST`.
    stylesheet.push_author_rules(parser.stylesheet.rules);
    for (k, v) in parser.stylesheet.variables {
        stylesheet.variables.insert(k, v);
    }
    stylesheet.raw_sources.extend(parser.stylesheet.raw_sources);
    stylesheet.keyframes.extend(parser.stylesheet.keyframes);

    let title = parser.title.clone();
    let linked_stylesheets = parser.linked_stylesheets.clone();

    let mut doc = Document {
        root: html_box,
        nodes: crate::types::NodeArena::new(),
        nodes_stale: true,
        stylesheet,
        title,
        base_url: base_url.to_string(),
        arena: parser.arena,
        next_node_id: parser.next_node_id,
        node_index: std::collections::HashMap::new(),
        // This function IS the HTML parser, so whatever it produces is an HTML
        // document by construction.
        kind: crate::types::DocumentKind::Html,
        layout_store: crate::layout::layout_box::LayoutStore::new(),
        pending_nodes: std::collections::HashMap::new(),
        linked_stylesheets,
        editor: crate::dom::Editor::new(),
        canvas_surfaces: crate::canvas::CanvasSurfaces::default(),
        events: crate::dom::EventListeners::new(),
        event_targets: crate::dom::events::EventTargetMap::new(),
        scroll_x: 0.0,
        scroll_y: 0.0,
        scrollbar_drag: None,
        hovered_box:       0,
        hover_suppress_count: 0,
        active_box:        0,
        focused_box:       0,
        mousedown_target:  0,
        last_click_target: 0,
        last_click_time:   None,
        drag_source:       0,
        drag_start_doc_pt: (0.0, 0.0),
        drag_active:       false,
        visited_urls:      std::collections::HashSet::new(),
        viewport_w:        0.0,
        viewport_h:        0.0,
        keyboard_focus:    false,
        caret_blink_epoch: std::time::Instant::now(), open_select: 0,
        open_picker: 0, dropdown_hover_idx: -1,
        // Transient interaction state, like the two popups beside it: a freshly
        // parsed document is holding nothing.
        dragging_range: 0, range_drag_origin: String::new(),
        active_animations:     Vec::new(),
        transition_states:     std::collections::HashMap::new(),
        prev_styles:           std::collections::HashMap::new(),
        animation_overrides:   std::collections::HashMap::new(),
        needs_animation_frame: false,
        hover_changed:         false,
            hover_sensitive_nodes: std::collections::HashSet::new(),
        style_dirty:           false,
        prev_hovered_box:      0,
        cascade_styles:        std::collections::HashMap::new(),
        pending_announcements:    Vec::new(),
        live_region_snapshots:    std::collections::HashMap::new(),
        live_regions_initialized: false,
        layout_generation:   0,
        pending_images:      None,
        images_in_flight:    std::sync::Arc::new(std::sync::atomic::AtomicUsize::new(0)),
        on_form_event: None, on_navigate: None, on_title_change: None, on_dom_mutation: None, on_visibility_change: None,
    };

    // NOTE: External CSS fetching, cascade, layout, and image loading are
    // handled by the caller (lib.rs load_html_with_registry) which does
    // parallel CSS fetching and batch image loading. We only do a minimal
    // cascade here for the standalone parse_html / parse_html_with_base paths.
    //
    // Fetch local-only linked stylesheets (file:// paths).
    // Remote stylesheets are handled by lib.rs in parallel.
    for (href, media) in &doc.linked_stylesheets.clone() {
        // Skip print-only stylesheets for screen rendering
        if media.eq_ignore_ascii_case("print") { continue; }
        let url = resolve_url(href, base_url);
        if !url.starts_with("http://") && !url.starts_with("https://") && !url.is_empty() {
            if let Ok(css_text) = std::fs::read_to_string(&url) {
                doc.stylesheet.parse_and_add(&css_text);
            }
        }
    }

    // Apply cascade (basic pass — lib.rs re-runs with viewport dimensions)
    let root_font_px = 16.0;
    doc.stylesheet.rebuild_index();
    apply_cascade(&mut doc.root, &doc.stylesheet, None, root_font_px);

    // Post-cascade fixes
    apply_details_summary_post_cascade(&mut doc.root);
    number_lists(&mut doc.root);

    // Populate linked-list sibling pointers on every node
    populate_sibling_links(&mut doc.root);

    doc
}

// ─── Arena wiring ────────────────────────────────────────────────────────────

/// Walk the WebCore tree and wire arena parent-child links to mirror it.
/// Called once after parsing is complete and the full WebCore tree is built.
fn wire_arena_children(arena: &mut crate::dom::arena::DomArena, root: &WebCore) {
    use crate::dom::arena::NodeId;
    let root_id = NodeId(root.node_id);
    if root_id.is_none() || !arena.is_alive(root_id) { return; }
    for child in &root.children {
        let child_id = NodeId(child.node_id);
        if child_id.is_none() || !arena.is_alive(child_id) { continue; }
        // Set text content on arena text nodes
        if child.tag == "#text" {
            arena.get_mut(child_id).text = child.text.clone();
        }
        // Copy attributes to arena node
        for (k, v) in &child.attributes {
            arena.get_mut(child_id).attributes.insert(k.clone(), v.clone());
        }
        arena.append_child(root_id, child_id);
        wire_arena_children(arena, child);
    }
}

/// Rebuild arena from an existing WebCore tree (e.g. after clone or DOM mutation).
/// Creates fresh arena nodes for every WebCore and wires parent-child links.
pub fn rebuild_arena_from_tree(arena: &mut crate::dom::arena::DomArena, root: &mut WebCore) {
    *arena = crate::dom::arena::DomArena::new();
    rebuild_arena_recursive(arena, root);
}

fn rebuild_arena_recursive(arena: &mut crate::dom::arena::DomArena, node: &mut WebCore) {
    use crate::dom::arena::NodeId;
    // Create arena node
    let arena_id = if node.tag == "#text" {
        arena.create_text(&node.text)
    } else {
        let id = arena.create_element(&node.tag);
        for (k, v) in &node.attributes {
            arena.get_mut(id).attributes.insert(k.clone(), v.clone());
        }
        id
    };
    node.node_id = arena_id.0;

    // Recurse children
    for child in &mut node.children {
        rebuild_arena_recursive(arena, child);
        let child_id = NodeId(child.node_id);
        arena.append_child(arena_id, child_id);
    }
    // Populate linked-list pointers on WebCore (second pass — all node_ids assigned)
    populate_sibling_links(node);
}

/// Populate parent/first_child/last_child/next_sibling/prev_sibling on a node
/// and all its Vec children. Called after node_ids are assigned.
pub fn populate_sibling_links(node: &mut WebCore) {
    let parent_id = node.node_id;
    let n = node.children.len();
    if n == 0 {
        node.first_child = 0;
        node.last_child = 0;
        return;
    }
    node.first_child = node.children[0].node_id;
    node.last_child = node.children[n - 1].node_id;
    for i in 0..n {
        node.children[i].parent = parent_id;
        node.children[i].prev_sibling = if i > 0 { node.children[i - 1].node_id } else { 0 };
        node.children[i].next_sibling = if i + 1 < n { node.children[i + 1].node_id } else { 0 };
    }
    // Recurse
    for child in &mut node.children {
        populate_sibling_links(child);
    }
}

/// Rebuild arena nodes for a subtree and append each child to `parent_arena_id`.
/// Used by `dom_set_inner_html` to wire new children into the existing arena.
pub fn rebuild_arena_recursive_pub(
    arena: &mut crate::dom::arena::DomArena,
    node: &mut WebCore,
    parent_arena_id: crate::dom::arena::NodeId,
) {
    rebuild_arena_recursive(arena, node);
    let child_id = crate::dom::arena::NodeId(node.node_id);
    arena.append_child(parent_arena_id, child_id);
}

// ─── Serialization ───────────────────────────────────────────────────────────

pub fn serialize_html(doc: &Document) -> String {
    serializer::serialize_html(doc)
}
