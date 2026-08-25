pub mod serializer;
pub mod streaming;

use std::collections::HashMap;
use crate::types::{HtmlBox, Document, Display, ListStyleType};
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
pub fn load_background_images(node: &mut HtmlBox, base_url: &str) {
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
pub fn set_image_on_node(node: &mut HtmlBox, data: Vec<u8>, w: u32, h: u32) {
    node.image_data   = Some(data);
    node.image_width  = w;
    node.image_height = h;
}

/// Set decoded image (raster or SVG) on an img node.
/// SVGs are stored as markup for deferred rasterization at the correct display size.
pub fn set_decoded_image_on_node(node: &mut HtmlBox, decoded: DecodedImage) {
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

pub fn decode_entities(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut i = 0;
    while i < s.len() {
        let b = s.as_bytes()[i];
        if b != b'&' {
            // Find next '&' or end, copy the whole chunk at once.
            let start = i;
            i += 1;
            while i < s.len() && s.as_bytes()[i] != b'&' { i += 1; }
            out.push_str(&s[start..i]);
            continue;
        }
        // Look ahead for ';' within a reasonable range (HTML entity names are short).
        // If no ';' found, treat '&' as literal.
        let after = i + 1;
        match s[after..].find(';') {
            Some(semi) if semi <= 32 => {
                let name = &s[after..after + semi];
                out.push_str(&resolve_entity(name));
                i = after + semi + 1;
            }
            _ => {
                out.push('&');
                i += 1;
            }
        }
    }
    out
}

fn resolve_entity(name: &str) -> String {
    if name.starts_with('#') {
        let code_str = &name[1..];
        let code = if code_str.starts_with('x') || code_str.starts_with('X') {
            u32::from_str_radix(&code_str[1..], 16).ok()
        } else {
            code_str.parse::<u32>().ok()
        };
        if let Some(cp) = code {
            if let Some(c) = char::from_u32(cp) {
                return c.to_string();
            }
        }
        return format!("&#{};", name);
    }
    let ch = match name {
        "amp"   => "&",  "lt"    => "<",  "gt"    => ">",
        "quot"  => "\"", "apos"  => "'",  "nbsp"  => "\u{00A0}",
        "copy"  => "©",  "reg"   => "®",  "trade" => "™",
        "mdash" => "—",  "ndash" => "–",  "hellip"=> "…",
        "laquo" => "«",  "raquo" => "»",  "lsaquo"=> "‹",  "rsaquo"=> "›",
        "lsquo" => "\u{2018}", "rsquo" => "\u{2019}",
        "ldquo" => "\u{201C}", "rdquo" => "\u{201D}",
        "bull"  => "•",  "middot"=> "·",
        "times" => "×",  "divide"=> "÷",
        "eacute"=> "é",  "egrave"=> "è",  "ecirc" => "ê",  "euml"  => "ë",
        "aacute"=> "á",  "agrave"=> "à",  "acirc" => "â",  "auml"  => "ä",
        "oacute"=> "ó",  "ograve"=> "ò",  "ocirc" => "ô",  "ouml"  => "ö",
        "uacute"=> "ú",  "ugrave"=> "ù",  "ucirc" => "û",  "uuml"  => "ü",
        "iacute"=> "í",  "igrave"=> "ì",  "icirc" => "î",  "iuml"  => "ï",
        "ntilde"=> "ñ",  "ccedil"=> "ç",  "szlig" => "ß",
        "aring" => "å",  "aelig" => "æ",  "oslash"=> "ø",
        "alpha" => "α",  "beta"  => "β",  "gamma" => "γ",  "delta" => "δ",
        "euro"  => "€",  "pound" => "£",  "yen"   => "¥",  "cent"  => "¢",
        _       => return format!("&{};", name),
    };
    ch.to_string()
}

// ─── Tokenizer ─────────────────────────────────────────────────────────────

#[derive(Debug, Clone)]
enum Token {
    Text(String),
    OpenTag  { tag: String, attrs: HashMap<String, String>, self_closing: bool },
    CloseTag { tag: String },
    Comment,
    Doctype,
}

fn tokenize(html: &str) -> Vec<Token> {
    let mut tokens = Vec::new();
    let mut i = 0;
    let bytes = html.as_bytes();

    while i < bytes.len() {
        if bytes[i] == b'<' {
            if html[i..].starts_with("<!--") {
                let end = html[i..].find("-->").map(|e| i + e + 3).unwrap_or(html.len());
                tokens.push(Token::Comment);
                i = end;
                continue;
            }
            if i + 9 <= bytes.len() && bytes[i..i+9].eq_ignore_ascii_case(b"<!doctype") {
                let end = html[i..].find('>').map(|e| i + e + 1).unwrap_or(html.len());
                tokens.push(Token::Doctype);
                i = end;
                continue;
            }
            if i + 1 < bytes.len() && bytes[i + 1] == b'/' {
                let end = html[i..].find('>').map(|e| i + e + 1).unwrap_or(html.len());
                let inner = &html[i + 2..end.saturating_sub(1)];
                let tag = inner.trim().to_ascii_lowercase();
                tokens.push(Token::CloseTag { tag });
                i = end;
                continue;
            }
            let end = find_tag_end(html, i);
            let tag_src = &html[i + 1..end.saturating_sub(1)];
            let self_closing = tag_src.trim_end().ends_with('/');
            let tag_src = tag_src.trim_end_matches('/').trim();
            let (tag, attrs) = parse_tag_attrs(tag_src);
            let is_void = is_void_element(&tag);
            tokens.push(Token::OpenTag { tag: tag.clone(), attrs, self_closing: self_closing || is_void });
            i = end;
            // Raw text / foreign content elements: content must not be parsed as HTML.
            // <svg> is foreign content — collect everything until </svg> as raw text
            // so inner SVG elements (path, circle, etc.) don't interfere with HTML parsing.
            if matches!(tag.as_str(), "style" | "script" | "noscript" | "svg") && !(self_closing || is_void) {
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
            map.insert(name, String::new());
            continue;
        }
        i += 1;
        while i < bytes.len() && bytes[i].is_ascii_whitespace() { i += 1; }
        let value = if i < bytes.len() && (bytes[i] == b'"' || bytes[i] == b'\'') {
            let q = bytes[i];
            i += 1;
            let start = i;
            while i < bytes.len() && bytes[i] != q { i += 1; }
            let v = decode_entities(&s[start..i]);
            if i < bytes.len() { i += 1; }
            v
        } else {
            let start = i;
            while i < bytes.len() && !bytes[i].is_ascii_whitespace() && bytes[i] != b'>' { i += 1; }
            decode_entities(&s[start..i])
        };
        map.insert(name, value);
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
        | "form" | "fieldset" | "legend" | "hgroup" | "search"
            => "block",
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
        | "option" | "optgroup" | "datalist" => "none",
        // Everything else is inline
        _ => "inline",
    }
}

// ─── Apply presentational attributes ───────────────────────────────────────

fn apply_presentational_attrs(node: &mut HtmlBox) {
    let attrs = node.attributes.clone();
    let tag = node.tag.clone();

    // Translate body `text` attribute to `color` attribute so the cascade picks it up
    if tag == "body" {
        if let Some(text_color) = attrs.get("text") {
            let text_color = text_color.clone();
            node.attributes.entry("color".to_string()).or_insert(text_color);
        }
    }

    for (attr, val) in &attrs {
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
            "rows" if tag == "textarea" => {
                if let Ok(n) = val.parse::<f32>() {
                    let h = n * 1.4;
                    let style_str = format!("height:{}em", h);
                    let existing = node.attributes.get("style").cloned().unwrap_or_default();
                    node.attributes.insert("style".into(), if existing.is_empty() { style_str } else { format!("{};{}", existing, style_str) });
                }
            }
            "cols" if tag == "textarea" => {
                if let Ok(n) = val.parse::<f32>() {
                    let w = n * 0.6;
                    let style_str = format!("width:{}em", w);
                    let existing = node.attributes.get("style").cloned().unwrap_or_default();
                    node.attributes.insert("style".into(), if existing.is_empty() { style_str } else { format!("{};{}", existing, style_str) });
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

fn apply_inline_style(node: &mut HtmlBox, css: &str) {
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
    /// Arena-based DOM being built in parallel with the HtmlBox tree.
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
        }
    }

    /// Create an HtmlBox with a fresh sequential node_id.
    /// Also creates the corresponding node in the arena.
    #[inline]
    fn new_box(&mut self, tag: &str) -> HtmlBox {
        let mut b = HtmlBox::new(tag);
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
        children: &mut Vec<HtmlBox>,
        ol_counter: &mut i32,
    ) {
        // Each frame represents one nesting level.
        struct Frame {
            parent_tag: String,
            node:       HtmlBox,   // the element whose children we're collecting
            ol_counter: i32,
        }

        // Bottom frame collects the top-level children that will be returned.
        let mut stack: Vec<Frame> = Vec::new();
        stack.push(Frame {
            parent_tag: parent_tag.to_string(),
            node:       HtmlBox::new("__root__"),  // temporary container, no arena node needed
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

                    if let Some(idx) = match_idx {
                        // Pop frames from top down to (and including) the match.
                        // Non-matching frames above the match are implicitly closed.
                        while stack.len() > idx + 1 {
                            let frame = stack.pop().unwrap();
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
                    } else {
                        // Stray close tag with no matching open — ignore it.
                        self.pos += 1;
                    }
                }

                Some(Token::Comment) | Some(Token::Doctype) => {
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
                        let mut text_node = self.new_box("#text");
                        text_node.text = text_val;
                        stack.last_mut().unwrap().node.children.push(text_node);
                    }
                }

                Some(Token::OpenTag { tag, attrs, self_closing }) => {
                    self.pos += 1;

                    // Script/noscript: give the host first chance to handle it.
                    // If the host doesn't handle it: noscript content is shown
                    // as fallback HTML, script is discarded.
                    if matches!(tag.as_str(), "script" | "noscript") {
                        let content = if !self_closing { self.collect_raw_text_until(&tag) } else { String::new() };
                        let host_handled = if let Some(ref mut f) = self.on_script {
                            f(&tag, &attrs, &content)
                        } else { false };
                        if !host_handled && tag == "noscript" && !content.is_empty() {
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
                    // Style block: extract CSS
                    // Inside <template shadowrootmode>, keep <style> as a node
                    // (will be extracted into shadow stylesheet by post_process_node)
                    if tag == "style" {
                        let css = self.collect_raw_text_until("style");
                        let css = normalize_css_text(&css);
                        // Inside <template shadowrootmode>, keep <style> as a node
                        // (will be extracted into shadow stylesheet by post_process_node)
                        let cur_parent = stack.last().map(|f| f.parent_tag.as_str()).unwrap_or(parent_tag);
                        if cur_parent == "template" {
                            let mut style_node = self.new_box("style");
                            style_node.text = css;
                            stack.last_mut().unwrap().node.children.push(style_node);
                        } else {
                            self.stylesheet.parse_and_add(&css);
                        }
                        continue;
                    }
                    // Title
                    if tag == "title" {
                        let text = self.collect_raw_text_until("title");
                        self.title = text.trim().to_string();
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
                            node.attributes.insert("_resolved_src".to_string(), resolved);
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
    fn post_process_node(node: &mut HtmlBox, base_url: &str) {
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
                    stylesheet.parse_and_add(&shadow_css);
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
            let mut selected_idx: usize = 0;
            let mut _found_selected = false;
            let mut opt_count = 0usize;
            // Find which option is selected
            for child in &node.children {
                if child.tag == "option" {
                    if child.attributes.contains_key("selected") {
                        selected_idx = opt_count;
                        _found_selected = true;
                    }
                    opt_count += 1;
                } else if child.tag == "optgroup" {
                    for gc in &child.children {
                        if gc.tag == "option" {
                            if gc.attributes.contains_key("selected") {
                                selected_idx = opt_count;
                                _found_selected = true;
                            }
                            opt_count += 1;
                        }
                    }
                }
            }
            node.data.insert("_selected_idx".into(), selected_idx.to_string());
            // Set all options to display:none except put the selected text as a visible child
            // Keep the original options for the dropdown popup
            let mut selected_text = String::new();
            opt_count = 0;
            for child in &mut node.children {
                if child.tag == "option" {
                    if opt_count == selected_idx {
                        // Extract text for display
                        for tc in &child.children {
                            if tc.tag == "#text" { selected_text.push_str(&tc.text); }
                        }
                    }
                    // All options hidden — shown only in dropdown popup
                    apply_property(&mut child.style, "display", "none");
                    opt_count += 1;
                } else if child.tag == "optgroup" {
                    apply_property(&mut child.style, "display", "none");
                    for gc in &mut child.children {
                        if gc.tag == "option" {
                            if opt_count == selected_idx {
                                for tc in &gc.children {
                                    if tc.tag == "#text" { selected_text.push_str(&tc.text); }
                                }
                            }
                            opt_count += 1;
                        }
                    }
                }
            }
            // Add a display text node for the currently selected option
            let mut display_node = HtmlBox::new("#text");
            display_node.text = selected_text.trim().to_string();
            node.children.push(display_node);
            // Set overflow hidden so options don't leak
            apply_property(&mut node.style, "overflow", "hidden");
        }
        // <input>: save defaultValue for form reset, create text child for buttons
        if node.tag == "input" {
            // Save original value for form reset
            if let Some(val) = node.attributes.get("value").cloned() {
                node.attributes.entry("defaultValue".to_string()).or_insert(val);
            }
            // `<input checked>` in the markup seeds CHECKEDNESS, and the
            // attribute stays as the default a reset restores to. The
            // `defaultChecked` attribute this used to invent was never a
            // content attribute — `defaultChecked` is the IDL name for the
            // `checked` attribute, which is right here.
            if node.attributes.contains_key("checked") {
                node.checkedness = true;
            }
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
                        let mut text_node = HtmlBox::new("#text");
                        text_node.text = label;
                        node.children.push(text_node);
                    }
                }
                "image" => {
                    // Image input: treat src like <img src>
                    if let Some(src) = node.attributes.get("src").cloned() {
                        let resolved = resolve_url(&src, base_url);
                        node.attributes.insert("_resolved_src".into(), resolved);
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
        children: &mut Vec<HtmlBox>,
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
            let css = normalize_css_text(&css);
            self.stylesheet.parse_and_add(&css);
            return;
        }
        if tag == "title" {
            let text = self.collect_raw_text_until("title");
            self.title = text.trim().to_string();
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
                node.attributes.insert("_resolved_src".to_string(), resolved);
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
fn resolve_picture_source(picture: &mut HtmlBox, base_url: &str, vw: f32, vh: f32) {
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
                let resolved = resolve_url(&url, base_url);
                child.attributes.insert("src".to_string(), url);
                child.attributes.insert("_resolved_src".to_string(), resolved);
                // Transfer width/height from the matched <source> so the image
                // is sized correctly (the <source> often has larger dimensions
                // than the fallback <img>).
                if let Some(ref w) = best_width {
                    child.attributes.insert("width".to_string(), w.clone());
                    // Also set CSS style so layout picks it up after cascade
                    crate::css::apply_property(&mut child.style, "width", &format!("{}px", w));
                }
                if let Some(ref h) = best_height {
                    child.attributes.insert("height".to_string(), h.clone());
                    crate::css::apply_property(&mut child.style, "height", &format!("{}px", h));
                }
                break;
            }
        }
    }
}

/// Post-pass: re-resolve `<picture>` elements with real viewport dimensions.
pub fn resolve_picture_elements(node: &mut HtmlBox, base_url: &str, vw: f32, vh: f32) {
    if node.tag == "picture" {
        resolve_picture_source(node, base_url, vw, vh);
    }
    for child in &mut node.children {
        resolve_picture_elements(child, base_url, vw, vh);
    }
}

fn number_lists(node: &mut HtmlBox) {
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
                match tag.as_str() {
                    "script" | "noscript" => {
                        let content = if !self_closing { parser.collect_raw_text_until(&tag) } else { String::new() };
                        if let Some(ref mut f) = parser.on_script {
                            f(&tag, &attrs, &content);
                        }
                        // In <head>, noscript fallback content is not rendered.
                    }
                    "style" => {
                        parser.fire_hook(&tag, &attrs);
                        let css = parser.collect_raw_text_until("style");
                        let css = normalize_css_text(&css);
                        parser.stylesheet.parse_and_add(&css);
                    }
                    "title" => {
                        parser.fire_hook(&tag, &attrs);
                        let text = parser.collect_raw_text_until("title");
                        parser.title = text.trim().to_string();
                    }
                    "link" => {
                        let rel  = attrs.get("rel").map(|s| s.as_str()).unwrap_or("");
                        let media = attrs.get("media").map(|s| s.as_str()).unwrap_or("");
                        let is_print_only = media.eq_ignore_ascii_case("print");
                        // Don't fire hook for print-only stylesheets — they
                        // shouldn't be fetched/applied in screen rendering.
                        if !(rel == "stylesheet" && is_print_only) {
                            parser.fire_hook(&tag, &attrs);
                        }
                        let href = attrs.get("href").cloned().unwrap_or_default();
                        if rel == "stylesheet" && !href.is_empty() {
                            parser.linked_stylesheets.push((href, media.to_string()));
                        }
                    }
                    _ => {
                        parser.fire_hook(&tag, &attrs);
                        if !self_closing {
                            parser.skip_until_close(&tag);
                        }
                    }
                }
            }
            _ => { parser.pos += 1; }
        }
    }
}

// ─── html-level children parser ──────────────────────────────────────────────

fn parse_html_children(
    parser: &mut HtmlParser,
    html_box: &mut HtmlBox,
    body_box: &mut HtmlBox,
    body_children: &mut Vec<HtmlBox>,
    ol_counter: &mut i32,
) {
    loop {
        match parser.tokens.get(parser.pos).cloned() {
            None => break,
            Some(Token::CloseTag { tag }) if tag == "html" => {
                parser.pos += 1;
                break;
            }
            Some(Token::CloseTag { .. }) | Some(Token::Comment) | Some(Token::Doctype) => {
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
                    body_children.push(node);
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
                    _ => {
                        parser.handle_tag(tag, attrs, self_closing, body_children, ol_counter);
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
fn apply_details_summary_post_cascade(node: &mut HtmlBox) {
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

    let mut body_children: Vec<HtmlBox> = Vec::new();
    let mut ol_counter = 0i32;

    while parser.pos < parser.tokens.len() {
        match parser.tokens.get(parser.pos).cloned() {
            None => break,
            Some(Token::Comment) | Some(Token::Doctype) => { parser.pos += 1; }
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
                    body_children.push(node);
                }
            }
            Some(Token::CloseTag { tag }) => {
                parser.pos += 1;
                if tag == "html" || tag == "body" { break; }
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
                    _ => {
                        // Content outside html/body goes into body
                        parser.handle_tag(tag, attrs, self_closing, &mut body_children, &mut ol_counter);
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

    html_box.children = vec![head_box, body_box];

    // Wire arena parent-child relationships to mirror the HtmlBox tree.
    wire_arena_children(&mut parser.arena, &html_box);

    // Build combined stylesheet (UA + author)
    let mut stylesheet = ua_stylesheet();
    // Author rules must always win over UA rules regardless of selector specificity.
    // Boost every author rule's specificity by a large constant so that even
    // `* { padding: 0 }` (sp=0+100_000) beats `ul { padding-left: 40px }` (sp=1).
    for mut rule in parser.stylesheet.rules {
        rule.specificity = rule.specificity.saturating_add(100_000);
        stylesheet.rules.push(rule);
    }
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

/// Walk the HtmlBox tree and wire arena parent-child links to mirror it.
/// Called once after parsing is complete and the full HtmlBox tree is built.
fn wire_arena_children(arena: &mut crate::dom::arena::DomArena, root: &HtmlBox) {
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

/// Rebuild arena from an existing HtmlBox tree (e.g. after clone or DOM mutation).
/// Creates fresh arena nodes for every HtmlBox and wires parent-child links.
pub fn rebuild_arena_from_tree(arena: &mut crate::dom::arena::DomArena, root: &mut HtmlBox) {
    *arena = crate::dom::arena::DomArena::new();
    rebuild_arena_recursive(arena, root);
}

fn rebuild_arena_recursive(arena: &mut crate::dom::arena::DomArena, node: &mut HtmlBox) {
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
    // Populate linked-list pointers on HtmlBox (second pass — all node_ids assigned)
    populate_sibling_links(node);
}

/// Populate parent/first_child/last_child/next_sibling/prev_sibling on a node
/// and all its Vec children. Called after node_ids are assigned.
pub fn populate_sibling_links(node: &mut HtmlBox) {
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
    node: &mut HtmlBox,
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
