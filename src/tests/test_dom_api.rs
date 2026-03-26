//! Tests for the public DOM API (query, read, mutate, classList, style, query_selector).

use crate::html::parse_html;
use crate::dom::arena::NodeId;

// ── Query ──────────────────────────────────────────────────────────────────

#[test]
fn get_element_by_id_finds_element() {
    let doc = parse_html(r#"<div id="main"><span id="inner">text</span></div>"#);
    let inner = doc.get_element_by_id("inner");
    assert!(inner.is_some());
    assert_eq!(doc.dom_tag(inner.unwrap()), Some("span"));
}

#[test]
fn get_element_by_id_returns_none_for_missing() {
    let doc = parse_html("<div>hello</div>");
    assert!(doc.get_element_by_id("nope").is_none());
}

#[test]
fn query_selector_by_tag() {
    let doc = parse_html("<div><p>one</p><p>two</p><span>three</span></div>");
    let p = doc.query_selector("p");
    assert!(p.is_some());
    assert_eq!(doc.dom_tag(p.unwrap()), Some("p"));
}

#[test]
fn query_selector_by_class() {
    let doc = parse_html(r#"<div><span class="a">x</span><span class="b">y</span></div>"#);
    let b = doc.query_selector(".b");
    assert!(b.is_some());
    assert_eq!(doc.dom_get_attribute(b.unwrap(), "class"), Some("b".to_string()));
}

#[test]
fn query_selector_by_id() {
    let doc = parse_html(r#"<div><p id="target">hello</p></div>"#);
    let target = doc.query_selector("#target");
    assert!(target.is_some());
    assert_eq!(doc.dom_tag(target.unwrap()), Some("p"));
}

#[test]
fn query_selector_descendant() {
    let doc = parse_html(r#"<div><ul><li>item</li></ul></div>"#);
    let li = doc.query_selector("div li");
    assert!(li.is_some());
    assert_eq!(doc.dom_tag(li.unwrap()), Some("li"));
}

#[test]
fn query_selector_all_returns_all_matches() {
    let doc = parse_html("<ul><li>a</li><li>b</li><li>c</li></ul>");
    let items = doc.query_selector_all("li");
    assert_eq!(items.len(), 3);
    for &id in &items {
        assert_eq!(doc.dom_tag(id), Some("li"));
    }
}

#[test]
fn query_selector_returns_none_for_no_match() {
    let doc = parse_html("<div>hello</div>");
    assert!(doc.query_selector("span").is_none());
    assert!(doc.query_selector_all("span").is_empty());
}

// ── Read ───────────────────────────────────────────────────────────────────

#[test]
fn dom_tag_returns_tag_name() {
    let doc = parse_html("<div><p>hi</p></div>");
    let p = doc.query_selector("p").unwrap();
    assert_eq!(doc.dom_tag(p), Some("p"));
}

#[test]
fn dom_get_attribute_returns_value() {
    let doc = parse_html(r#"<a href="/link" title="tip">click</a>"#);
    let a = doc.query_selector("a").unwrap();
    assert_eq!(doc.dom_get_attribute(a, "href"), Some("/link".to_string()));
    assert_eq!(doc.dom_get_attribute(a, "title"), Some("tip".to_string()));
    assert_eq!(doc.dom_get_attribute(a, "missing"), None);
}

#[test]
fn dom_text_content_collects_all_text() {
    let doc = parse_html("<div>Hello <span>World</span>!</div>");
    let div = doc.query_selector("div").unwrap();
    let text = doc.dom_text_content(div);
    assert!(text.contains("Hello"));
    assert!(text.contains("World"));
}

#[test]
fn dom_parent_and_children() {
    let doc = parse_html("<ul><li>a</li><li>b</li></ul>");
    let ul = doc.query_selector("ul").unwrap();
    let children = doc.dom_children(ul);
    assert_eq!(children.len(), 2);

    for &child_id in &children {
        assert_eq!(doc.dom_parent(child_id), ul);
    }
}

#[test]
fn dom_sibling_navigation() {
    let doc = parse_html("<div><span>a</span><p>b</p><em>c</em></div>");
    let span = doc.query_selector("span").unwrap();
    let p = doc.query_selector("p").unwrap();
    let em = doc.query_selector("em").unwrap();

    assert_eq!(doc.dom_next_sibling(span), p);
    assert_eq!(doc.dom_next_sibling(p), em);
    assert_eq!(doc.dom_next_sibling(em), 0);

    assert_eq!(doc.dom_prev_sibling(em), p);
    assert_eq!(doc.dom_prev_sibling(p), span);
    assert_eq!(doc.dom_prev_sibling(span), 0);
}

// ── Mutate ─────────────────────────────────────────────────────────────────

#[test]
fn create_and_append_element() {
    let mut doc = parse_html("<div></div>");
    let div = doc.query_selector("div").unwrap();

    let p = doc.dom_create_element("p");
    assert!(p != 0);
    doc.dom_append_child(div, p);

    // Verify in arena
    let children = doc.dom_children(div);
    assert_eq!(children.len(), 1);
    assert_eq!(children[0], p);
    assert_eq!(doc.dom_tag(p), Some("p"));

    // Verify in HtmlBox tree
    let div_box = doc.get_box_by_id(div).unwrap();
    assert_eq!(div_box.children.len(), 1);
    assert_eq!(div_box.children[0].tag, "p");
    assert_eq!(div_box.children[0].node_id, p);
}

#[test]
fn create_and_append_text() {
    let mut doc = parse_html("<p></p>");
    let p = doc.query_selector("p").unwrap();

    let text = doc.dom_create_text("Hello World");
    doc.dom_append_child(p, text);

    assert_eq!(doc.dom_text_content(p), "Hello World");

    let p_box = doc.get_box_by_id(p).unwrap();
    assert_eq!(p_box.children.len(), 1);
    assert_eq!(p_box.children[0].text, "Hello World");
}

#[test]
fn insert_before_works() {
    let mut doc = parse_html("<ul><li>b</li></ul>");
    let ul = doc.query_selector("ul").unwrap();
    let li_b = doc.query_selector("li").unwrap();

    let li_a = doc.dom_create_element("li");
    doc.dom_insert_before(ul, li_a, li_b);

    let children = doc.dom_children(ul);
    assert_eq!(children.len(), 2);
    assert_eq!(children[0], li_a);
    assert_eq!(children[1], li_b);

    // HtmlBox tree matches
    let ul_box = doc.get_box_by_id(ul).unwrap();
    assert_eq!(ul_box.children.len(), 2);
    assert_eq!(ul_box.children[0].node_id, li_a);
    assert_eq!(ul_box.children[1].node_id, li_b);
}

#[test]
fn remove_child_works() {
    let mut doc = parse_html("<ul><li>a</li><li>b</li><li>c</li></ul>");
    let ul = doc.query_selector("ul").unwrap();
    let items = doc.query_selector_all("li");
    assert_eq!(items.len(), 3);

    // Remove middle item
    doc.dom_remove_child(items[1]);

    let remaining = doc.dom_children(ul);
    assert_eq!(remaining.len(), 2);
    assert_eq!(remaining[0], items[0]);
    assert_eq!(remaining[1], items[2]);

    // HtmlBox tree matches
    let ul_box = doc.get_box_by_id(ul).unwrap();
    assert_eq!(ul_box.children.len(), 2);
}

#[test]
fn set_attribute_updates_both_trees() {
    let mut doc = parse_html(r#"<div id="target"></div>"#);
    let div = doc.get_element_by_id("target").unwrap();

    doc.dom_set_attribute(div, "data-value", "42");

    // Arena updated
    assert_eq!(doc.dom_get_attribute(div, "data-value"), Some("42".to_string()));

    // HtmlBox updated
    let div_box = doc.get_box_by_id(div).unwrap();
    assert_eq!(div_box.attributes.get("data-value").map(|s| s.as_str()), Some("42"));
}

#[test]
fn remove_attribute_updates_both_trees() {
    let mut doc = parse_html(r#"<div id="target" data-x="old"></div>"#);
    let div = doc.get_element_by_id("target").unwrap();

    doc.dom_remove_attribute(div, "data-x");

    assert_eq!(doc.dom_get_attribute(div, "data-x"), None);
    let div_box = doc.get_box_by_id(div).unwrap();
    assert!(!div_box.attributes.contains_key("data-x"));
}

#[test]
fn set_text_content_replaces_children() {
    let mut doc = parse_html("<div><p>old</p><span>stuff</span></div>");
    let div = doc.query_selector("div").unwrap();

    doc.dom_set_text_content(div, "new text only");

    assert_eq!(doc.dom_text_content(div), "new text only");
    assert_eq!(doc.dom_children(div).len(), 0); // arena children removed

    let div_box = doc.get_box_by_id(div).unwrap();
    assert_eq!(div_box.children.len(), 0);
    assert_eq!(div_box.text, "new text only");
}

#[test]
fn set_inner_html_parses_and_replaces() {
    let mut doc = parse_html(r#"<div id="container">old</div>"#);
    let div = doc.get_element_by_id("container").unwrap();

    doc.dom_set_inner_html(div, "<p>new</p><span>content</span>");

    let children = doc.dom_children(div);
    assert_eq!(children.len(), 2);
    assert_eq!(doc.dom_tag(children[0]), Some("p"));
    assert_eq!(doc.dom_tag(children[1]), Some("span"));

    let div_box = doc.get_box_by_id(div).unwrap();
    assert_eq!(div_box.children.len(), 2);
    assert_eq!(div_box.children[0].tag, "p");
    assert_eq!(div_box.children[1].tag, "span");
}

// ── classList ──────────────────────────────────────────────────────────────

#[test]
fn class_list_add_and_contains() {
    let mut doc = parse_html("<div></div>");
    let div = doc.query_selector("div").unwrap();

    assert!(!doc.class_list_contains(div, "active"));
    doc.class_list_add(div, "active");
    assert!(doc.class_list_contains(div, "active"));

    // Adding again doesn't duplicate
    doc.class_list_add(div, "active");
    assert_eq!(doc.dom_get_attribute(div, "class"), Some("active".to_string()));
}

#[test]
fn class_list_remove() {
    let mut doc = parse_html(r#"<div class="a b c"></div>"#);
    let div = doc.query_selector("div").unwrap();

    doc.class_list_remove(div, "b");
    assert!(!doc.class_list_contains(div, "b"));
    assert!(doc.class_list_contains(div, "a"));
    assert!(doc.class_list_contains(div, "c"));
}

#[test]
fn class_list_toggle() {
    let mut doc = parse_html(r#"<div class="on"></div>"#);
    let div = doc.query_selector("div").unwrap();

    let result = doc.class_list_toggle(div, "on");
    assert!(!result); // was present, now removed
    assert!(!doc.class_list_contains(div, "on"));

    let result = doc.class_list_toggle(div, "on");
    assert!(result); // was absent, now added
    assert!(doc.class_list_contains(div, "on"));
}

// ── Inline style ───────────────────────────────────────────────────────────

#[test]
fn set_and_get_style_property() {
    let mut doc = parse_html("<div></div>");
    let div = doc.query_selector("div").unwrap();

    doc.set_style_property(div, "color", "red");
    assert_eq!(doc.get_style_property(div, "color"), Some("red".to_string()));

    doc.set_style_property(div, "font-size", "16px");
    assert_eq!(doc.get_style_property(div, "font-size"), Some("16px".to_string()));
    assert_eq!(doc.get_style_property(div, "color"), Some("red".to_string()));
}

#[test]
fn set_style_property_overwrites() {
    let mut doc = parse_html(r#"<div style="color: blue"></div>"#);
    let div = doc.query_selector("div").unwrap();

    assert_eq!(doc.get_style_property(div, "color"), Some("blue".to_string()));
    doc.set_style_property(div, "color", "red");
    assert_eq!(doc.get_style_property(div, "color"), Some("red".to_string()));
}

#[test]
fn remove_style_property() {
    let mut doc = parse_html(r#"<div style="color: red; font-size: 14px"></div>"#);
    let div = doc.query_selector("div").unwrap();

    doc.remove_style_property(div, "color");
    assert_eq!(doc.get_style_property(div, "color"), None);
    assert_eq!(doc.get_style_property(div, "font-size"), Some("14px".to_string()));
}

// ── Layout queries ─────────────────────────────────────────────────────────

#[test]
fn dom_offset_returns_zero_without_layout() {
    let doc = parse_html("<div>hi</div>");
    let div = doc.query_selector("div").unwrap();
    // Without layout pass, dimensions are 0
    assert_eq!(doc.dom_offset_width(div), 0.0);
    assert_eq!(doc.dom_offset_height(div), 0.0);
}

// ── Move existing node ─────────────────────────────────────────────────────

#[test]
fn append_child_moves_existing_node() {
    let mut doc = parse_html("<div><p>a</p></div><section></section>");
    let p = doc.query_selector("p").unwrap();
    let section = doc.query_selector("section").unwrap();
    let div = doc.query_selector("div").unwrap();

    // Move <p> from <div> to <section>
    doc.dom_append_child(section, p);

    // div should have 0 children
    assert_eq!(doc.dom_children(div).len(), 0);
    // section should have 1 child
    let sec_children = doc.dom_children(section);
    assert_eq!(sec_children.len(), 1);
    assert_eq!(sec_children[0], p);
    assert_eq!(doc.dom_parent(p), section);
}

// ── Dirty flags ────────────────────────────────────────────────────────────

#[test]
fn attribute_mutation_sets_dirty_flag() {
    let mut doc = parse_html(r#"<div id="target"></div>"#);
    let div = doc.get_element_by_id("target").unwrap();

    // Clear dirty flags
    doc.arena.get_mut(NodeId(div)).dirty = crate::dom::arena::DirtyFlags::NONE;

    doc.dom_set_attribute(div, "class", "active");

    // Should be style-dirty now
    assert!(doc.arena.get(NodeId(div)).dirty.contains(crate::dom::arena::DirtyFlags::STYLE));
}
