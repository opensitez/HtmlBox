//! Fragment tree — separates layout from DOM.
//!
//! The fragment tree is an owned tree (like HtmlBox) with the same field names
//! so that layout functions can operate on it with minimal changes.
//! Generated from the DOM before each layout pass, with structural corrections:
//! - Anonymous block boxes for mixed block+inline children
//! - display:contents elements removed (children reparented)
//! - ::before/::after as real child fragments
//! - Flex/grid children blockified
//! - Parent-first-child margin collapsing resolved (margin zeroed on fragment)
//!
//! Layout reads Fragment.style (never mutates it for structural reasons).
//! The DOM is never modified during layout.

use crate::types::*;
use std::collections::HashMap;

// ─── LayoutNode trait ─────────────────────────────────────────────────────────
//
// Both HtmlBox and Fragment implement this trait so layout functions
// can operate on either type. This enables the migration: layout_geometry
// generates a Fragment tree and lays it out without touching the DOM.

/// Trait for types that can be laid out by the layout engine.
/// Both HtmlBox and Fragment implement this.
pub trait LayoutNode {
    fn node_id(&self) -> u32;
    fn tag(&self) -> &str;
    fn text(&self) -> &str;
    fn style(&self) -> &ComputedStyle;
    fn style_mut(&mut self) -> &mut ComputedStyle;
    fn layout(&self) -> &LayoutBox;
    fn layout_mut(&mut self) -> &mut LayoutBox;
    fn children(&self) -> &[Self] where Self: Sized;
    fn children_mut(&mut self) -> &mut Vec<Self> where Self: Sized;
    fn attributes(&self) -> &HashMap<String, String>;
    fn image_width(&self) -> u32;
    fn image_height(&self) -> u32;
    fn svg_viewbox_w(&self) -> f32;
    fn svg_viewbox_h(&self) -> f32;
    fn component_width(&self) -> f32;
    fn component_height(&self) -> f32;
    fn set_component_size(&mut self, w: f32, h: f32);
    fn is_text_node(&self) -> bool;
    fn shadow_root(&self) -> Option<&ShadowRoot>;
    fn shadow_root_mut(&mut self) -> Option<&mut ShadowRoot>;
    fn has_shadow_root(&self) -> bool;
    fn cascade_dirty(&self) -> bool;
    fn has_dirty_descendant(&self) -> bool;
    fn has_dirty_layout_descendant(&self) -> bool;
}

impl LayoutNode for Fragment {
    fn node_id(&self) -> u32 { self.node_id }
    fn tag(&self) -> &str { &self.tag }
    fn text(&self) -> &str { &self.text }
    fn style(&self) -> &ComputedStyle { &self.style }
    fn style_mut(&mut self) -> &mut ComputedStyle { &mut self.style }
    fn layout(&self) -> &LayoutBox { &self.layout }
    fn layout_mut(&mut self) -> &mut LayoutBox { &mut self.layout }
    fn children(&self) -> &[Fragment] { &self.children }
    fn children_mut(&mut self) -> &mut Vec<Fragment> { &mut self.children }
    fn attributes(&self) -> &HashMap<String, String> { &self.attributes }
    fn image_width(&self) -> u32 { self.image_width }
    fn image_height(&self) -> u32 { self.image_height }
    fn svg_viewbox_w(&self) -> f32 { self.svg_viewbox_w }
    fn svg_viewbox_h(&self) -> f32 { self.svg_viewbox_h }
    fn component_width(&self) -> f32 { self.component_width }
    fn component_height(&self) -> f32 { self.component_height }
    fn set_component_size(&mut self, w: f32, h: f32) { self.component_width = w; self.component_height = h; }
    fn is_text_node(&self) -> bool { self.tag == "#text" }
    fn shadow_root(&self) -> Option<&ShadowRoot> { self.shadow_root.as_deref() }
    fn shadow_root_mut(&mut self) -> Option<&mut ShadowRoot> { self.shadow_root.as_deref_mut() }
    fn has_shadow_root(&self) -> bool { self.shadow_root.is_some() }
    fn cascade_dirty(&self) -> bool { self.cascade_dirty }
    fn has_dirty_descendant(&self) -> bool { self.has_dirty_descendant }
    fn has_dirty_layout_descendant(&self) -> bool { self.has_dirty_layout_descendant }
}

impl LayoutNode for HtmlBox {
    fn node_id(&self) -> u32 { self.node_id }
    fn tag(&self) -> &str { &self.tag }
    fn text(&self) -> &str { &self.text }
    fn style(&self) -> &ComputedStyle { &self.style }
    fn style_mut(&mut self) -> &mut ComputedStyle { &mut self.style }
    fn layout(&self) -> &LayoutBox { &self.layout }
    fn layout_mut(&mut self) -> &mut LayoutBox { &mut self.layout }
    fn children(&self) -> &[HtmlBox] { &self.children }
    fn children_mut(&mut self) -> &mut Vec<HtmlBox> { &mut self.children }
    fn attributes(&self) -> &HashMap<String, String> { &self.attributes }
    fn image_width(&self) -> u32 { self.image_width }
    fn image_height(&self) -> u32 { self.image_height }
    fn svg_viewbox_w(&self) -> f32 { self.svg_viewbox_w }
    fn svg_viewbox_h(&self) -> f32 { self.svg_viewbox_h }
    fn component_width(&self) -> f32 { self.component_width }
    fn component_height(&self) -> f32 { self.component_height }
    fn set_component_size(&mut self, w: f32, h: f32) { self.component_width = w; self.component_height = h; }
    fn is_text_node(&self) -> bool { self.tag == "#text" }
    fn shadow_root(&self) -> Option<&ShadowRoot> { self.shadow_root.as_deref() }
    fn shadow_root_mut(&mut self) -> Option<&mut ShadowRoot> { self.shadow_root.as_deref_mut() }
    fn has_shadow_root(&self) -> bool { self.shadow_root.is_some() }
    fn cascade_dirty(&self) -> bool { self.cascade_dirty }
    fn has_dirty_descendant(&self) -> bool { self.has_dirty_descendant }
    fn has_dirty_layout_descendant(&self) -> bool { self.has_dirty_layout_descendant }
}

/// A layout fragment — structurally mirrors HtmlBox so layout functions
/// can operate on it with minimal code changes.
#[derive(Clone, Debug)]
pub struct Fragment {
    /// Back-pointer to DOM node (0 for anonymous boxes).
    pub node_id: u32,
    /// Tag name.
    pub tag: String,
    /// Text content.
    pub text: String,
    /// Computed style snapshot.
    pub style: ComputedStyle,
    /// Layout output geometry.
    pub layout: LayoutBox,
    /// Children (owned, like HtmlBox).
    pub children: Vec<Fragment>,
    /// HTML attributes.
    pub attributes: HashMap<String, String>,
    /// Component cached dimensions.
    pub component_width: f32,
    pub component_height: f32,
    /// Image dimensions.
    pub image_width: u32,
    pub image_height: u32,
    /// Image pixel data (reference, not cloned — None during fragment layout).
    pub image_data: Option<Vec<u8>>,
    /// SVG viewbox dimensions.
    pub svg_viewbox_w: f32,
    pub svg_viewbox_h: f32,
    /// Shadow root (carried over from DOM for shadow DOM support).
    pub shadow_root: Option<Box<ShadowRoot>>,
    /// Hover state flags (carried from DOM).
    pub hover_applied: bool,
    pub cascade_dirty: bool,
    pub has_dirty_descendant: bool,
    pub has_dirty_layout_descendant: bool,
}

impl Fragment {
    /// Create a minimal fragment (like HtmlBox::new).
    pub fn new(tag: &str) -> Self {
        Self {
            node_id: 0,
            tag: tag.to_string(),
            text: String::new(),
            style: ComputedStyle::default(),
            layout: LayoutBox::default(),
            children: Vec::new(),
            attributes: HashMap::new(),
            component_width: 0.0,
            component_height: 0.0,
            image_width: 0,
            image_height: 0,
            image_data: None,
            svg_viewbox_w: 0.0,
            svg_viewbox_h: 0.0,
            shadow_root: None,
            hover_applied: false,
            cascade_dirty: false,
            has_dirty_descendant: false,
            has_dirty_layout_descendant: false,
        }
    }

    /// Is this a text node?
    pub fn is_text_node(&self) -> bool {
        self.tag == "#text"
    }

    /// Get effective children (handles shadow DOM).
    /// For fragments, shadow children are already resolved during generation.
    pub fn effective_children(&self) -> &[Fragment] {
        &self.children
    }
}

// ─── Generate fragment tree from DOM ──────────────────────────────────────────

/// Generate a fragment tree from a DOM tree.
/// The fragment tree has the correct structure for layout:
/// - Anonymous blocks for mixed block+inline
/// - display:contents removed
/// - Flex/grid children blockified
/// - Margin collapsing resolved
pub fn generate_fragments(root: &HtmlBox) -> Fragment {
    generate_node(root, false)
}

fn generate_node(node: &HtmlBox, parent_is_flex_grid: bool) -> Fragment {
    let mut frag = Fragment {
        node_id: node.node_id,
        tag: node.tag.clone(),
        text: node.text.clone(),
        style: node.style.clone(),
        layout: node.layout.clone(),
        children: Vec::new(),
        attributes: node.attributes.clone(),
        component_width: node.component_width,
        component_height: node.component_height,
        image_width: node.image_width,
        image_height: node.image_height,
        image_data: None, // don't clone pixel data
        svg_viewbox_w: node.svg_viewbox_w,
        svg_viewbox_h: node.svg_viewbox_h,
        shadow_root: node.shadow_root.clone(),
        hover_applied: node.hover_applied,
        cascade_dirty: node.cascade_dirty,
        has_dirty_descendant: node.has_dirty_descendant,
        has_dirty_layout_descendant: node.has_dirty_layout_descendant,
    };

    // Blockify inline items in flex/grid parents
    if parent_is_flex_grid
        && matches!(frag.style.display, Display::Inline)
        && matches!(frag.style.float, Float::None)
        && !matches!(frag.style.position, Position::Absolute | Position::Fixed)
    {
        frag.style.display = Display::Block;
    }

    let is_flex_grid = matches!(frag.style.display,
        Display::Flex | Display::InlineFlex | Display::Grid | Display::InlineGrid);

    // Generate children
    let effective = node.effective_children();
    let mut children: Vec<Fragment> = Vec::new();
    let mut has_block = false;
    let mut has_inline = false;

    for child in effective {
        if matches!(child.style.display, Display::None) { continue; }

        // display:contents → reparent children
        if matches!(child.style.display, Display::Contents) {
            for gc in &child.children {
                if matches!(gc.style.display, Display::None) { continue; }
                let gc_frag = generate_node(gc, is_flex_grid);
                track_block_inline(&gc_frag, &mut has_block, &mut has_inline);
                children.push(gc_frag);
            }
            continue;
        }

        let child_frag = generate_node(child, is_flex_grid);
        track_block_inline(&child_frag, &mut has_block, &mut has_inline);
        children.push(child_frag);
    }

    // Anonymous block insertion for mixed block+inline in block containers
    if has_block && has_inline
        && !is_flex_grid
        && !matches!(frag.style.display, Display::Inline | Display::InlineBlock
            | Display::InlineFlex | Display::InlineGrid)
    {
        children = wrap_inline_runs(children, &frag.style);
    }

    // Parent-first-child margin collapsing:
    // If this block has no border-top/padding-top and is not a BFC root,
    // zero the first in-flow child's margin-top on the fragment.
    // The margin propagates to this fragment's collapsed_margin_top instead.
    if can_collapse_top_margin(&frag.style, &frag.layout) && !children.is_empty() {
        if let Some(first) = children.iter_mut().find(|c|
            !matches!(c.style.display, Display::None)
            && !matches!(c.style.position, Position::Absolute | Position::Fixed)
            && matches!(c.style.float, Float::None)
        ) {
            let child_mt = first.style.margin_top.resolve(16.0, 0.0, 16.0);
            if child_mt > 0.0 {
                // Store the real margin for collapsed_margin_top propagation
                frag.layout.collapsed_margin_top = frag.layout.collapsed_margin_top.max(child_mt);
                // Zero it in the fragment so layout doesn't add internal gap
                first.style.margin_top = CssLength::Px(0.0);
            }
        }
    }

    frag.children = children;
    frag
}

fn track_block_inline(frag: &Fragment, has_block: &mut bool, has_inline: &mut bool) {
    if matches!(frag.style.position, Position::Absolute | Position::Fixed) { return; }
    if !matches!(frag.style.float, Float::None) { return; }
    if frag.style.is_block_level() { *has_block = true; }
    else { *has_inline = true; }
}

fn can_collapse_top_margin(style: &ComputedStyle, _layout: &LayoutBox) -> bool {
    if style.establishes_bfc() { return false; }
    // Check for border-top or padding-top (use the style values, not resolved)
    if !matches!(style.border_top_width, CssLength::Px(0.0) | CssLength::Auto) { return false; }
    if !matches!(style.padding_top, CssLength::Px(0.0) | CssLength::Auto) { return false; }
    true
}

/// Wrap runs of inline children in anonymous block fragments.
fn wrap_inline_runs(children: Vec<Fragment>, parent_style: &ComputedStyle) -> Vec<Fragment> {
    let mut result: Vec<Fragment> = Vec::new();
    let mut inline_run: Vec<Fragment> = Vec::new();

    for child in children {
        let is_block = child.style.is_block_level()
            && matches!(child.style.float, Float::None)
            && !matches!(child.style.position, Position::Absolute | Position::Fixed);

        if is_block {
            if !inline_run.is_empty() {
                result.push(create_anonymous_block(std::mem::take(&mut inline_run), parent_style));
            }
            result.push(child);
        } else {
            inline_run.push(child);
        }
    }

    if !inline_run.is_empty() {
        result.push(create_anonymous_block(inline_run, parent_style));
    }

    result
}

fn create_anonymous_block(children: Vec<Fragment>, parent_style: &ComputedStyle) -> Fragment {
    let mut style = ComputedStyle::default();
    style.color = parent_style.color;
    style.font_size = parent_style.font_size.clone();
    style.font_family = parent_style.font_family.clone();
    style.font_weight = parent_style.font_weight;
    style.font_style = parent_style.font_style;
    style.line_height = parent_style.line_height.clone();
    style.text_align = parent_style.text_align;
    style.white_space = parent_style.white_space;
    style.direction = parent_style.direction;
    style.display = Display::Block;

    Fragment {
        node_id: 0,
        tag: "#anon-block".to_string(),
        text: String::new(),
        style,
        layout: LayoutBox::default(),
        children,
        attributes: HashMap::new(),
        component_width: 0.0,
        component_height: 0.0,
        image_width: 0,
        image_height: 0,
        image_data: None,
        svg_viewbox_w: 0.0,
        svg_viewbox_h: 0.0,
        shadow_root: None,
        hover_applied: false,
        cascade_dirty: false,
        has_dirty_descendant: false,
        has_dirty_layout_descendant: false,
    }
}

// ─── Write-back ───────────────────────────────────────────────────────────────

/// Copy layout results from fragment tree back to DOM tree.
pub fn write_back(frag: &Fragment, dom: &mut HtmlBox) {
    if frag.node_id == dom.node_id && frag.node_id != 0 {
        dom.layout = frag.layout.clone();
    }

    // Walk fragment children, match to DOM children by node_id
    let mut dom_idx = 0;
    for fc in &frag.children {
        if fc.node_id == 0 {
            // Anonymous fragment — its children map to DOM children
            write_back_anon(fc, dom);
            continue;
        }
        while dom_idx < dom.children.len() {
            if dom.children[dom_idx].node_id == fc.node_id {
                write_back(fc, &mut dom.children[dom_idx]);
                dom_idx += 1;
                break;
            }
            dom_idx += 1;
        }
    }
}

fn write_back_anon(frag: &Fragment, dom: &mut HtmlBox) {
    for fc in &frag.children {
        if fc.node_id == 0 {
            write_back_anon(fc, dom);
            continue;
        }
        if let Some(dc) = dom.children.iter_mut().find(|c| c.node_id == fc.node_id) {
            write_back(fc, dc);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::load_html;

    #[test]
    fn fragment_generation_basic() {
        let doc = load_html("<div><p>Hello</p><p>World</p></div>", 400.0);
        let frag = generate_fragments(&doc.root);
        assert_eq!(frag.tag, "html");
        assert!(!frag.children.is_empty());
    }

    #[test]
    fn fragment_display_none_excluded() {
        let doc = load_html("<div><span style='display:none'>hidden</span><span>visible</span></div>", 400.0);
        let frag = generate_fragments(&doc.root);

        fn count(f: &Fragment) -> usize {
            1 + f.children.iter().map(|c| count(c)).sum::<usize>()
        }
        fn has_none(f: &Fragment) -> bool {
            if matches!(f.style.display, Display::None) { return true; }
            f.children.iter().any(|c| has_none(c))
        }

        assert!(!has_none(&frag), "display:none should not appear in fragment tree");
    }

    #[test]
    fn fragment_anonymous_blocks() {
        let doc = load_html("<div>text <p>block</p> more</div>", 400.0);
        let frag = generate_fragments(&doc.root);

        fn find_div(f: &Fragment) -> Option<&Fragment> {
            if f.tag == "div" { return Some(f); }
            f.children.iter().find_map(|c| find_div(c))
        }

        if let Some(div) = find_div(&frag) {
            let has_anon = div.children.iter().any(|c| c.node_id == 0);
            assert!(has_anon, "mixed block+inline should produce anonymous blocks");
        }
    }

    #[test]
    fn write_back_preserves_dom() {
        let mut doc = load_html("<div id='a'><p id='b'>Hello</p></div>", 400.0);
        let orig_y = doc.root.layout.content_rect.y;
        let frag = generate_fragments(&doc.root);
        write_back(&frag, &mut doc.root);
        // Layout values should be preserved (or overwritten with same values)
        assert_eq!(doc.root.layout.content_rect.y, orig_y);
    }
}
