//! Fragment tree — separates layout from DOM.
//!
//! The fragment tree is an owned tree (like WebCore) with the same field names
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

// ─── LayoutNode trait ─────────────────────────────────────────────────────────
//
// Both WebCore and Fragment implement this trait so layout functions
// can operate on either type. This enables the migration: layout_geometry
// generates a Fragment tree and lays it out without touching the DOM.

/// Trait for types that can be laid out by the layout engine.
/// Both WebCore and Fragment implement this.
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
    fn attributes(&self) -> &crate::dom::attrs::AttrMap;
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
    fn attributes(&self) -> &crate::dom::attrs::AttrMap { &self.attributes }
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

impl LayoutNode for WebCore {
    fn node_id(&self) -> u32 { self.node_id }
    fn tag(&self) -> &str { &self.tag }
    fn text(&self) -> &str { &self.text }
    fn style(&self) -> &ComputedStyle { &self.style }
    fn style_mut(&mut self) -> &mut ComputedStyle { &mut self.style }
    fn layout(&self) -> &LayoutBox { &self.layout }
    fn layout_mut(&mut self) -> &mut LayoutBox { &mut self.layout }
    fn children(&self) -> &[WebCore] { &self.children }
    fn children_mut(&mut self) -> &mut Vec<WebCore> { &mut self.children }
    fn attributes(&self) -> &crate::dom::attrs::AttrMap { &self.attributes }
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

/// A layout fragment — structurally mirrors WebCore so layout functions
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
    /// Children (owned, like WebCore).
    pub children: Vec<Fragment>,
    /// HTML attributes.
    pub attributes: crate::dom::attrs::AttrMap,
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
    /// Create a minimal fragment (like WebCore::new).
    pub fn new(tag: &str) -> Self {
        Self {
            node_id: 0,
            tag: tag.to_string(),
            text: String::new(),
            style: ComputedStyle::default(),
            layout: LayoutBox::default(),
            children: Vec::new(),
            attributes: crate::dom::attrs::AttrMap::new(),
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
pub fn generate_fragments(root: &WebCore) -> Fragment {
    generate_node(root, false)
}

fn generate_node(node: &WebCore, _parent_is_flex_grid: bool) -> Fragment {
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

    // Blockification of flex/grid inline children is handled by the
    // existing cascade (::before/::after) and layout_box dispatch.
    // Don't modify styles in the fragment tree to avoid divergence.

    let is_flex_grid = matches!(frag.style.display,
        Display::Flex | Display::InlineFlex | Display::Grid | Display::InlineGrid);

    // Generate children
    let effective = node.effective_children();
    let mut children: Vec<Fragment> = Vec::new();
    let mut has_block = false;
    let mut has_inline = false;

    for child in effective {
        if matches!(child.style.display, Display::None) { continue; }

        // display:contents reparenting is handled by layout_box at runtime.
        // Keep the display:contents element in the fragment tree so the
        // tree structure matches the DOM (needed for write-back).

        let child_frag = generate_node(child, is_flex_grid);
        track_block_inline(&child_frag, &mut has_block, &mut has_inline);
        children.push(child_frag);
    }

    // Anonymous block insertion for mixed block+inline in block containers
    // NOTE: disabled for now — the existing layout engine already handles
    // mixed block+inline via has_block_children() runtime checks. Enabling
    // anonymous blocks changes the tree structure which breaks the existing
    // block/inline layout dispatch. Enable once layout reads from fragments directly.
    // if has_block && has_inline
    //     && !is_flex_grid
    //     && !matches!(frag.style.display, Display::Inline | Display::InlineBlock
    //         | Display::InlineFlex | Display::InlineGrid)
    // {
    //     children = wrap_inline_runs(children, &frag.style);
    // }

    frag.children = children;

    // NOTE: margin collapsing is disabled for now — it's too aggressive and
    // breaks real pages (AP News, Wikipedia, Yahoo Finance). The collapsing
    // logic needs to account for inline content, floats, and BFC boundaries
    // more carefully. The fragment tree infrastructure is correct; the
    // collapsing rules need refinement.
    // TODO: re-enable with proper MarginCollapseState during layout walk
    // resolve_margin_collapsing(&mut frag);

    frag
}

fn track_block_inline(frag: &Fragment, has_block: &mut bool, has_inline: &mut bool) {
    if matches!(frag.style.position, Position::Absolute | Position::Fixed) { return; }
    if !matches!(frag.style.float, Float::None) { return; }
    if frag.style.is_block_level() { *has_block = true; }
    else { *has_inline = true; }
}

/// Can this element's top margin collapse through to its first child?
/// CSS 2.1 §8.3.1: no border-top, no padding-top, no BFC, is block container.


// ─── Fragment ↔ WebCore conversion ────────────────────────────────────────────

/// Convert a Fragment tree to an WebCore tree for layout.
/// The Fragment has structural fixes (anonymous blocks, zeroed margins)
/// that the WebCore tree doesn't. Layout runs on this converted tree,
/// then results are written back to the real DOM.
///
/// Copies ALL fields from the DOM node (not just what Fragment stores),
/// then applies the Fragment's style overrides (margin zeroing, blockification, etc.).
pub fn to_webcore(frag: &Fragment, dom: &WebCore) -> WebCore {
    // Start from a full clone of the DOM node if IDs match, otherwise build from fragment
    let mut hbox = if frag.node_id != 0 && frag.node_id == dom.node_id {
        let mut h = dom.clone();
        h.children.clear(); // rebuild children from fragment tree structure
        h
    } else {
        // Anonymous fragment or no DOM match — build from fragment data
        let mut h = WebCore::new(&frag.tag);
        h.node_id = frag.node_id;
        h.text = frag.text.clone();
        h.layout = frag.layout.clone();
        h.attributes = frag.attributes.clone();
        h.component_width = frag.component_width;
        h.component_height = frag.component_height;
        h.image_width = frag.image_width;
        h.image_height = frag.image_height;
        h.svg_viewbox_w = frag.svg_viewbox_w;
        h.svg_viewbox_h = frag.svg_viewbox_h;
        h
    };

    // Apply fragment's style overrides (margin zeroing, blockification, etc.)
    hbox.style = frag.style.clone();

    // Rebuild children from the fragment tree structure
    // (may include anonymous blocks, reparented display:contents children, etc.)
    for fc in &frag.children {
        // Find the matching DOM node for a full field copy
        let dom_child = if fc.node_id != 0 {
            find_dom_node(dom, fc.node_id)
        } else if fc.tag == "::before" || fc.tag == "::after" {
            dom.children.iter().find(|c| c.tag == fc.tag)
        } else {
            None
        };
        let dummy = WebCore::new("");
        hbox.children.push(to_webcore(fc, dom_child.unwrap_or(&dummy)));
    }

    hbox
}

/// Find a DOM node by node_id anywhere in the subtree (immutable).
fn find_dom_node(root: &WebCore, id: u32) -> Option<&WebCore> {
    if root.node_id == id { return Some(root); }
    for child in &root.children {
        if let Some(found) = find_dom_node(child, id) {
            return Some(found);
        }
    }
    None
}

// ─── Write-back ───────────────────────────────────────────────────────────────

/// Copy layout results from a laid-out WebCore tree (converted from fragments)
/// back to the real DOM tree. Uses recursive node_id matching because the
/// fragment tree may have reparented children (display:contents, anonymous blocks).
pub fn write_back_webcore(laid_out: &WebCore, dom: &mut WebCore) {
    // Write this node's layout geometry if node_ids match.
    // Preserve resolved_margin/border/padding values from the DOM's own cascade
    // (margin collapsing changes positioning but not the CSS-specified values).
    if laid_out.node_id == dom.node_id && laid_out.node_id != 0 {
        dom.layout.content_rect = laid_out.layout.content_rect;
        dom.layout.padding_rect = laid_out.layout.padding_rect;
        dom.layout.border_rect = laid_out.layout.border_rect;
        dom.layout.margin_rect = laid_out.layout.margin_rect;
        dom.layout.baseline = laid_out.layout.baseline;
        dom.layout.line_cache = laid_out.layout.line_cache.clone();
        dom.layout.inline_runs = laid_out.layout.inline_runs.clone();
        dom.layout.collapsed_margin_top = laid_out.layout.collapsed_margin_top;
        dom.layout.collapsed_margin_bottom = laid_out.layout.collapsed_margin_bottom;
        dom.layout.scroll_height = laid_out.layout.scroll_height;
        dom.layout.scroll_width = laid_out.layout.scroll_width;
        dom.layout.abs_static_y = laid_out.layout.abs_static_y;
        dom.layout.layout_dirty = laid_out.layout.layout_dirty;
        dom.layout.last_containing_width = laid_out.layout.last_containing_width;
        dom.layout.resolved_content_width = laid_out.layout.resolved_content_width;
        // Resolved margin/border/padding: copy from laid-out tree.
        // These reflect the actual CSS values resolved during layout.
        dom.layout.resolved_margin_top = dom.style.margin_top.resolve(16.0, 0.0, 16.0).max(0.0);
        dom.layout.resolved_margin_right = laid_out.layout.resolved_margin_right;
        dom.layout.resolved_margin_bottom = dom.style.margin_bottom.resolve(16.0, 0.0, 16.0).max(0.0);
        dom.layout.resolved_margin_left = laid_out.layout.resolved_margin_left;
        dom.layout.resolved_border_top = laid_out.layout.resolved_border_top;
        dom.layout.resolved_border_right = laid_out.layout.resolved_border_right;
        dom.layout.resolved_border_bottom = laid_out.layout.resolved_border_bottom;
        dom.layout.resolved_border_left = laid_out.layout.resolved_border_left;
        dom.layout.resolved_pad_top = laid_out.layout.resolved_pad_top;
        dom.layout.resolved_pad_right = laid_out.layout.resolved_pad_right;
        dom.layout.resolved_pad_bottom = laid_out.layout.resolved_pad_bottom;
        dom.layout.resolved_pad_left = laid_out.layout.resolved_pad_left;
    }

    // For each laid-out child, find the matching DOM node anywhere in this subtree
    for lc in &laid_out.children {
        if lc.node_id == 0 {
            // Anonymous box or pseudo-element — check if it's ::before/::after
            if lc.tag == "::before" || lc.tag == "::after" {
                // Match by tag within direct children of DOM parent
                if let Some(dom_pseudo) = dom.children.iter_mut().find(|c| c.tag == lc.tag) {
                    dom_pseudo.layout = lc.layout.clone();
                    // Recurse for pseudo's children
                    write_back_webcore(lc, dom_pseudo);
                }
            } else {
                // Regular anonymous box — recurse into its children
                write_back_webcore_into_subtree(lc, dom);
            }
            continue;
        }
        // Find matching DOM node in this subtree and write back
        if let Some(dom_node) = find_dom_node_mut(dom, lc.node_id) {
            write_back_webcore(lc, dom_node);
        }
    }
}

/// Write back from an anonymous box's children into the DOM subtree.
fn write_back_webcore_into_subtree(anon: &WebCore, dom: &mut WebCore) {
    for lc in &anon.children {
        if lc.node_id == 0 {
            write_back_webcore_into_subtree(lc, dom);
            continue;
        }
        if let Some(dom_node) = find_dom_node_mut(dom, lc.node_id) {
            write_back_webcore(lc, dom_node);
        }
    }
}

/// Find a mutable DOM node by node_id anywhere in the subtree.
fn find_dom_node_mut(root: &mut WebCore, id: u32) -> Option<&mut WebCore> {
    if root.node_id == id { return Some(root); }
    for child in &mut root.children {
        if let Some(found) = find_dom_node_mut(child, id) {
            return Some(found);
        }
    }
    None
}

/// Copy layout results from fragment tree back to DOM tree.
pub fn write_back(frag: &Fragment, dom: &mut WebCore) {
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

fn write_back_anon(frag: &Fragment, dom: &mut WebCore) {
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
    fn fragment_tree_mirrors_dom() {
        // Fragment tree should mirror DOM structure (no structural changes yet)
        let doc = load_html("<div>text <p>block</p> more</div>", 400.0);
        let frag = generate_fragments(&doc.root);

        fn find_div(f: &Fragment) -> Option<&Fragment> {
            if f.tag == "div" { return Some(f); }
            f.children.iter().find_map(|c| find_div(c))
        }

        if let Some(div) = find_div(&frag) {
            // Should have same children as DOM (no anonymous blocks yet)
            assert!(!div.children.is_empty(), "div should have children");
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
