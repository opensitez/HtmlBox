pub mod block;
pub mod inline_layout;
pub mod text;
pub mod flex;
pub mod grid;
pub mod table;
pub mod hit_test;
pub mod layout_box;

use std::cell::Cell;
use crate::types::*;

// ─── Font loading helpers ──────────────────────────────────────────────────────

/// WOFF2 magic bytes: `wOF2` (0x774F4632).
const WOFF2_MAGIC: [u8; 4] = [0x77, 0x4F, 0x46, 0x32];

/// Split a CSS `src:` value into individual source entries, respecting
/// parentheses so that `data:` URIs (which contain commas) are not split.
fn split_font_sources(src: &str) -> Vec<&str> {
    let mut result = Vec::new();
    let mut depth = 0;
    let mut start = 0;
    for (i, c) in src.char_indices() {
        match c {
            '(' => depth += 1,
            ')' => { if depth > 0 { depth -= 1; } }
            ',' if depth == 0 => {
                result.push(&src[start..i]);
                start = i + 1;
            }
            _ => {}
        }
    }
    if start < src.len() {
        result.push(&src[start..]);
    }
    result
}

/// Load raw font bytes into the font system, with format detection.
/// WOFF2 is detected and skipped (it requires Brotli decompression which is not
/// currently bundled; convert to TTF/OTF/WOFF1 for use with @font-face).
/// WOFF1 magic bytes: `wOFF` (0x774F4646).
const WOFF1_MAGIC: [u8; 4] = [0x77, 0x4F, 0x46, 0x46];

fn load_font_bytes(fs: &mut cosmic_text::FontSystem, data: Vec<u8>) {
    if data.starts_with(&WOFF2_MAGIC) {
        return;
    }
    let font_data = if data.starts_with(&WOFF1_MAGIC) {
        match decode_woff1(&data) {
            Some(ttf) => ttf,
            None => return,
        }
    } else {
        data
    };
    fs.db_mut().load_font_data(font_data);
}

/// Decode WOFF1 container to raw OpenType/TrueType.
/// WOFF1 wraps each OT table with optional zlib compression.
fn decode_woff1(data: &[u8]) -> Option<Vec<u8>> {
    use flate2::read::ZlibDecoder;

    if data.len() < 44 { return None; }

    let r32 = |off: usize| -> u32 {
        u32::from_be_bytes([data[off], data[off+1], data[off+2], data[off+3]])
    };
    let r16 = |off: usize| -> u16 {
        u16::from_be_bytes([data[off], data[off+1]])
    };

    let _signature = r32(0);       // 'wOFF'
    let flavor     = r32(4);       // original sfVersion (e.g. 0x00010000 for TrueType)
    let _length    = r32(8);       // total WOFF file size
    let num_tables = r16(12);
    let _reserved  = r16(14);
    let total_sfnt = r32(16) as usize; // total size of uncompressed font
    // bytes 20..44: version, metadata, private data offsets (not needed)

    // Each table directory entry is 20 bytes, starting at offset 44
    struct TableEntry {
        tag:             [u8; 4],
        offset:          usize,
        comp_length:     usize,
        orig_length:     usize,
        orig_checksum:   u32,
    }
    let mut entries = Vec::with_capacity(num_tables as usize);
    for i in 0..num_tables as usize {
        let base = 44 + i * 20;
        if base + 20 > data.len() { return None; }
        let mut tag = [0u8; 4];
        tag.copy_from_slice(&data[base..base+4]);
        entries.push(TableEntry {
            tag,
            offset:       r32(base + 4) as usize,
            comp_length:  r32(base + 8) as usize,
            orig_length:  r32(base + 12) as usize,
            orig_checksum: r32(base + 16),
        });
    }

    // Build the output OTF/TTF
    let mut out = Vec::with_capacity(total_sfnt);

    // OT header: sfVersion(4) + numTables(2) + searchRange(2) + entrySelector(2) + rangeShift(2) = 12
    out.extend_from_slice(&flavor.to_be_bytes());
    out.extend_from_slice(&num_tables.to_be_bytes());
    let n = num_tables as u32;
    let entry_sel = (n as f64).log2().floor() as u16;
    let search_range = (1u16 << entry_sel) * 16;
    let range_shift = num_tables * 16 - search_range;
    out.extend_from_slice(&search_range.to_be_bytes());
    out.extend_from_slice(&entry_sel.to_be_bytes());
    out.extend_from_slice(&range_shift.to_be_bytes());

    // We need to write the table directory first (each entry = 16 bytes),
    // then the actual table data. Compute offsets.
    let dir_size = 12 + (num_tables as usize) * 16;
    let mut table_data: Vec<Vec<u8>> = Vec::new();
    let mut current_offset = dir_size;

    for entry in &entries {
        // Decompress or copy table data
        let raw = if entry.comp_length < entry.orig_length {
            // zlib compressed
            let compressed = &data[entry.offset..entry.offset + entry.comp_length];
            let mut decoder = ZlibDecoder::new(compressed);
            let mut decompressed = Vec::with_capacity(entry.orig_length);
            if std::io::Read::read_to_end(&mut decoder, &mut decompressed).is_err() {
                return None;
            }
            decompressed
        } else {
            // uncompressed
            if entry.offset + entry.orig_length > data.len() { return None; }
            data[entry.offset..entry.offset + entry.orig_length].to_vec()
        };

        // Write table directory entry: tag(4) + checksum(4) + offset(4) + length(4)
        out.extend_from_slice(&entry.tag);
        out.extend_from_slice(&entry.orig_checksum.to_be_bytes());
        out.extend_from_slice(&(current_offset as u32).to_be_bytes());
        out.extend_from_slice(&(raw.len() as u32).to_be_bytes());

        // Pad to 4-byte boundary
        let padded = (raw.len() + 3) & !3;
        let mut padded_raw = raw;
        padded_raw.resize(padded, 0);
        current_offset += padded;
        table_data.push(padded_raw);
    }

    // Append all table data
    for td in table_data {
        out.extend_from_slice(&td);
    }

    Some(out)
}

/// Minimal Base64 decoder (no external dependency).
/// Returns `Err` on invalid input.
fn decode_base64(s: &str) -> Result<Vec<u8>, ()> {
    const TABLE: &[u8; 128] = b"\
        \xff\xff\xff\xff\xff\xff\xff\xff\xff\xff\xff\xff\xff\xff\xff\xff\
        \xff\xff\xff\xff\xff\xff\xff\xff\xff\xff\xff\xff\xff\xff\xff\xff\
        \xff\xff\xff\xff\xff\xff\xff\xff\xff\xff\xff\x3e\xff\xff\xff\x3f\
        \x34\x35\x36\x37\x38\x39\x3a\x3b\x3c\x3d\xff\xff\xff\xff\xff\xff\
        \xff\x00\x01\x02\x03\x04\x05\x06\x07\x08\x09\x0a\x0b\x0c\x0d\x0e\
        \x0f\x10\x11\x12\x13\x14\x15\x16\x17\x18\x19\xff\xff\xff\xff\xff\
        \xff\x1a\x1b\x1c\x1d\x1e\x1f\x20\x21\x22\x23\x24\x25\x26\x27\x28\
        \x29\x2a\x2b\x2c\x2d\x2e\x2f\x30\x31\x32\x33\xff\xff\xff\xff\xff";

    let s: Vec<u8> = s.bytes().filter(|&b| b != b'\n' && b != b'\r' && b != b' ').collect();
    let mut out = Vec::with_capacity(s.len() / 4 * 3);
    let mut i = 0;
    while i + 3 < s.len() {
        let a = s[i];
        let b = s[i + 1];
        let c = s[i + 2];
        let d = s[i + 3];
        if a >= 128 || b >= 128 || c >= 128 || d >= 128 { return Err(()); }
        let va = TABLE[a as usize];
        let vb = TABLE[b as usize];
        let vc = if c == b'=' { 0 } else { TABLE[c as usize] };
        let vd = if d == b'=' { 0 } else { TABLE[d as usize] };
        if va == 0xff || vb == 0xff || vc == 0xff || vd == 0xff { return Err(()); }
        out.push((va << 2) | (vb >> 4));
        if c != b'=' { out.push((vb << 4) | (vc >> 2)); }
        if d != b'=' { out.push((vc << 6) | vd); }
        i += 4;
    }
    Ok(out)
}

// ─── Float Context ────────────────────────────────────────────────────────────

#[derive(Debug, Default, Clone)]
pub struct FloatItem {
    pub rect:  Rect,
    pub side:  FloatSide,
    pub clear: f32,  // bottom of this float
}

#[derive(Debug, Clone, PartialEq)]
pub enum FloatSide { Left, Right }

impl Default for FloatSide {
    fn default() -> Self { Self::Left }
}

#[derive(Debug, Default, Clone)]
pub struct FloatContext {
    pub floats: Vec<FloatItem>,
    pub origin_y: f32, // Document Y of the context root
}

impl FloatContext {
    pub fn available_width(
        &self,
        y: f32, line_h: f32,
        containing_w: f32,
        out_left: &mut f32,
        out_right: &mut f32,
    ) {
        *out_left  = 0.0;
        *out_right = containing_w;
        for f in &self.floats {
            if f.rect.y < y + line_h && f.clear > y {
                if f.side == FloatSide::Left {
                    let r = f.rect.x + f.rect.w;
                    if r > *out_left { *out_left = r; }
                } else {
                    let l = f.rect.x;
                    if l < *out_right { *out_right = l; }
                }
            }
        }
    }

    pub fn clear_y(&self, current_y: f32, clear: Clear) -> f32 {
        let mut y = current_y;
        for f in &self.floats {
            match clear {
                Clear::Left  if f.side == FloatSide::Left  => { if f.clear > y { y = f.clear; } }
                Clear::Right if f.side == FloatSide::Right => { if f.clear > y { y = f.clear; } }
                Clear::Both                                 => { if f.clear > y { y = f.clear; } }
                _ => {}
            }
        }
        y
    }

    pub fn place_float(
        &mut self,
        current_y: f32,
        float_w: f32, float_h: f32,
        containing_w: f32,
        side: FloatSide,
    ) -> Rect {
        // Find the lowest Y position where the float fits horizontally.
        let mut y = current_y;
        loop {
            let mut left = 0.0f32;
            let mut right = containing_w;
            self.available_width(y, float_h, containing_w, &mut left, &mut right);
            let available = right - left;
            if available >= float_w { break; }
            // Move past the nearest float bottom
            let next_y = self.floats.iter()
                .filter(|f| f.clear > y)
                .map(|f| f.clear)
                .fold(f32::MAX, f32::min);
            if next_y == f32::MAX { break; }
            y = next_y;
        }

        let mut left = 0.0f32;
        let mut right = containing_w;
        self.available_width(y, float_h, containing_w, &mut left, &mut right);

        let x = if side == FloatSide::Left { left } else { right - float_w };
        let rect = Rect::new(x, y, float_w, float_h);
        self.floats.push(FloatItem { rect, side, clear: y + float_h });
        rect
    }
}

/// Walk the tree bottom-up: if any child is `layout_dirty`, mark the parent
/// dirty too.  Returns `true` if the node (or any descendant) is dirty.
/// Skips subtrees where `has_dirty_descendant` is false and the node itself
/// is not dirty, making this O(dirty_path) instead of O(all_nodes) after
/// incremental cascade.
fn propagate_dirty(node: &mut HtmlBox) -> bool {
    let mut child_dirty = false;
    node.cached_intrinsic_w.set(f32::NAN);
    for child in &mut node.children {
        if propagate_dirty(child) { child_dirty = true; }
    }
    if child_dirty { node.layout_dirty = true; }
    node.layout_dirty
}

// ─── Resolved box model ───────────────────────────────────────────────────────

#[derive(Debug, Default, Clone, Copy)]
pub struct ResolvedBox {
    pub margin_top:    f32,
    pub margin_right:  f32,
    pub margin_bottom: f32,
    pub margin_left:   f32,

    pub padding_top:    f32,
    pub padding_right:  f32,
    pub padding_bottom: f32,
    pub padding_left:   f32,

    pub border_top:    f32,
    pub border_right:  f32,
    pub border_bottom: f32,
    pub border_left:   f32,

    pub content_width:  Option<f32>,  // None = auto
    pub content_height: Option<f32>,
}

impl ResolvedBox {
    pub fn h_space(&self) -> f32 {
        self.margin_left + self.border_left + self.padding_left
            + self.padding_right + self.border_right + self.margin_right
    }
    pub fn v_space(&self) -> f32 {
        self.margin_top + self.border_top + self.padding_top
            + self.padding_bottom + self.border_bottom + self.margin_bottom
    }
    pub fn inner_h_space(&self) -> f32 {
        self.border_left + self.padding_left + self.padding_right + self.border_right
    }
    pub fn inner_v_space(&self) -> f32 {
        self.border_top + self.padding_top + self.padding_bottom + self.border_bottom
    }
}

pub fn resolve_box(style: &ComputedStyle, parent_font_px: f32,
                   containing_w: f32, root_font_px: f32) -> ResolvedBox {
    resolve_box_vp(style, parent_font_px, containing_w, root_font_px, 0.0, 0.0, None)
}

pub fn resolve_box_vp(style: &ComputedStyle, parent_font_px: f32,
                   containing_w: f32, root_font_px: f32,
                   viewport_w: f32, viewport_h: f32,
                   containing_h: Option<f32>) -> ResolvedBox {
    let res = |l: &CssLength| l.resolve_vp(parent_font_px, containing_w, root_font_px, viewport_w, viewport_h);
    let _font_px = style.font_size_px(parent_font_px, root_font_px);

    let pad_left   = res(&style.padding_left);
    let pad_right  = res(&style.padding_right);
    let pad_top    = res(&style.padding_top);
    let pad_bottom = res(&style.padding_bottom);

    let border_left   = if style.border_left_style   != BorderStyle::None { res(&style.border_left_width)   } else { 0.0 };
    let border_right  = if style.border_right_style  != BorderStyle::None { res(&style.border_right_width)  } else { 0.0 };
    let border_top    = if style.border_top_style    != BorderStyle::None { res(&style.border_top_width)    } else { 0.0 };
    let border_bottom = if style.border_bottom_style != BorderStyle::None { res(&style.border_bottom_width) } else { 0.0 };

    let content_width = if style.width.is_auto() {
        None
    } else {
        let mut w = res(&style.width).max(0.0);
        // box-sizing: border-box — subtract padding + border from declared width
        if style.box_sizing == BoxSizing::BorderBox {
            w = (w - pad_left - pad_right - border_left - border_right).max(0.0);
        }
        Some(w)
    };

    // CSS 2.1 §10.5: percentage heights resolve against the containing block's height.
    // If the containing block's height is not explicitly set (containing_h is None),
    // percentage heights are treated as auto.
    let content_height = if style.height.is_auto() {
        None
    } else if matches!(style.height, CssLength::Percent(_)) {
        match containing_h {
            Some(ch) => {
                let mut h = style.height.resolve_vp(parent_font_px, ch, root_font_px, viewport_w, viewport_h).max(0.0);
                if style.box_sizing == BoxSizing::BorderBox {
                    h = (h - pad_top - pad_bottom - border_top - border_bottom).max(0.0);
                }
                Some(h)
            }
            None => None, // percentage height with no explicit containing height → auto
        }
    } else {
        let mut h = res(&style.height).max(0.0);
        if style.box_sizing == BoxSizing::BorderBox {
            h = (h - pad_top - pad_bottom - border_top - border_bottom).max(0.0);
        }
        Some(h)
    };

    ResolvedBox {
        margin_top:    res(&style.margin_top),
        margin_right:  res(&style.margin_right),
        margin_bottom: res(&style.margin_bottom),
        margin_left:   res(&style.margin_left),

        padding_top:    pad_top,
        padding_right:  pad_right,
        padding_bottom: pad_bottom,
        padding_left:   pad_left,

        border_top,
        border_right,
        border_bottom,
        border_left,

        content_width,
        content_height,
    }
}

// ─── Layout Engine ────────────────────────────────────────────────────────────

pub struct LayoutEngine {
    pub root_font_px: f32,
    /// Logical viewport width (for vw units).
    pub viewport_w: f32,
    /// Logical viewport height (for vh units).
    pub viewport_h: f32,
    /// Reference to a font system for accurate measurement.
    pub font_system: Option<*mut cosmic_text::FontSystem>,
    /// Custom component registry for custom tags
    pub component_registry: ComponentRegistry,
    /// Device pixel ratio (e.g. 2.0 on HiDPI/Retina). Used so that char_x
    /// positions are shaped at physical pixel size — matching the renderer —
    /// giving accurate click↔caret mapping on every display density.
    pub scale: f32,
    /// Viewport width used in the last cascade pass, for skip-cascade optimization.
    last_cascade_vw: f32,
    /// Viewport height used in the last geometry pass, to detect vh-unit changes.
    last_geometry_viewport_h: f32,
    /// Whether any @media rules exist — cached to avoid O(n) scan every layout.
    cached_has_media_q: bool,
    /// Whether any @container rules exist — cached to avoid O(n) scan every layout.
    cached_has_container_q: bool,
    /// Whether @font-face font fetches have been kicked off (not necessarily finished).
    fonts_loaded: bool,
    /// Receiver for async font data arriving from background threads.
    pending_fonts: Option<std::sync::mpsc::Receiver<(String, Vec<u8>)>>,
    /// Number of font fetches still in flight.
    fonts_in_flight: std::sync::Arc<std::sync::atomic::AtomicUsize>,
    /// Containing block rect for the nearest positioned (non-static) ancestor.
    /// Used by abs-pos children to resolve their containing block correctly.
    pub pos_cb: Cell<Rect>,
    /// Current recursion depth — prevents stack overflow on deeply nested DOMs.
    layout_depth: Cell<usize>,
    /// Total layout_box calls — detect infinite loops.
    layout_calls: Cell<usize>,
    /// Layout start time — detect long-running layout.
    layout_start: Cell<Option<std::time::Instant>>,
}

/// Maximum layout recursion depth to prevent stack overflow.
const MAX_LAYOUT_DEPTH: usize = 400;

impl LayoutEngine {
    pub fn new() -> Self {
        Self {
            root_font_px: 16.0,
            viewport_w: 900.0,
            viewport_h: 700.0,
            font_system: None,
            component_registry: ComponentRegistry::default(),
            scale: 1.0,
            last_cascade_vw: f32::NAN,   // NAN forces cascade on first call
            last_geometry_viewport_h: f32::NAN, // NAN forces full layout on first call
            cached_has_media_q: false,
            cached_has_container_q: false,
            fonts_loaded: false,
            pending_fonts: None,
            fonts_in_flight: std::sync::Arc::new(std::sync::atomic::AtomicUsize::new(0)),
            pos_cb: Cell::new(Rect::new(0.0, 0.0, 0.0, 0.0)),
            layout_depth: Cell::new(0),
            layout_calls: Cell::new(0),
            layout_start: Cell::new(None),
        }
    }

    /// Resolve a box's styles using the engine's viewport dimensions.
    #[inline]
    pub fn res_box(&self, style: &ComputedStyle, font_px: f32, containing_w: f32, root_font_px: f32) -> ResolvedBox {
        resolve_box_vp(style, font_px, containing_w, root_font_px, self.viewport_w, self.viewport_h, None)
    }

    /// Resolve a single CSS length using the engine's viewport dimensions.
    #[inline]
    pub fn res_len(&self, len: &CssLength, font_px: f32, containing: f32, root_font_px: f32) -> f32 {
        len.resolve_vp(font_px, containing, root_font_px, self.viewport_w, self.viewport_h)
    }

    /// Compute max-content width of a node WITHOUT calling layout_box.
    /// This avoids the exponential blowup in nested flex layouts by measuring
    /// text directly with the font system instead of doing full inline layout.
    /// Compute the min-content width of a node (the smallest width it can take
    /// without overflowing).  For text, this is the width of the longest word.
    pub fn min_content_width(&self, node: &HtmlBox, parent_font_px: f32, root_font_px: f32) -> f32 {
        if matches!(node.style.display, Display::None) { return 0.0; }

        let font_px = node.style.font_size_px(parent_font_px, root_font_px);

        // Explicit width → use that directly
        if !node.style.width.is_auto() && !matches!(node.style.width, CssLength::Percent(_)) {
            let w = self.res_len(&node.style.width, font_px, 0.0, root_font_px);
            return w.max(0.0);
        }

        // Replaced elements (img): use natural dimensions
        if node.tag == "img" && node.image_width > 0 {
            return node.image_width as f32;
        }

        let rbox = self.res_box(&node.style, font_px, 0.0, root_font_px);
        let pad_border = rbox.padding_left + rbox.padding_right
                       + rbox.border_left + rbox.border_right;

        // Text node: measure the longest word
        if node.is_text_node() {
            let text = &node.text;
            if text.is_empty() { return 0.0; }
            let mut max_word = 0.0f32;
            for word in text.split(|c: char| c.is_ascii_whitespace()) {
                if word.is_empty() { continue; }
                let w = if let Some(fs_ptr) = self.font_system {
                    let fs = unsafe { &mut *fs_ptr };
                    crate::layout::inline_layout::measure_text_width_weighted(
                        word, font_px * self.scale,
                        Some(fs),
                        node.style.font_weight, node.style.font_style,
                        self.scale,
                        &node.style.font_family,
                    )
                } else {
                    crate::layout::inline_layout::measure_text_width_ts(word, font_px, 8)
                };
                if w > max_word { max_word = w; }
            }
            return max_word;
        }

        // For containers: max of children's min-content widths
        let mut max_w = 0.0f32;
        for ch in &node.children {
            if matches!(ch.style.display, Display::None) { continue; }
            if matches!(ch.style.position, Position::Absolute | Position::Fixed) { continue; }
            let child_font = ch.style.font_size_px(font_px, root_font_px);
            let child_rbox = self.res_box(&ch.style, child_font, 0.0, root_font_px);
            let child_outer = child_rbox.padding_left + child_rbox.padding_right
                + child_rbox.border_left + child_rbox.border_right
                + child_rbox.margin_left + child_rbox.margin_right;
            let cw = self.min_content_width(ch, font_px, root_font_px) + child_outer;
            if cw > max_w { max_w = cw; }
        }
        max_w + pad_border
    }

    pub fn max_content_width(&self, node: &HtmlBox, parent_font_px: f32, root_font_px: f32) -> f32 {
        if matches!(node.style.display, Display::None) { return 0.0; }

        // Explicit width → use that directly (but skip percentages — they can't
        // resolve without a known containing width during intrinsic measurement).
        let font_px = node.style.font_size_px(parent_font_px, root_font_px);
        if !node.style.width.is_auto() && !matches!(node.style.width, CssLength::Percent(_)) {
            let w = self.res_len(&node.style.width, font_px, 0.0, root_font_px);
            return w.max(0.0);
        }

        // Replaced elements (img): use natural dimensions.
        if node.tag == "img" && node.image_width > 0 {
            return node.image_width as f32;
        }

        let rbox = self.res_box(&node.style, font_px, 0.0, root_font_px);
        let pad_border = rbox.padding_left + rbox.padding_right
                       + rbox.border_left + rbox.border_right;

        // Text node: measure text width directly.
        if node.is_text_node() {
            let text = &node.text;
            if text.is_empty() { return 0.0; }
            let w = if let Some(fs_ptr) = self.font_system {
                let fs = unsafe { &mut *fs_ptr };
                crate::layout::inline_layout::measure_text_width_weighted(
                    text, font_px * self.scale,
                    Some(fs),
                    node.style.font_weight, node.style.font_style,
                    self.scale,
                    &node.style.font_family,
                )
            } else {
                crate::layout::inline_layout::measure_text_width_ts(text, font_px, 8)
            };
            return w;
        }

        let is_row_flex = matches!(node.style.display, Display::Flex | Display::InlineFlex)
            && matches!(node.style.flex_direction, FlexDirection::Row | FlexDirection::RowReverse);
        let is_col_flex = matches!(node.style.display, Display::Flex | Display::InlineFlex)
            && !is_row_flex;

        if is_row_flex {
            // Row flex: sum of children's max-content widths + their box model.
            let mut total = 0.0f32;
            let gap = self.res_len(&node.style.column_gap, font_px, 0.0, root_font_px);
            let mut count = 0usize;
            for ch in &node.children {
                if matches!(ch.style.display, Display::None) { continue; }
                if matches!(ch.style.position, Position::Absolute | Position::Fixed) { continue; }
                if ch.tag == "#text" && ch.text.chars().all(|c| c.is_ascii_whitespace()) { continue; }
                let child_font = ch.style.font_size_px(font_px, root_font_px);
                let child_rbox = self.res_box(&ch.style, child_font, 0.0, root_font_px);
                let child_outer = child_rbox.padding_left + child_rbox.padding_right
                    + child_rbox.border_left + child_rbox.border_right
                    + child_rbox.margin_left + child_rbox.margin_right;
                // Use flex-basis if explicit, otherwise max-content width.
                let child_main = if !ch.style.flex_basis.is_auto() {
                    self.res_len(&ch.style.flex_basis, child_font, 0.0, root_font_px).max(0.0)
                } else {
                    self.max_content_width(ch, font_px, root_font_px)
                };
                total += child_main + child_outer;
                if count > 0 { total += gap; }
                count += 1;
            }
            return total + pad_border;
        }

        // Column flex or block: max of children's max-content widths.
        let mut max_w = 0.0f32;
        for ch in &node.children {
            if matches!(ch.style.display, Display::None) { continue; }
            if matches!(ch.style.position, Position::Absolute | Position::Fixed) { continue; }
            let child_font = ch.style.font_size_px(font_px, root_font_px);
            let child_rbox = self.res_box(&ch.style, child_font, 0.0, root_font_px);
            let child_outer = child_rbox.padding_left + child_rbox.padding_right
                + child_rbox.border_left + child_rbox.border_right
                + child_rbox.margin_left + child_rbox.margin_right;
            let cw = self.max_content_width(ch, font_px, root_font_px) + child_outer;
            if cw > max_w { max_w = cw; }
        }
        max_w + pad_border
    }

    /// Kick off non-blocking font loading. Base64 and local fonts are loaded
    /// immediately; remote fonts are fetched in background threads and arrive
    /// via `pending_fonts` channel — polled each `layout()` call.
    pub fn load_font_faces(&mut self, faces: &[crate::css::FontFaceDecl], base_url: &str) {
        if let Some(fs_ptr) = self.font_system {
            let fs = unsafe { &mut *fs_ptr };

            // ── Phase 1: Resolve each @font-face to its best fetchable URL ──────
            let mut remote: Vec<(String, String)> = Vec::new();

            for face in faces {
                let mut found = false;
                for source in split_font_sources(&face.src) {
                    if found { break; }
                    let source = source.trim();
                    let url_inner = if let Some(start) = source.find("url(") {
                        let rest = &source[start + 4..];
                        let end = rest.find(')').unwrap_or(rest.len());
                        rest[..end].trim().trim_matches('"').trim_matches('\'')
                    } else {
                        continue;
                    };

                    // Strip fragment (#iefix etc.)
                    let url_clean = url_inner.split('#').next().unwrap_or(url_inner);
                    // Strip query string for extension check
                    let url_for_ext = url_clean.split('?').next().unwrap_or(url_clean);

                    // Skip font formats we can't handle (eot, svg, woff2)
                    if url_for_ext.ends_with(".eot") || url_for_ext.ends_with(".svg")
                       || url_for_ext.ends_with(".woff2") {
                        continue;
                    }

                    // Base64 data URI — load immediately (no network)
                    if let Some(b64) = url_inner.strip_prefix("data:")
                        .and_then(|s| s.find(";base64,").map(|i| &s[i + 8..]))
                    {
                        if let Ok(bytes) = decode_base64(b64.trim()) {
                            load_font_bytes(fs, bytes);
                            found = true;
                        }
                        continue;
                    }

                    let resolved = crate::html::resolve_url(url_clean, base_url);

                    if resolved.starts_with("http://") || resolved.starts_with("https://") {
                        remote.push((face.family.clone(), resolved));
                        found = true;
                    } else if !resolved.is_empty() {
                        // Local file — load immediately
                        if let Ok(data) = std::fs::read(&resolved) {
                            load_font_bytes(fs, data);
                            found = true;
                        }
                    }
                }
            }

            // ── Phase 2: Fire-and-forget remote font fetches ────────────────────
            if !remote.is_empty() {
                let (tx, rx) = std::sync::mpsc::channel::<(String, Vec<u8>)>();
                let in_flight = self.fonts_in_flight.clone();
                in_flight.store(remote.len(), std::sync::atomic::Ordering::SeqCst);

                for (family, url) in remote {
                    let sender = tx.clone();
                    let counter = in_flight.clone();
                    std::thread::spawn(move || {
                        let result = crate::http_client()
                            .get(&url)
                            .send().ok()
                            .and_then(|r| r.bytes().ok())
                            .map(|b| b.to_vec())
                            .filter(|b| !b.is_empty());
                        if let Some(bytes) = result {
                            eprintln!("  Font loaded: {} ({} bytes) from {}", family, bytes.len(), &url[..url.len().min(80)]);
                            let _ = sender.send((family, bytes));
                        } else {
                            eprintln!("  Font fetch failed: {}", family);
                        }
                        counter.fetch_sub(1, std::sync::atomic::Ordering::SeqCst);
                    });
                }
                self.pending_fonts = Some(rx);
            }
        }
    }

    /// Poll for fonts that have arrived from background threads.
    /// Returns `true` if any new fonts were loaded (caller should re-layout).
    pub fn poll_pending_fonts(&mut self) -> bool {
        let rx = match self.pending_fonts.as_ref() {
            Some(rx) => rx,
            None => return false,
        };
        let fs = match self.font_system {
            Some(ptr) => unsafe { &mut *ptr },
            None => return false,
        };

        let mut loaded_any = false;
        // Drain all available font data without blocking.
        while let Ok((_, bytes)) = rx.try_recv() {
            load_font_bytes(fs, bytes);
            loaded_any = true;
        }

        // If all fetches are done, drop the receiver.
        if self.fonts_in_flight.load(std::sync::atomic::Ordering::SeqCst) == 0 {
            self.pending_fonts = None;
        }

        loaded_any
    }

    /// Returns `true` if there are still font fetches in flight.
    pub fn has_pending_fonts(&self) -> bool {
        self.pending_fonts.is_some()
    }

    /// Main entry point: layout the full document.
    pub fn layout(&mut self, doc: &mut Document, viewport_width: f32) {
        self.viewport_w = viewport_width;
        // Keep viewport in doc so focus-change recascades use the correct size.
        doc.viewport_w = self.viewport_w;
        doc.viewport_h = self.viewport_h;
        let root_font_px = self.root_font_px;

        // Rebuild selector index if rules changed (lazy, skips if already up-to-date).
        doc.stylesheet.rebuild_index();

        // Load @font-face fonts (non-blocking — remote fonts arrive via poll_pending_fonts).
        if !self.fonts_loaded && !doc.stylesheet.font_faces.is_empty() {
            self.load_font_faces(&doc.stylesheet.font_faces, &doc.base_url);
            self.fonts_loaded = true;
        }

        // Cache @media / @container presence so we don't O(n)-scan rules every layout.
        self.cached_has_media_q     = doc.stylesheet.rules.iter().any(|r| !r.media_condition.is_empty());
        self.cached_has_container_q = doc.stylesheet.rules.iter().any(|r| !r.container_condition.is_empty());

        // Skip the CSS cascade on resize when nothing media-query-relevant changed.
        let needs_cascade = self.last_cascade_vw.is_nan()
            || (self.cached_has_media_q
                && doc.stylesheet.rules.iter().any(|r| {
                    !r.media_condition.is_empty()
                        && crate::css::evaluate_media(&r.media_condition, self.last_cascade_vw, self.viewport_h)
                            != crate::css::evaluate_media(&r.media_condition, viewport_width, self.viewport_h)
                }));

        let hover_changed = doc.hover_changed;
        doc.hover_changed = false;
        let dom_style_dirty = doc.style_dirty;
        doc.style_dirty = false;

        let did_cascade = if needs_cascade || dom_style_dirty {
            // Full cascade needed (initial, viewport change, etc.)
            let hover_chain = crate::css::build_hover_chain(&doc.root, doc.hovered_box);
            crate::css::apply_cascade_vp_hover(
                &mut doc.root, &doc.stylesheet, None, root_font_px,
                self.viewport_w, self.viewport_h, doc.focused_box, doc.keyboard_focus,
                &hover_chain,
            );
            // Clear any leftover dirty flags after full cascade
            crate::css::clear_cascade_dirty(&mut doc.root);
            self.last_cascade_vw = viewport_width;
            doc.prev_hovered_box = doc.hovered_box;
            true
        } else if hover_changed {
            // Incremental hover cascade — only re-cascade elements affected by
            // the hover change (old chain + new chain), skip everything else.
            let old_chain = crate::css::build_hover_chain(&doc.root, doc.prev_hovered_box);
            let new_chain = crate::css::build_hover_chain(&doc.root, doc.hovered_box);
            // Mark dirty flags on nodes affected by hover change
            crate::css::mark_hover_dirty(&old_chain, &new_chain, &doc.node_map, false);

            crate::css::apply_cascade_incremental(
                &mut doc.root, &doc.stylesheet, None, root_font_px,
                self.viewport_w, self.viewport_h, doc.focused_box, doc.keyboard_focus,
                &new_chain,
            );
            crate::css::clear_cascade_dirty(&mut doc.root);
            doc.hover_suppress_count = 1;
            doc.prev_hovered_box = doc.hovered_box;
            true
        } else {
            false
        };

        // ── CSS animation / transition runtime ─────────────────────────────
        let now = std::time::Instant::now();
        doc.sync_animations(now);
        if did_cascade || hover_changed { doc.sync_transitions(now, did_cascade); }
        doc.tick_animations(now);
        if !doc.animation_overrides.is_empty() {
            let overrides = doc.animation_overrides.clone();
            crate::css::apply_animation_overrides(&mut doc.root, &overrides);
        }
        // ──────────────────────────────────────────────────────────────────

        self.layout_geometry(doc, viewport_width, root_font_px);
        self.last_geometry_viewport_h = self.viewport_h;

        // Container query post-pass: now that box sizes are known, apply @container rules
        // whose conditions match the computed dimensions of container ancestors, then
        // re-layout so the updated styles take effect.
        if self.cached_has_container_q {
            let changed = crate::css::apply_container_cascade_tree(
                &mut doc.root, &doc.stylesheet, &[], &[], 0, 1, 0, 1,
                root_font_px, self.viewport_w, self.viewport_h,
                doc.focused_box, doc.keyboard_focus,
            );
            if changed {
                // Re-apply animation overrides after container-query cascade.
                if !doc.animation_overrides.is_empty() {
                    let overrides = doc.animation_overrides.clone();
                    crate::css::apply_animation_overrides(&mut doc.root, &overrides);
                }
                self.layout_geometry(doc, viewport_width, root_font_px);
                self.last_geometry_viewport_h = self.viewport_h;
            }
        }

        // Detect aria-live region changes and queue announcements.
        doc.check_live_regions();
    }

    /// Force the next `layout()` call to re-run the full CSS cascade.
    ///
    /// Call this after DOM mutations (adding/removing elements, changing classes or
    /// inline styles) so the skip-cascade optimisation does not hide the change.
    pub fn invalidate_cascade(&mut self) {
        self.last_cascade_vw = f32::NAN;
    }

    /// Layout without re-running the CSS cascade.
    ///
    /// Use this when only text content changed (e.g. keystrokes in an editable
    /// element).  Skipping the cascade saves CSS selector matching across every
    /// element in the tree; the line-cache early-stop then skips unchanged lines.
    pub fn layout_no_cascade(&mut self, doc: &mut Document, viewport_width: f32) {
        self.viewport_w = viewport_width;
        let root_font_px = self.root_font_px;
        self.layout_geometry(doc, viewport_width, root_font_px);
        self.last_geometry_viewport_h = self.viewport_h;
    }

    fn layout_geometry(&self, doc: &mut Document, viewport_width: f32, root_font_px: f32) {
        self.layout_calls.set(0);
        self.layout_start.set(Some(std::time::Instant::now()));
        // Propagate layout_dirty upward: if any descendant is dirty, mark
        // ancestors dirty so the subtree-pruning check doesn't skip them.
        propagate_dirty(&mut doc.root);

        // Set up root geometry
        let rbox = self.res_box(&doc.root.style, root_font_px, viewport_width, root_font_px);
        let content_w = rbox.content_width.unwrap_or(viewport_width);
        doc.root.content_rect = Rect::new(0.0, 0.0, content_w, 0.0);
        doc.root.padding_rect = Rect::new(0.0, 0.0, content_w, 0.0);
        doc.root.border_rect  = Rect::new(0.0, 0.0, content_w, 0.0);
        doc.root.margin_rect  = Rect::new(0.0, 0.0, content_w, 0.0);

        // Resolve shadow DOM slots before layout (only if any shadow roots exist)
        if has_shadow_roots(&doc.root) {
            resolve_all_slots(&mut doc.root);
        }

        self.pos_cb.set(Rect::new(0.0, 0.0, content_w, self.viewport_h));
        self.layout_box(&mut doc.root, content_w, 0.0, 0.0, root_font_px, root_font_px);

        // Update root geometry with final height
        let h = doc.root.margin_rect.h;
        doc.root.content_rect.h = h;
        doc.root.padding_rect.h = h;
        doc.root.border_rect.h  = h;

        // Clear descendant dirty flags now that layout is complete
        crate::css::clear_descendant_dirty(&mut doc.root);
    }

    pub fn layout_box(
        &self,
        node:       &mut HtmlBox,
        containing_w: f32,
        x:    f32,
        y:    f32,
        parent_font_px: f32,
        root_font_px:   f32,
    ) -> f32 {
        self.layout_box_with_fc(node, containing_w, x, y, parent_font_px, root_font_px, None)
    }

    pub fn layout_box_with_fc(
        &self,
        node:       &mut HtmlBox,
        containing_w: f32,
        x:    f32,
        y:    f32,
        parent_font_px: f32,
        root_font_px:   f32,
        fc:  Option<&mut FloatContext>,
    ) -> f32 {
        // Guard against infinite layout loops.
        let calls = self.layout_calls.get();
        self.layout_calls.set(calls + 1);
        if calls > 5_000_000 {
            eprintln!("  [layout] ABORTING: >5M layout calls — infinite loop detected");
            node.content_rect = Rect::new(x, y, containing_w, 0.0);
            node.padding_rect = node.content_rect;
            node.border_rect  = node.content_rect;
            node.margin_rect  = node.content_rect;
            return 0.0;
        }
        // Guard against stack overflow on deeply nested DOMs.
        let depth = self.layout_depth.get();
        if depth >= MAX_LAYOUT_DEPTH {
            node.content_rect = Rect::new(x, y, containing_w, 0.0);
            node.padding_rect = node.content_rect;
            node.border_rect  = node.content_rect;
            node.margin_rect  = node.content_rect;
            return 0.0;
        }
        // Don't layout display:none
        if matches!(node.style.display, Display::None) {
            node.content_rect = Rect::default();
            node.padding_rect = Rect::default();
            node.border_rect  = Rect::default();
            node.margin_rect  = Rect::default();
            return 0.0;
        }

        // display:contents — the element itself generates no box.
        // Its children are promoted to the parent's formatting context.
        if matches!(node.style.display, Display::Contents) {
            node.content_rect = Rect::default();
            node.padding_rect = Rect::default();
            node.border_rect  = Rect::default();
            node.margin_rect  = Rect::default();
            return 0.0;
        }

        self.layout_depth.set(depth + 1);

        let font_px = node.style.font_size_px(parent_font_px, root_font_px);

        // <img>/<svg> aspect ratio: when one dimension is auto and the natural
        // dimensions are known, compute the auto dimension to preserve the
        // image's intrinsic aspect ratio (CSS Images §5.1).
        // For <svg>, intrinsic dimensions come from viewBox.
        let (has_intrinsic, iw, ih) = if node.tag == "img" && node.image_width > 0 && node.image_height > 0 {
            (true, node.image_width as f32, node.image_height as f32)
        } else if node.tag == "svg" && node.svg_viewbox_w > 0.0 && node.svg_viewbox_h > 0.0 {
            (true, node.svg_viewbox_w, node.svg_viewbox_h)
        } else {
            (false, 0.0, 0.0)
        };
        if has_intrinsic {
            if node.style.width.is_auto() && !node.style.height.is_auto() {
                let h = node.style.height.resolve_vp(font_px, 0.0, root_font_px, self.viewport_w, self.viewport_h);
                let w = (h * iw / ih).round();
                node.style.width = CssLength::Px(w);
            } else if node.style.height.is_auto() && !node.style.width.is_auto() {
                let w = node.style.width.resolve_vp(font_px, containing_w, root_font_px, self.viewport_w, self.viewport_h);
                let h = (w * ih / iw).round();
                node.style.height = CssLength::Px(h);
            } else if node.style.width.is_auto() && node.style.height.is_auto() {
                // Start with natural dimensions
                let mut w = iw;
                let mut h = ih;
                // Apply max-width/max-height constraints, maintaining aspect ratio
                let max_w = node.style.max_width.resolve_vp(font_px, containing_w, root_font_px, self.viewport_w, self.viewport_h);
                if max_w > 0.0 && w > max_w {
                    h = (max_w * ih / iw).round();
                    w = max_w;
                }
                let max_h = node.style.max_height.resolve_vp(font_px, 0.0, root_font_px, self.viewport_w, self.viewport_h);
                if max_h > 0.0 && h > max_h {
                    w = (max_h * iw / ih).round();
                    h = max_h;
                }
                node.style.width  = CssLength::Px(w);
                node.style.height = CssLength::Px(h);
            }
        }

        let mut rbox = resolve_box_vp(&node.style, font_px, containing_w, root_font_px, self.viewport_w, self.viewport_h, None);

        // CSS 2.1 §10.5: when this element has a definite content height,
        // children with percentage heights can resolve against it.
        // Pre-resolve them here so layout_box (which passes containing_h=None
        // to resolve_box_vp) gets the right values.
        if let Some(parent_h) = rbox.content_height {
            if parent_h > 0.0 {
                for child in &mut node.children {
                    if matches!(child.style.height, CssLength::Percent(_)) {
                        let h = child.style.height.resolve_vp(font_px, parent_h, root_font_px, self.viewport_w, self.viewport_h);
                        child.style.height = CssLength::Px(h.max(0.0));
                    }
                }
            }
        }

        // Auto-margin centering (CSS 2.1 §10.3.3) — applies to any element with an
        // explicit width and at least one auto horizontal margin.  Block layout has
        // its own copy of this logic; here we handle flex/grid/table/custom.
        if let Some(content_w) = rbox.content_width {
            let left_auto  = node.style.margin_left.is_auto();
            let right_auto = node.style.margin_right.is_auto();
            if left_auto || right_auto {
                let non_margin = rbox.border_left + rbox.padding_left + content_w
                               + rbox.padding_right + rbox.border_right;
                let available  = (containing_w - non_margin).max(0.0);
                if left_auto && right_auto {
                    let ml = (available / 2.0).floor();
                    rbox.margin_left  = ml;
                    rbox.margin_right = available - ml;
                } else if left_auto {
                    rbox.margin_left  = available - rbox.margin_right;
                } else {
                    rbox.margin_right = available - rbox.margin_left;
                }
            }
        }

        // ── Layout subtree pruning ────────────────────────────────────────────
        // If this box's resolved content width is identical to the previous
        // layout AND nothing is dirty AND there is no incoming float context
        // (which could alter line widths), the entire subtree produces exactly
        // the same geometry as before.  We just shift the cached rects to the
        // new position without re-running any layout algorithm.
        //
        // This is the dominant win on resize for fixed-width components nested
        // inside a fluid viewport (grid cards, sidebar items, etc.) — their
        // content width never changes even when the viewport grows or shrinks.
        // Also disable pruning when the viewport height changed so that vh-units
        // (e.g. height: 100vh) and flex-stretch heights dependent on the viewport
        // are recalculated rather than returning stale cached geometry.
        let viewport_h_unchanged = self.viewport_h == self.last_geometry_viewport_h;
        if fc.is_none() && !node.layout_dirty && node.resolved_content_width > 0.0
            && viewport_h_unchanged
        {
            let new_content_w = if let Some(cw) = rbox.content_width {
                cw
            } else {
                let outer = rbox.margin_left + rbox.border_left  + rbox.padding_left
                          + rbox.border_right + rbox.padding_right + rbox.margin_right;
                (containing_w - outer).max(0.0)
            };
            // Also check the explicit content height hasn't changed.
            // This catches flex-stretch re-layouts where the parent mutates
            // child.style.height before calling layout_box a second time.
            let height_ok = match rbox.content_height {
                None    => true,  // auto height is determined by children — safe
                Some(h) => (h - node.content_rect.h).abs() < 0.5,
            };
            if (new_content_w - node.resolved_content_width).abs() < 0.5 && height_ok {
                // Content size is unchanged — just move the subtree.
                let dx = (x + rbox.margin_left) - node.border_rect.x;
                let dy = (y + rbox.margin_top)  - node.border_rect.y;
                if dx.abs() > 0.01 || dy.abs() > 0.01 {
                    shift_rects(node, dx, dy);
                }
                node.layout_dirty = false;
                return node.margin_rect.h;
            }
        }

        // Check for custom component measurement
        if let Some(callbacks) = self.component_registry.map.get(&node.tag) {
            let (cw, ch) = (callbacks.measure)(node, containing_w);
            let final_w = if node.style.width.is_auto() { cw } else { rbox.content_width.unwrap_or(cw) };
            let final_h = if node.style.height.is_auto() { ch } else { rbox.content_height.unwrap_or(ch) };
            block::build_box_rects(node, &rbox, x + rbox.margin_left + rbox.border_left + rbox.padding_left,
                                   y + rbox.margin_top + rbox.border_top + rbox.padding_top,
                                   final_w, final_h, rbox.margin_left, rbox.margin_right);
            node.layout_dirty = false;
            return node.margin_rect.h;
        }

        // Track the nearest positioned ancestor's padding rect for abs children.
        let old_pos_cb = self.pos_cb.get();
        if !matches!(node.style.position, Position::Static) {
            let est_padding_x = x + rbox.margin_left + rbox.border_left;
            let est_padding_y = y + rbox.margin_top  + rbox.border_top;
            let est_content_w = rbox.content_width.unwrap_or((containing_w - rbox.h_space()).max(0.0));
            let est_padding_w = est_content_w + rbox.padding_left + rbox.padding_right;
            self.pos_cb.set(Rect::new(est_padding_x, est_padding_y, est_padding_w, self.viewport_h));
        }

        // Shadow DOM: swap shadow children into node.children for layout so all
        // existing layout code (inline, block, flex, grid) works unchanged.
        let has_shadow = node.shadow_root.is_some();
        let saved_light_children = if has_shadow {
            let sr = node.shadow_root.as_mut().unwrap();
            let shadow_children = std::mem::take(&mut sr.children);
            let light = std::mem::replace(&mut node.children, shadow_children);
            Some(light)
        } else {
            None
        };

        // Replaced elements (input, select, textarea, img) cannot be flex/grid
        // containers per CSS spec — blockify so parent flex/grid also sees correct display.
        if matches!(node.tag.as_str(), "input" | "select" | "textarea" | "img" | "video" | "canvas" | "iframe") {
            match node.style.display {
                Display::Flex | Display::Grid => { node.style.display = Display::Block; }
                Display::InlineFlex | Display::InlineGrid => { node.style.display = Display::InlineBlock; }
                _ => {}
            }
        }

        let h = match node.style.display {
            Display::Flex | Display::InlineFlex => {
                flex::layout_flex(self, node, &rbox, containing_w, x, y, font_px, root_font_px)
            }
            Display::Grid | Display::InlineGrid => {
                grid::layout_grid(self, node, &rbox, containing_w, x, y, font_px, root_font_px)
            }
            Display::Table => {
                table::layout_table(self, node, &rbox, containing_w, x, y, font_px, root_font_px)
            }
            _ => {
                // Determine if children are block-level or inline-level
                if has_block_children(node) {
                    block::layout_block_with_fc(self, node, &rbox, containing_w, x, y, font_px, root_font_px, fc)
                } else {
                    // Pass parent float context so inline content wraps around
                    // floats from ancestor block containers (CSS §9.5).
                    inline_layout::layout_inline_block(self, node, &rbox, containing_w, x, y, font_px, root_font_px, fc)
                }
            }
        };

        // Restore shadow/light children after layout
        if let Some(light) = saved_light_children {
            let shadow_children = std::mem::replace(&mut node.children, light);
            if let Some(ref mut sr) = node.shadow_root {
                sr.children = shadow_children;
            }
        }

        self.pos_cb.set(old_pos_cb);
        self.layout_depth.set(depth);
        node.layout_dirty = false;
        h
    }

    /// Layout a box in inline context — returns (width, height, baseline).
    pub fn layout_inline(
        &self,
        node: &mut HtmlBox,
        max_w: f32,
        x: f32, y: f32,
        parent_font_px: f32,
        root_font_px:   f32,
    ) -> (f32, f32, f32) {
        let font_px = node.style.font_size_px(parent_font_px, root_font_px);
        let _rbox = self.res_box(&node.style, font_px, max_w, root_font_px);

        let h = self.layout_box(node, max_w, x, y, parent_font_px, root_font_px);
        let w = node.border_rect.w;
        let baseline = node.baseline;
        (w, h, baseline)
    }
}

// ─── Helper: does a box have any block-level children? ────────────────────────

/// Quick check if any node in the tree has a shadow root.
fn has_shadow_roots(node: &HtmlBox) -> bool {
    if node.shadow_root.is_some() { return true; }
    node.children.iter().any(|c| has_shadow_roots(c))
}

/// Walk the tree and resolve `<slot>` elements in all shadow roots.
fn resolve_all_slots(node: &mut HtmlBox) {
    node.resolve_slots();
    for child in &mut node.children {
        resolve_all_slots(child);
    }
    if let Some(ref mut sr) = node.shadow_root {
        for child in &mut sr.children {
            resolve_all_slots(child);
        }
    }
}

pub fn has_block_children(node: &HtmlBox) -> bool {
    node.effective_children().iter().any(|c| {
        if matches!(c.style.display, Display::None) { return false; }
        if matches!(c.style.display, Display::Contents) {
            return has_block_children(c);
        }
        matches!(c.style.position, Position::Static | Position::Relative | Position::Sticky) &&
        c.style.is_block_level() &&
        matches!(c.style.float, Float::None)
    })
}

// ─── Absolute / fixed positioning pass ───────────────────────────────────────

pub fn layout_positioned(engine: &LayoutEngine, node: &mut HtmlBox,
                         containing_rect: Rect, parent_font_px: f32, root_font_px: f32) {
    layout_positioned_static(engine, node, containing_rect, parent_font_px, root_font_px, None);
}

/// Layout an absolutely/fixed positioned element, with optional static position.
/// `static_y` is the y offset (relative to containing block) where the element would
/// appear in normal flow — used when `top` and `bottom` are both `auto`.
pub fn layout_positioned_static(engine: &LayoutEngine, node: &mut HtmlBox,
                         containing_rect: Rect, parent_font_px: f32, root_font_px: f32,
                         static_y: Option<f32>) {
    let font_px = node.style.font_size_px(parent_font_px, root_font_px);
    // By default the containing block is the passed containing_rect. For `fixed`
    // positioned elements the containing block is the viewport (0,0, viewport_w, viewport_h).
    let mut containing_w = containing_rect.w;
    let mut containing_h = containing_rect.h;
    let mut containing_x = containing_rect.x;
    let mut containing_y = containing_rect.y;
    if node.style.position == Position::Fixed {
        containing_w = engine.viewport_w;
        containing_h = engine.viewport_h;
        containing_x = 0.0;
        containing_y = 0.0;
    }

    let left_auto  = node.style.left.is_auto();
    let right_auto = node.style.right.is_auto();
    let top_auto   = node.style.top.is_auto();
    let bot_auto   = node.style.bottom.is_auto();

    // If both horizontal sides are set AND width is auto, compute width from stretch.
    // If width is explicit (or the element has intrinsic size), don't stretch —
    // auto margins will center it instead (CSS 2.1 §10.3.7).
    let constrained_w = if !left_auto && !right_auto && node.style.width.is_auto()
        && !(node.tag == "img" && node.image_width > 0)
    {
        let l = node.style.left.resolve_vp(font_px, containing_w, root_font_px, engine.viewport_w, engine.viewport_h);
        let r = node.style.right.resolve_vp(font_px, containing_w, root_font_px, engine.viewport_w, engine.viewport_h);
        let rbox_inner = resolve_box_vp(&node.style, font_px, containing_w, root_font_px, engine.viewport_w, engine.viewport_h, Some(containing_h));
        let w = (containing_w - l - r - rbox_inner.inner_h_space()).max(0.0);
        Some(w)
    } else {
        None
    };

    let constrained_h = if !top_auto && !bot_auto {
        let t = node.style.top.resolve_vp(font_px, containing_h, root_font_px, engine.viewport_w, engine.viewport_h);
        let b = node.style.bottom.resolve_vp(font_px, containing_h, root_font_px, engine.viewport_w, engine.viewport_h);
        let rbox_inner = resolve_box_vp(&node.style, font_px, containing_w, root_font_px, engine.viewport_w, engine.viewport_h, Some(containing_h));
        let h = (containing_h - t - b - rbox_inner.inner_v_space()).max(0.0);
        Some(h)
    } else {
        None
    };

    // Resolve percentage height against the containing block height.
    // layout_box normally passes containing_h = None, which makes percentage
    // heights auto.  For abs/fixed elements the containing block height IS
    // known (it's the positioned ancestor's padding-box height).
    if matches!(node.style.height, CssLength::Percent(_)) && containing_h > 0.0 {
        let h = node.style.height.resolve_vp(font_px, containing_h, root_font_px, engine.viewport_w, engine.viewport_h);
        node.style.height = CssLength::Px(h.max(0.0));
    }
    if matches!(node.style.min_height, CssLength::Percent(_)) && containing_h > 0.0 {
        let h = node.style.min_height.resolve_vp(font_px, containing_h, root_font_px, engine.viewport_w, engine.viewport_h);
        node.style.min_height = CssLength::Px(h.max(0.0));
    }
    if matches!(node.style.max_height, CssLength::Percent(_)) && containing_h > 0.0 {
        let h = node.style.max_height.resolve_vp(font_px, containing_h, root_font_px, engine.viewport_w, engine.viewport_h);
        node.style.max_height = CssLength::Px(h.max(0.0));
    }

    // Layout to get natural size (or constrained size)
    let layout_w = constrained_w.unwrap_or(containing_w);
    engine.layout_box(node, layout_w, 0.0, 0.0, font_px, root_font_px);

    // Shrink-to-fit: width:auto absolutely-positioned elements wrap their content
    // (CSS 2.1 §10.3.7), just like floats — but only when width is not already
    // constrained by having both left and right set.
    if constrained_w.is_none() && node.style.width.is_auto() {
        let intrinsic_w = block::compute_intrinsic_width(node);
        if intrinsic_w > 0.0 && intrinsic_w < layout_w {
            let shrink_w = intrinsic_w
                + node.resolved_pad_left  + node.resolved_pad_right
                + node.resolved_border_left + node.resolved_border_right
                + node.resolved_margin_left + node.resolved_margin_right;
            engine.layout_box(node, shrink_w, 0.0, 0.0, font_px, root_font_px);
        }
    }

    // Now resolve position offsets
    let rbox = resolve_box_vp(&node.style, font_px, containing_w, root_font_px, engine.viewport_w, engine.viewport_h, Some(containing_h));
    let res_l = node.style.left.resolve_vp(font_px, containing_w, root_font_px, engine.viewport_w, engine.viewport_h);
    let res_r = node.style.right.resolve_vp(font_px, containing_w, root_font_px, engine.viewport_w, engine.viewport_h);
    let res_t = node.style.top.resolve_vp(font_px, containing_h, root_font_px, engine.viewport_w, engine.viewport_h);
    let res_b = node.style.bottom.resolve_vp(font_px, containing_h, root_font_px, engine.viewport_w, engine.viewport_h);

    let x = if !left_auto && !right_auto
        && (node.style.margin_left.is_auto() || node.style.margin_right.is_auto())
        && !node.style.width.is_auto()
    {
        // Both left and right set with auto margins — center the element.
        // available = containing_w - left - right - border_box_w
        let avail = containing_w - res_l - res_r - node.border_rect.w;
        if node.style.margin_left.is_auto() && node.style.margin_right.is_auto() {
            containing_x + res_l + (avail / 2.0).max(0.0)
        } else if node.style.margin_left.is_auto() {
            containing_x + res_l + avail.max(0.0) - rbox.margin_right
        } else {
            containing_x + res_l + rbox.margin_left
        }
    } else if !left_auto {
        containing_x + res_l + rbox.margin_left
    } else if !right_auto {
        (containing_x + containing_w) - res_r - node.border_rect.w - rbox.margin_right
    } else {
        containing_x + rbox.margin_left
    };

    let y = if !top_auto {
        containing_y + res_t + rbox.margin_top
    } else if !bot_auto {
        (containing_y + containing_h) - res_b - node.border_rect.h - rbox.margin_bottom
    } else if let Some(abs_sy) = static_y {
        // Static position: absolute document-space y where the element would
        // appear in normal flow. Already accounts for parent offsets.
        abs_sy + rbox.margin_top
    } else {
        // Fallback: containing block content start.
        containing_y + rbox.margin_top
    };

    // Shift all rects to final position
    let dx = x - node.border_rect.x;
    let dy = y - node.border_rect.y;
    shift_rects(node, dx, dy);

    // If both sides set → we may need to re-layout with constrained size
    if let Some(cw) = constrained_w {
        if node.content_rect.w != cw {
            engine.layout_box(node, layout_w, x, y, font_px, root_font_px);
        }
    }

    // Apply constrained height when both top and bottom are set and height is auto.
    // Without this, inset:0 (top:0 bottom:0) leaves height at 0 because layout_box
    // has no content to fill the space.
    if let Some(ch) = constrained_h {
        if node.style.height.is_auto() && (node.content_rect.h - ch).abs() > 0.5 {
            let diff = ch - node.content_rect.h;
            node.content_rect.h += diff;
            node.padding_rect.h += diff;
            node.border_rect.h  += diff;
            node.margin_rect.h  += diff;
        }
    }
}

pub fn shift_rects(node: &mut HtmlBox, dx: f32, dy: f32) {
    node.content_rect.x += dx; node.content_rect.y += dy;
    node.padding_rect.x += dx; node.padding_rect.y += dy;
    node.border_rect.x  += dx; node.border_rect.y  += dy;
    node.margin_rect.x  += dx; node.margin_rect.y  += dy;
    for line in &mut node.line_cache {
        line.x += dx;
        line.y += dy;
    }
    for child in &mut node.children {
        // Fixed-position elements are placed relative to the viewport,
        // not their parent — don't shift them when a parent moves.
        if child.style.position == Position::Fixed { continue; }
        shift_rects(child, dx, dy);
    }
}

impl Default for LayoutEngine {
    fn default() -> Self { Self::new() }
}
