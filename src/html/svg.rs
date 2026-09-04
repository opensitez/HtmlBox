//! SVG extraction and rasterization.

#![allow(unused_imports)]
use super::*;
use crate::css::*;
use crate::types::*;

// ─── SVG extraction ────────────────────────────────────────────────────────

/// Pre-pass: extract `<svg>…</svg>` blocks and replace with `<img>` placeholders.
/// Returns (processed_html, map_of_key→svg_markup).
// SVG blocks are now handled inline by the tokenizer (collected as raw text)
// and rasterized by the parser when building the DOM tree.

/// Parse leading integer pixels from a string like "20px", "512", "20px;height:10px".
pub(crate) fn parse_px(s: &str) -> Option<u32> {
    let num: String = s
        .trim()
        .chars()
        .take_while(|c| c.is_ascii_digit())
        .collect();
    if num.is_empty() {
        None
    } else {
        num.parse().ok()
    }
}

/// Extract a CSS pixel value for `prop` from an inline style string (e.g. "width:20px;height:20px").
pub(crate) fn style_px(style: &str, prop: &str) -> Option<u32> {
    let lower = style.to_ascii_lowercase();
    let needle = format!("{}:", prop);
    let idx = lower.find(&needle)?;
    let after = style[idx + needle.len()..].trim_start();
    parse_px(after)
}

/// Parse a viewBox attribute value "min-x min-y width height" → (width, height).
pub(crate) fn parse_viewbox_value(val: Option<&str>) -> Option<(u32, u32)> {
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
            node.bg_image_data = Some(data);
            node.bg_image_width = w;
            node.bg_image_height = h;
        }
    }
    // Load mask-image (CSS masking for icons etc.)
    if node.mask_image_data.is_none() && !node.style.rare().mask_image_url.is_empty() {
        let url = node.style.rare().mask_image_url.clone();
        if let Some((data, w, h)) = load_image_from_src(&url, base_url) {
            node.mask_image_data = Some(data);
            node.mask_image_width = w;
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
