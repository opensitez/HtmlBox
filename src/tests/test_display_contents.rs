/// Tests for display:contents in block/flex layout contexts.
///
/// Per CSS spec, an element with `display:contents` generates no box itself;
/// its children are promoted into its parent's formatting context as if the
/// element didn't exist.  Grid layout already handles this via
/// `collect_grid_children()`, but block and flex layout must also flatten
/// display:contents children.

use crate::types::*;
use crate::css::apply_property;
use super::harness::*;

// ─── display:contents in block layout ────────────────────────────────────────

#[test]
fn display_contents_child_has_zero_box() {
    // The element with display:contents itself should have 0×0 dimensions.
    let html = r#"
        <div style="width:400px;">
            <div id="wrapper" style="display:contents;">
                <p>Hello</p>
            </div>
        </div>
    "#;
    let doc = parse_and_layout(html, 800.0);
    let wrapper = find_box(&doc.root, &|b: &HtmlBox| {
        b.attributes.get("id").map(|v| v == "wrapper").unwrap_or(false)
    }).expect("wrapper element");
    assert_eq!(wrapper.layout.content_rect.w, 0.0, "display:contents element should have 0 width");
    assert_eq!(wrapper.layout.content_rect.h, 0.0, "display:contents element should have 0 height");
}

#[test]
fn display_contents_children_visible_in_block() {
    // Children of a display:contents element inside a block container must
    // be laid out and have non-zero dimensions.
    let html = r#"
        <div style="width:400px;">
            <div style="display:contents;">
                <p id="inner">Hello world</p>
            </div>
        </div>
    "#;
    let doc = parse_and_layout(html, 800.0);
    let inner = find_box(&doc.root, &|b: &HtmlBox| {
        b.attributes.get("id").map(|v| v == "inner").unwrap_or(false)
    }).expect("inner <p>");
    assert!(inner.layout.content_rect.w > 0.0,
        "child of display:contents must have width > 0, got {}", inner.layout.content_rect.w);
    assert!(inner.layout.content_rect.h > 0.0,
        "child of display:contents must have height > 0, got {}", inner.layout.content_rect.h);
}

#[test]
fn display_contents_nested_chain() {
    // Multiple levels of display:contents should all be transparent.
    // The <p> at the bottom should still be laid out by the outermost block.
    let html = r#"
        <div style="width:400px;">
            <div style="display:contents;">
                <div style="display:contents;">
                    <div style="display:contents;">
                        <p id="deep">Deep content</p>
                    </div>
                </div>
            </div>
        </div>
    "#;
    let doc = parse_and_layout(html, 800.0);
    let deep = find_box(&doc.root, &|b: &HtmlBox| {
        b.attributes.get("id").map(|v| v == "deep").unwrap_or(false)
    }).expect("deep <p>");
    assert!(deep.layout.content_rect.h > 0.0,
        "deeply nested display:contents child must have height > 0, got {}", deep.layout.content_rect.h);
}

#[test]
fn display_contents_block_height_includes_promoted_children() {
    // The parent block's height must account for children promoted from
    // display:contents elements.
    let html = r#"
        <div id="parent" style="width:400px;">
            <div style="display:contents;">
                <p>Line one</p>
                <p>Line two</p>
            </div>
        </div>
    "#;
    let doc = parse_and_layout(html, 800.0);
    let parent = find_box(&doc.root, &|b: &HtmlBox| {
        b.attributes.get("id").map(|v| v == "parent").unwrap_or(false)
    }).expect("parent div");
    assert!(parent.layout.content_rect.h > 0.0,
        "parent must have height > 0 from promoted children, got {}", parent.layout.content_rect.h);
}

#[test]
fn display_contents_mixed_with_normal_children() {
    // A block container with both normal children and display:contents children.
    // All visible children should be laid out vertically.
    let html = r#"
        <div id="parent" style="width:400px;">
            <p id="first">First</p>
            <div style="display:contents;">
                <p id="promoted">Promoted</p>
            </div>
            <p id="last">Last</p>
        </div>
    "#;
    let doc = parse_and_layout(html, 800.0);
    let first = find_box(&doc.root, &|b: &HtmlBox| {
        b.attributes.get("id").map(|v| v == "first").unwrap_or(false)
    }).expect("first");
    let promoted = find_box(&doc.root, &|b: &HtmlBox| {
        b.attributes.get("id").map(|v| v == "promoted").unwrap_or(false)
    }).expect("promoted");
    let last = find_box(&doc.root, &|b: &HtmlBox| {
        b.attributes.get("id").map(|v| v == "last").unwrap_or(false)
    }).expect("last");

    assert!(promoted.layout.content_rect.h > 0.0,
        "promoted child must have height > 0");
    assert!(promoted.layout.margin_rect.y > first.layout.margin_rect.y,
        "promoted child must be below first child (promoted.y={}, first.y={})",
        promoted.layout.margin_rect.y, first.layout.margin_rect.y);
    assert!(last.layout.margin_rect.y > promoted.layout.margin_rect.y,
        "last child must be below promoted child (last.y={}, promoted.y={})",
        last.layout.margin_rect.y, promoted.layout.margin_rect.y);
}

// ─── display:contents in flex layout ─────────────────────────────────────────

#[test]
fn display_contents_in_flex_children_promoted() {
    // In a flex container, display:contents children should be transparent
    // and their children should become flex items.
    let html = r#"
        <div id="flex" style="display:flex; width:400px;">
            <div style="display:contents;">
                <div id="item" style="width:100px; height:50px;"></div>
            </div>
        </div>
    "#;
    let doc = parse_and_layout(html, 800.0);
    let item = find_box(&doc.root, &|b: &HtmlBox| {
        b.attributes.get("id").map(|v| v == "item").unwrap_or(false)
    }).expect("flex item");
    assert!(item.layout.content_rect.h > 0.0,
        "display:contents child in flex must be laid out, got height={}", item.layout.content_rect.h);
}

// ─── AOL/netscape.com header pattern ─────────────────────────────────────────

#[test]
fn aol_header_pattern_contents_chain_in_block() {
    // Mimics the netscape.com header structure:
    // block container > display:contents > display:contents > header(contents) > actual content
    // The actual content must be visible (not 0×0).
    let html = r#"
        <div id="page" style="width:1200px;">
            <div class="header-tw" style="display:contents;">
                <div style="display:contents;">
                    <div style="display:contents;">
                        <header style="display:contents;">
                            <div id="header-inner" style="background:yellow; padding:10px;">
                                <nav>Menu items here</nav>
                            </div>
                        </header>
                    </div>
                </div>
            </div>
            <div id="content">
                <p>Page content</p>
            </div>
        </div>
    "#;
    let doc = parse_and_layout(html, 1200.0);
    let header_inner = find_box(&doc.root, &|b: &HtmlBox| {
        b.attributes.get("id").map(|v| v == "header-inner").unwrap_or(false)
    }).expect("header-inner");
    assert!(header_inner.layout.content_rect.h > 0.0,
        "header content through display:contents chain must have height > 0, got {}",
        header_inner.layout.content_rect.h);
    assert!(header_inner.layout.content_rect.w > 100.0,
        "header content must have reasonable width, got {}", header_inner.layout.content_rect.w);

    // Content below the header must be pushed down.
    let content = find_box(&doc.root, &|b: &HtmlBox| {
        b.attributes.get("id").map(|v| v == "content").unwrap_or(false)
    }).expect("content div");
    assert!(content.layout.margin_rect.y >= header_inner.layout.margin_rect.y + header_inner.layout.margin_rect.h,
        "content must be below header (content.y={}, header bottom={})",
        content.layout.margin_rect.y, header_inner.layout.margin_rect.y + header_inner.layout.margin_rect.h);
}
