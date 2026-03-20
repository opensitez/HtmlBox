pub mod serializer;

use std::collections::HashMap;
use std::io::Read as _;
use crate::types::{HtmlBox, Document, Display, ListStyleType};
use crate::css::{Stylesheet, apply_property, apply_cascade, ua_stylesheet};

// ─── SVG extraction ────────────────────────────────────────────────────────

/// Pre-pass: extract `<svg>…</svg>` blocks and replace with `<img>` placeholders.
/// Returns (processed_html, map_of_key→svg_markup).
fn extract_svg_blocks(html: &str) -> (String, HashMap<String, String>) {
    // Quick check: skip lowercasing and allocation if no SVGs present
    if crate::css::find_case_insensitive(html, "<svg") .is_none() {
        return (String::new(), HashMap::new());
    }
    let mut result = String::with_capacity(html.len());
    let mut svg_map: HashMap<String, String> = HashMap::new();
    let mut pos = 0usize;
    let mut svg_idx = 0u32;

    while pos < html.len() {
        // Find next <svg (case-insensitive, no full-string lowercase)
        let svg_start = match crate::css::find_case_insensitive(&html[pos..], "<svg") {
            Some(offset) => pos + offset,
            None => {
                result.push_str(&html[pos..]);
                break;
            }
        };

        // Copy everything before <svg>
        result.push_str(&html[pos..svg_start]);

        // Find closing </svg>
        let svg_end = match crate::css::find_case_insensitive(&html[svg_start..], "</svg>") {
            Some(offset) => svg_start + offset + 6, // include </svg>
            None => {
                // Malformed: no closing tag, emit rest as-is
                result.push_str(&html[svg_start..]);
                pos = html.len(); // consumed everything
                let _ = pos;
                break;
            }
        };

        // Extract and patch <svg> tag if needed
        let tag_end = html[svg_start..].find('>').map(|o| svg_start + o + 1).unwrap_or(svg_end);
        let mut svg_tag = html[svg_start..tag_end].to_string();
        // Patch in xmlns if missing
        if !svg_tag.contains("xmlns=") {
            if let Some(insert_pos) = svg_tag.rfind('>') {
                svg_tag.insert_str(insert_pos, " xmlns=\"http://www.w3.org/2000/svg\"");
            }
        }
        let svg_body = &html[tag_end..svg_end];
        // Patch in xmlns:xlink if the body uses xlink: but the tag doesn't declare it
        if svg_body.contains("xlink:") && !svg_tag.contains("xmlns:xlink") {
            if let Some(insert_pos) = svg_tag.rfind('>') {
                svg_tag.insert_str(insert_pos, " xmlns:xlink=\"http://www.w3.org/1999/xlink\"");
            }
        }
        // Rebuild svg_markup with patched tag
        let svg_markup = format!("{}{}", svg_tag, svg_body);

        // Parse all attributes from the SVG opening tag in one pass
        let tag_content = svg_tag.trim_start_matches('<').trim_end_matches('>');
        let (_, attrs) = parse_tag_attrs(tag_content);

        let vb = parse_viewbox_value(attrs.get("viewbox").map(|s| s.as_str()));
        // Inline style width/height take precedence over HTML attributes
        let style_w = attrs.get("style").and_then(|s| style_px(s, "width"));
        let style_h = attrs.get("style").and_then(|s| style_px(s, "height"));
        let attr_w = attrs.get("width").and_then(|s| parse_px(s));
        let attr_h = attrs.get("height").and_then(|s| parse_px(s));
        let explicit_w = style_w.or(attr_w);
        let explicit_h = style_h.or(attr_h);

        let (w, h) = resolve_replaced_size(explicit_w, explicit_h, vb);

        let key = format!("__svg_{}__", svg_idx);
        svg_idx += 1;
        svg_map.insert(key.clone(), svg_markup);

        // Build <img> with all SVG attributes forwarded as data-svg-* plus sizing
        let mut img_tag = format!(
            "<img src=\"{}\" width=\"{}\" height=\"{}\" data-svg-w=\"{}\" data-svg-h=\"{}\"",
            key, w, h, w, h
        );
        for (k, v) in &attrs {
            match k.as_str() {
                "width" | "height" | "xmlns" | "xmlns:xlink" | "version" => {}
                _ => {
                    img_tag.push_str(&format!(" data-svg-{}=\"{}\"", k, v));
                }
            }
        }
        img_tag.push('>');
        result.push_str(&img_tag);

        pos = svg_end;
    }

    (result, svg_map)
}

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

/// Resolve replaced element dimensions per CSS Images §5.2.
/// With both explicit → use them. One explicit + viewBox ratio → derive the other.
/// Neither explicit → fit viewBox ratio into 300×150 default object size.
fn resolve_replaced_size(ew: Option<u32>, eh: Option<u32>, vb: Option<(u32, u32)>) -> (u32, u32) {
    match (ew, eh) {
        (Some(w), Some(h)) => (w, h),
        (Some(w), None) => {
            let h = vb.map(|(vw, vh)| (w as f64 * vh as f64 / vw.max(1) as f64) as u32)
                .unwrap_or(w);
            (w, h)
        }
        (None, Some(h)) => {
            let w = vb.map(|(vw, vh)| (h as f64 * vw as f64 / vh.max(1) as f64) as u32)
                .unwrap_or(h);
            (w, h)
        }
        (None, None) => {
            if let Some((vw, vh)) = vb {
                let ratio = vw as f64 / vh.max(1) as f64;
                let (fw, fh) = if ratio > (300.0 / 150.0) {
                    (300, (300.0 / ratio).round() as u32)
                } else {
                    ((150.0 * ratio).round() as u32, 150)
                };
                (fw.max(1), fh.max(1))
            } else {
                (300, 150)
            }
        }
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
    for child in &mut node.children {
        load_background_images(child, base_url);
    }
}

fn rasterize_svgs(node: &mut HtmlBox, svg_map: &HashMap<String, String>) {
    if node.tag == "img" {
        if let Some(src) = node.attributes.get("src") {
            if src.starts_with("__svg_") {
                if let Some(svg_source) = svg_map.get(src) {
                    let w = node.attributes.get("data-svg-w")
                        .and_then(|s| s.parse().ok())
                        .unwrap_or_else(|| if node.image_width > 0 { node.image_width } else { 200 });
                    let h = node.attributes.get("data-svg-h")
                        .and_then(|s| s.parse().ok())
                        .unwrap_or_else(|| if node.image_height > 0 { node.image_height } else { 150 });
                    eprintln!("[SVG] rasterizing {} ({}x{}) markup len={}", src, w, h, svg_source.len());
                    if let Some(rgba) = rasterize_svg_to_rgba(svg_source, w, h) {
                        eprintln!("[SVG] success: {} bytes of RGBA data", rgba.len());
                        node.image_data = Some(rgba);
                        node.image_width = w;
                        node.image_height = h;
                        node.svg_markup = Some(svg_source.clone());
                        // Ensure dimensions are set in style
                        if node.style.width.is_auto() {
                            apply_property(&mut node.style, "width", &format!("{}px", w));
                        }
                        if node.style.height.is_auto() {
                            apply_property(&mut node.style, "height", &format!("{}px", h));
                        }
                    }
                }
            }
        }
    }
    for child in &mut node.children {
        rasterize_svgs(child, svg_map);
    }
}

/// Rasterize an SVG string to RGBA pixel data using resvg.
fn rasterize_svg_to_rgba(svg: &str, width: u32, height: u32) -> Option<Vec<u8>> {
    use resvg::usvg;
    eprintln!("[SVG] full markup ({} bytes):\n{}", svg.len(), svg);
    let opt = usvg::Options::default();
    let tree = match usvg::Tree::from_str(svg, &opt) {
        Ok(t) => t,
        Err(e) => {
            eprintln!("[SVG] usvg parse error: {}", e);
            return None;
        }
    };
    let size = tree.size();
    eprintln!("[SVG] tree size: {}x{}", size.width(), size.height());
    let sx = width as f32 / size.width();
    let sy = height as f32 / size.height();
    let transform = resvg::tiny_skia::Transform::from_scale(sx, sy);
    let mut pixmap = match resvg::tiny_skia::Pixmap::new(width, height) {
        Some(p) => p,
        None => {
            eprintln!("[SVG] pixmap creation failed for {}x{}", width, height);
            return None;
        }
    };
    resvg::render(&tree, transform, &mut pixmap.as_mut());
    // resvg outputs premultiplied RGBA; convert to straight alpha
    let pma = pixmap.data();
    let mut rgba = Vec::with_capacity(pma.len());
    for chunk in pma.chunks(4) {
        let (pr, pg, pb, a) = (chunk[0] as u32, chunk[1] as u32, chunk[2] as u32, chunk[3]);
        if a == 0 {
            rgba.extend_from_slice(&[0, 0, 0, 0]);
        } else {
            let a32 = a as u32;
            rgba.push(((pr * 255 + a32 / 2) / a32).min(255) as u8);
            rgba.push(((pg * 255 + a32 / 2) / a32).min(255) as u8);
            rgba.push(((pb * 255 + a32 / 2) / a32).min(255) as u8);
            rgba.push(a);
        }
    }
    Some(rgba)
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
fn resolve_url(src: &str, base_url: &str) -> String {
    if src.contains("://") {
        return src.to_string();
    }
    if base_url.starts_with("http://") || base_url.starts_with("https://") {
        if src.starts_with("//") {
            // Protocol-relative URL
            let scheme = if base_url.starts_with("https") { "https:" } else { "http:" };
            return format!("{}{}", scheme, src);
        }
        if src.starts_with('/') {
            if let Some(slash3) = base_url.find("://").and_then(|i| base_url[i+3..].find('/').map(|j| i+3+j)) {
                return format!("{}{}", &base_url[..slash3], src);
            }
            return format!("{}{}", base_url.trim_end_matches('/'), src);
        }
        // Relative path
        if let Some(last_slash) = base_url.rfind('/') {
            return format!("{}{}", &base_url[..=last_slash], src);
        }
    }
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

/// Set decoded image data on a node, applying width/height/aspect-ratio.
pub fn set_image_on_node(node: &mut HtmlBox, data: Vec<u8>, w: u32, h: u32) {
    node.image_data   = Some(data);
    node.image_width  = w;
    node.image_height = h;
    let w_auto = node.style.width.is_auto();
    let h_auto = node.style.height.is_auto();
    if w_auto && h_auto {
        apply_property(&mut node.style, "width",  &format!("{}px", w));
        apply_property(&mut node.style, "height", &format!("{}px", h));
    } else if w_auto && h > 0 {
        let specified_h = node.style.height.resolve(16.0, 0.0, 16.0);
        let ratio_w = (specified_h * w as f32 / h as f32).round() as u32;
        apply_property(&mut node.style, "width", &format!("{}px", ratio_w));
    } else if h_auto && w > 0 {
        let specified_w = node.style.width.resolve(16.0, 0.0, 16.0);
        let ratio_h = (specified_w * h as f32 / w as f32).round() as u32;
        apply_property(&mut node.style, "height", &format!("{}px", ratio_h));
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
        let bytes = ureq::get(&path)
            .timeout(std::time::Duration::from_secs(10))
            .call().ok()
            .and_then(|r| {
                let mut buf = Vec::new();
                r.into_reader().read_to_end(&mut buf).ok()?;
                Some(buf)
            })?;
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

pub fn decode_image_bytes(bytes: &[u8]) -> Option<(Vec<u8>, u32, u32)> {
    use image::ImageReader;
    use std::io::Cursor;
    let reader = ImageReader::new(Cursor::new(bytes))
        .with_guessed_format()
        .ok()?;
    let img = reader.decode().ok()?;
    let rgba = img.to_rgba8();
    let (w, h) = rgba.dimensions();
    Some((rgba.into_raw(), w, h))
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
    let mut chars = s.chars().peekable();
    while let Some(ch) = chars.next() {
        if ch != '&' {
            out.push(ch);
            continue;
        }
        let rest: String = chars.by_ref().take_while(|&c| c != ';').collect();
        out.push_str(&resolve_entity(&rest));
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
            // Raw text elements: <style> and <script> content must not be parsed as HTML.
            // Collect everything until the matching close tag as a single Text token.
            if (tag == "style" || tag == "script") && !(self_closing || is_void) {
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
        | "link" | "meta" | "param" | "source" | "track" | "wbr")
}

/// Elements whose content should be completely suppressed (no box, no text)
fn is_non_visual(tag: &str) -> bool {
    matches!(tag, "head" | "meta" | "link" | "script" | "noscript")
}

// ─── Default display ────────────────────────────────────────────────────────

fn default_display(tag: &str) -> &'static str {
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
        "img" | "svg" => "inline-block",
        "input" | "button" | "select" | "textarea" => "inline-block",
        "ruby" => "ruby",
        "rt"   => "ruby-text",
        // Non-visual: display:none
        "head" | "style" | "script" | "title" | "meta" | "link" | "noscript" => "none",
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
    linked_stylesheets: Vec<String>,
    /// Optional host-registered hook, fired for every open tag as it is parsed.
    /// Receives the tag name and its attribute map.
    on_open_tag: Option<Box<dyn FnMut(&str, &HashMap<String, String>) + 'static>>,
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
            on_open_tag: None,
        }
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
    fn parse_children_into(
        &mut self,
        parent_tag: &str,
        children: &mut Vec<HtmlBox>,
        ol_counter: &mut i32,
    ) {
        // Elements that require literal whitespace preservation (CSS white-space: pre).
        let preserve_ws = matches!(parent_tag, "pre" | "textarea" | "listing" | "xmp" | "plaintext");
        loop {
            match self.tokens.get(self.pos).cloned() {
                None => break,
                Some(Token::CloseTag { tag }) => {
                    if parent_tag.is_empty() || tag == parent_tag {
                        self.pos += 1;
                    }
                    break;
                }
                Some(Token::Comment) | Some(Token::Doctype) => {
                    self.pos += 1;
                }
                Some(Token::Text(t)) => {
                    self.pos += 1;
                    let text_val = if preserve_ws {
                        // Preserve literal text including newlines.
                        // Per the HTML spec, a single newline immediately after the
                        // opening tag of a pre element is stripped.
                        if t.starts_with('\n') { t[1..].to_string() } else { t }
                    } else if t.trim().is_empty() && t.contains('\n') {
                        // Whitespace-only inter-element text that contains a newline —
                        // keep as "\n" so `white-space: pre` parents get a line break.
                        "\n".to_string()
                    } else {
                        collapse_whitespace(&t)
                    };
                    let keep = !text_val.trim().is_empty()
                        || text_val == " "
                        || text_val == "\n";
                    if keep {
                        let mut node = HtmlBox::new("#text");
                        node.text = text_val;
                        children.push(node);
                    }
                }
                Some(Token::OpenTag { tag, attrs, self_closing }) => {
                    self.pos += 1;
                    self.fire_hook(&tag, &attrs);
                    self.handle_tag(tag, attrs, self_closing, children, ol_counter);
                }
            }
        }
    }

    fn handle_tag(
        &mut self,
        tag: String,
        attrs: HashMap<String, String>,
        self_closing: bool,
        children: &mut Vec<HtmlBox>,
        ol_counter: &mut i32,
    ) {
        // Skip non-visual tags entirely (no box, no text)
        if is_non_visual(&tag) {
            if !self_closing {
                self.skip_until_close(&tag);
            }
            return;
        }

        // Style block: extract CSS
        if tag == "style" {
            let css = self.collect_raw_text_until("style");
            let css = normalize_css_text(&css);
            self.stylesheet.parse_and_add(&css);
            return;
        }

        // Title: extract text, store in title
        if tag == "title" {
            let text = self.collect_raw_text_until("title");
            self.title = text.trim().to_string();
            return;
        }

        // Create the box
        let mut node = HtmlBox::new(tag.clone());
        node.attributes = attrs;

        // Apply default display
        apply_property(&mut node.style, "display", default_display(&tag));

        // Apply presentational attributes
        apply_presentational_attrs(&mut node);

        // Load image data for <img> elements
        if tag == "img" {
            // Swap data-src into src for lazy-loaded images (the real URL replaces the placeholder)
            if let Some(ds) = node.attributes.get("data-src").cloned() {
                if !ds.is_empty() {
                    node.attributes.insert("src".to_string(), ds);
                }
            }
            if let Some(src) = node.attributes.get("src").cloned() {
                let resolved = resolve_url(&src, &self.base_url);
                let is_remote = resolved.starts_with("http://") || resolved.starts_with("https://");
                // Only load non-remote images during parsing; remote images
                // are batch-fetched in parallel after parsing (see lib.rs).
                if !is_remote {
                    if let Some((data, w, h)) = load_image_from_src(&src, &self.base_url) {
                        set_image_on_node(&mut node, data, w, h);
                    }
                }
                // Store the resolved URL for deferred loading
                node.attributes.insert("_resolved_src".to_string(), resolved);
            }
        }

        // Load background image data for any element with background-image: url(...)
        if !node.style.background_image_url.is_empty() {
            let url = node.style.background_image_url.clone();
            if let Some((data, w, h)) = load_image_from_src(&url, &self.base_url) {
                node.bg_image_data   = Some(data);
                node.bg_image_width  = w;
                node.bg_image_height = h;
            }
        }

        // List counter
        if tag == "ol" {
            *ol_counter = 0;
        }
        if tag == "li" {
            *ol_counter += 1;
            node.style.list_index = *ol_counter;
        }

        // Summary: always list-item + Disclosure marker
        if tag == "summary" {
            node.style.display = Display::ListItem;
            node.style.list_style_type = ListStyleType::Disclosure;
        }

        // Recurse children
        if !self_closing {
            let mut inner_ol = 0i32;
            self.parse_children_into(&tag, &mut node.children, &mut inner_ol);

            // Details: handle open/closed state
            if tag == "details" {
                let is_open = node.attributes.contains_key("open");
                for child in &mut node.children {
                    if child.tag == "summary" {
                        // summary always visible
                    } else if !is_open {
                        // hide non-summary children when closed
                        apply_property(&mut child.style, "display", "none");
                    }
                }
            }
        }

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
                parser.fire_hook(&tag, &attrs);
                match tag.as_str() {
                    "style" => {
                        let css = parser.collect_raw_text_until("style");
                        let css = normalize_css_text(&css);
                        parser.stylesheet.parse_and_add(&css);
                    }
                    "title" => {
                        let text = parser.collect_raw_text_until("title");
                        parser.title = text.trim().to_string();
                    }
                    "link" => {
                        // Collect <link rel="stylesheet" href="..."> for the host to fetch.
                        let rel  = attrs.get("rel").map(|s| s.as_str()).unwrap_or("");
                        let href = attrs.get("href").cloned().unwrap_or_default();
                        if rel == "stylesheet" && !href.is_empty() {
                            parser.linked_stylesheets.push(href);
                        }
                        // <link> is void — no closing tag to skip.
                    }
                    _ => {
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
                    let mut node = HtmlBox::new("#text");
                    node.text = collapsed;
                    body_children.push(node);
                }
            }
            Some(Token::OpenTag { tag, attrs, self_closing }) => {
                parser.pos += 1;
                match tag.as_str() {
                    "head" => {
                        if !self_closing {
                            parse_head_content(parser);
                        }
                    }
                    "body" => {
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
pub fn parse_html_with_hooks<F>(html: &str, base_url: &str, mut hook: F) -> Document
where
    F: FnMut(&str, &HashMap<String, String>) + 'static,
{
    // Pre-pass: extract <svg> blocks, replace with <img> placeholders
    let (processed_html, svg_map) = extract_svg_blocks(html);
    let html_to_parse = if svg_map.is_empty() { html } else { &processed_html };

    let tokens = tokenize(html_to_parse);
    let mut parser = HtmlParser::new(tokens);
    parser.base_url = base_url.to_string();
    parser.on_open_tag = Some(Box::new(move |tag, attrs| hook(tag, attrs)));

    // Always create html > body structure
    let mut html_box = HtmlBox::new("html");
    apply_property(&mut html_box.style, "display", "block");

    let mut body_box = HtmlBox::new("body");
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
                    let mut node = HtmlBox::new("#text");
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
                        if !self_closing {
                            parse_head_content(&mut parser);
                        }
                    }
                    "body" => {
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
    html_box.children = vec![body_box];

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
    stylesheet.keyframes.extend(parser.stylesheet.keyframes);

    let title = parser.title.clone();
    let linked_stylesheets = parser.linked_stylesheets.clone();

    let mut doc = Document {
        root: html_box,
        stylesheet,
        title,
        base_url: base_url.to_string(),
        linked_stylesheets,
        editor: crate::dom::Editor::new(),
        events: crate::dom::EventListeners::new(),
        scroll_x: 0.0,
        scroll_y: 0.0,
        scrollbar_drag: None,
        hovered_box:       std::ptr::null(),
        active_box:        std::ptr::null(),
        focused_box:       std::ptr::null(),
        mousedown_target:  std::ptr::null(),
        last_click_target: std::ptr::null(),
        last_click_time:   None,
        drag_source:       std::ptr::null(),
        drag_start_doc_pt: (0.0, 0.0),
        drag_active:       false,
        visited_urls:      std::collections::HashSet::new(),
        viewport_w:        0.0,
        viewport_h:        0.0,
        keyboard_focus:    false,
        active_animations:     Vec::new(),
        transition_states:     std::collections::HashMap::new(),
        prev_styles:           std::collections::HashMap::new(),
        animation_overrides:   std::collections::HashMap::new(),
        needs_animation_frame: false,
        hover_changed:         false,
        cascade_styles:        std::collections::HashMap::new(),
        pending_announcements:    Vec::new(),
        live_region_snapshots:    std::collections::HashMap::new(),
        live_regions_initialized: false,
    };

    // NOTE: External CSS fetching, cascade, layout, and image loading are
    // handled by the caller (lib.rs load_html_with_registry) which does
    // parallel CSS fetching and batch image loading. We only do a minimal
    // cascade here for the standalone parse_html / parse_html_with_base paths.
    //
    // Fetch local-only linked stylesheets (file:// paths).
    // Remote stylesheets are handled by lib.rs in parallel.
    for href in &doc.linked_stylesheets.clone() {
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

    // Post-pass: rasterize SVG placeholders into image data
    if !svg_map.is_empty() {
        rasterize_svgs(&mut doc.root, &svg_map);
    }

    doc
}

// ─── Serialization ───────────────────────────────────────────────────────────

pub fn serialize_html(doc: &Document) -> String {
    serializer::serialize_html(doc)
}
