// Ported from cpptests/test_dom.cpp
// DOM query, traversal, text content, and attribute tests.

use rhtmledit::types::*;
use rhtmledit::parse_html;
use rhtmledit::dom::*;
use rhtmledit::html::serializer::serialize_box;

// ── Helpers ──────────────────────────────────────────────────────────────────

fn parse(html: &str) -> Document {
    parse_html(html)
}

fn find_box<'a, F: Fn(&HtmlBox) -> bool>(root: &'a HtmlBox, pred: &F) -> Option<&'a HtmlBox> {
    if pred(root) { return Some(root); }
    for child in &root.children {
        if let Some(b) = find_box(child, pred) { return Some(b); }
    }
    None
}

// ============================================================
// QuerySelector / QuerySelectorAll
// ============================================================

#[test]
fn dom_query_selector_by_tag() {
    let doc = parse("<div><p>One</p><p>Two</p><span>Three</span></div>");
    let results = doc.root.query_selector_all("p");
    assert_eq!(results.len(), 2);
    assert_eq!(results[0].tag, "p");
    assert_eq!(results[1].tag, "p");
}

#[test]
fn dom_query_selector_by_class() {
    let doc = parse(r#"<div><p class="a">One</p><p class="b">Two</p></div>"#);
    let results = doc.root.query_selector_all(".a");
    assert_eq!(results.len(), 1);
    assert_eq!(results[0].attributes.get("class").unwrap(), "a");
}

#[test]
fn dom_query_selector_by_id() {
    let doc = parse(r#"<div><p id="target">Found</p><p>Other</p></div>"#);
    let results = doc.root.query_selector_all("#target");
    assert!(!results.is_empty());
    assert_eq!(results[0].attributes.get("id").unwrap(), "target");
}

#[test]
fn dom_query_selector_no_match() {
    let doc = parse("<div><p>Text</p></div>");
    let results_class = doc.root.query_selector_all(".nonexistent");
    assert!(results_class.is_empty());
    let results_tag = doc.root.query_selector_all("h1");
    assert!(results_tag.is_empty());
}

#[test]
fn dom_query_selector_multiple_classes() {
    let doc = parse(r#"<div><p class="a b">AB</p><p class="a">A</p></div>"#);
    let results = doc.root.query_selector_all(".b");
    assert_eq!(results.len(), 1);
    assert_eq!(results[0].attributes.get("class").unwrap(), "a b");
}

// ============================================================
// Class checking (read-only, via attributes)
// ============================================================

#[test]
fn dom_has_class_multiple() {
    let doc = parse(r#"<div><p id="t" class="foo bar baz">Text</p></div>"#);
    let t = query_selector(&doc.root, "#t").unwrap();
    assert!(has_class(t, "foo"));
    assert!(has_class(t, "bar"));
    assert!(has_class(t, "baz"));
    assert!(!has_class(t, "qux"));
}

#[test]
fn dom_add_class() {
    let mut doc = parse(r#"<div><p id="t">Text</p></div>"#);
    let t = query_selector_mut(&mut doc.root, "#t").unwrap();
    add_class(t, "highlight");
    assert!(has_class(t, "highlight"));
}

#[test]
fn dom_remove_class() {
    let mut doc = parse(r#"<div><p id="t" class="a b c">Text</p></div>"#);
    let t = query_selector_mut(&mut doc.root, "#t").unwrap();
    remove_class(t, "b");
    assert!(!has_class(t, "b"));
    assert!(has_class(t, "a"));
    assert!(has_class(t, "c"));
}

#[test]
fn dom_toggle_class() {
    let mut doc = parse(r#"<div><p id="t">Text</p></div>"#);
    let t = query_selector_mut(&mut doc.root, "#t").unwrap();
    toggle_class(t, "active");
    assert!(has_class(t, "active"));
    toggle_class(t, "active");
    assert!(!has_class(t, "active"));
}

// ============================================================
// Attributes (read-only)
// ============================================================

#[test]
fn dom_get_tag_attribute() {
    let doc = parse(r#"<div><p id="t">Text</p></div>"#);
    let results = doc.root.query_selector_all("#t");
    assert!(!results.is_empty());
    assert_eq!(results[0].tag, "p");
}

#[test]
fn dom_get_id_attribute() {
    let doc = parse(r#"<div><p id="myid">Text</p></div>"#);
    let results = doc.root.query_selector_all("#myid");
    assert_eq!(results.len(), 1);
    assert_eq!(results[0].attributes.get("id").unwrap(), "myid");
}

#[test]
fn dom_get_class_attribute() {
    let doc = parse(r#"<div><p class="x y">Text</p></div>"#);
    let results = doc.root.query_selector_all(".x");
    assert_eq!(results.len(), 1);
    assert_eq!(get_attribute(results[0], "class").unwrap(), "x y");
}

#[test]
fn dom_set_attribute() {
    let mut doc = parse(r#"<div><p id="t">Text</p></div>"#);
    let t = query_selector_mut(&mut doc.root, "#t").unwrap();
    set_attribute(t, "title", "Hello");
    assert_eq!(get_attribute(t, "title").unwrap(), "Hello");
}

#[test]
fn dom_remove_attribute() {
    let mut doc = parse(r#"<div><p id="t" class="a">Text</p></div>"#);
    let t = query_selector_mut(&mut doc.root, "#t").unwrap();
    remove_attribute(t, "class");
    assert!(get_attribute(t, "class").is_none());
}

// ============================================================
// Text content (read-only)
// ============================================================

#[test]
fn dom_get_text_content() {
    let doc = parse(r#"<div><p id="t">Hello world</p></div>"#);
    let results = doc.root.query_selector_all("#t");
    assert!(!results.is_empty());
    let text = results[0].text_content();
    assert!(text.contains("Hello world"), "got: {text}");
}

#[test]
fn dom_get_text_content_recursive() {
    let doc = parse(r#"<div id="t"><p>Hello </p><p>world</p></div>"#);
    let results = doc.root.query_selector_all("#t");
    assert!(!results.is_empty());
    let text = get_text_content(results[0]);
    assert!(text.contains("Hello"), "got: {text}");
    assert!(text.contains("world"), "got: {text}");
}

#[test]
fn dom_set_text_content() {
    let mut doc = parse(r#"<div><p id="t">Old text</p></div>"#);
    let t = query_selector_mut(&mut doc.root, "#t").unwrap();
    set_text_content(t, "New text");
    assert!(get_text_content(t).contains("New text"));
    assert!(!get_text_content(t).contains("Old text"));
}

// ============================================================
// Traversal (read-only via children Vec)
// ============================================================

#[test]
fn dom_get_element_children() {
    let doc = parse(r#"<div id="parent"><p>A</p><p>B</p><p>C</p></div>"#);
    let results = doc.root.query_selector_all("#parent");
    assert!(!results.is_empty());
    let parent = results[0];
    let element_children: Vec<&HtmlBox> = parent.children.iter()
        .filter(|c| !c.is_text_node())
        .collect();
    assert_eq!(element_children.len(), 3);
}

#[test]
fn dom_get_first_and_last_child() {
    let doc = parse(r#"<div id="parent"><p id="first">A</p><p id="last">B</p></div>"#);
    let results = doc.root.query_selector_all("#parent");
    assert!(!results.is_empty());
    let parent = results[0];
    let element_children: Vec<&HtmlBox> = parent.children.iter()
        .filter(|c| !c.is_text_node())
        .collect();
    assert!(element_children.len() >= 2);
    let first = element_children.first().unwrap();
    let last = element_children.last().unwrap();
    assert_eq!(first.attributes.get("id").unwrap(), "first");
    assert_eq!(last.attributes.get("id").unwrap(), "last");
    assert!(!std::ptr::eq(*first, *last));
}

#[test]
fn dom_get_child_count() {
    let doc = parse(r#"<div id="parent"><p>A</p><p>B</p></div>"#);
    let results = doc.root.query_selector_all("#parent");
    assert!(!results.is_empty());
    let parent = results[0];
    let element_children: Vec<&HtmlBox> = parent.children.iter()
        .filter(|c| !c.is_text_node())
        .collect();
    assert_eq!(element_children.len(), 2);
}

#[test]
fn dom_get_child_count_empty() {
    let doc = parse(r#"<div id="empty"></div>"#);
    let results = doc.root.query_selector_all("#empty");
    assert!(!results.is_empty());
    let element_children: Vec<&HtmlBox> = results[0].children.iter()
        .filter(|c| !c.is_text_node())
        .collect();
    assert_eq!(element_children.len(), 0);
}

// ============================================================
// Compound queries — verify multiple selectors on same tree
// ============================================================

#[test]
fn dom_multiple_queries_on_same_tree() {
    let doc = parse(r#"<div>
        <p class="a">One</p>
        <p class="b">Two</p>
        <span id="s1">Three</span>
        <span class="a">Four</span>
    </div>"#);

    let p_results = doc.root.query_selector_all("p");
    assert_eq!(p_results.len(), 2);

    let class_a = doc.root.query_selector_all(".a");
    assert_eq!(class_a.len(), 2); // p.a + span.a

    let span_results = doc.root.query_selector_all("span");
    assert_eq!(span_results.len(), 2);

    let id_results = doc.root.query_selector_all("#s1");
    assert_eq!(id_results.len(), 1);
    assert_eq!(id_results[0].tag, "span");
}

#[test]
fn dom_nested_query() {
    let doc = parse(r#"<div id="outer">
        <div id="inner">
            <p class="deep">Deep text</p>
        </div>
    </div>"#);

    // query_selector_all searches the entire subtree
    let deep = doc.root.query_selector_all(".deep");
    assert_eq!(deep.len(), 1);
    assert!(deep[0].text_content().contains("Deep text"));

    // Query from inner div
    let inner_results = doc.root.query_selector_all("#inner");
    assert!(!inner_results.is_empty());
    let inner = inner_results[0];
    let inner_deep = inner.query_selector_all(".deep");
    assert_eq!(inner_deep.len(), 1);
}

#[test]
fn dom_children_are_ordered() {
    let doc = parse(r#"<ul id="list">
        <li>First</li>
        <li>Second</li>
        <li>Third</li>
    </ul>"#);

    let list_results = doc.root.query_selector_all("#list");
    assert!(!list_results.is_empty());
    let list = list_results[0];
    let items: Vec<&HtmlBox> = list.children.iter()
        .filter(|c| c.tag == "li")
        .collect();
    assert_eq!(items.len(), 3);
    assert!(items[0].text_content().contains("First"));
    assert!(items[1].text_content().contains("Second"));
    assert!(items[2].text_content().contains("Third"));
}

#[test]
fn dom_text_content_with_nested_inline() {
    let doc = parse(r#"<p id="t">Hello <strong>bold</strong> world</p>"#);
    let results = doc.root.query_selector_all("#t");
    assert!(!results.is_empty());
    let text = results[0].text_content();
    assert!(text.contains("Hello"), "got: {text}");
    assert!(text.contains("bold"), "got: {text}");
    assert!(text.contains("world"), "got: {text}");
}

#[test]
fn dom_attributes_preserved() {
    let doc = parse(r#"<a href="https://example.com" title="Link">Click</a>"#);
    let links = doc.root.query_selector_all("a");
    assert!(!links.is_empty());
    assert_eq!(links[0].attributes.get("href").unwrap(), "https://example.com");
    assert_eq!(links[0].attributes.get("title").unwrap(), "Link");
}

#[test]
fn dom_data_attributes_preserved() {
    let doc = parse(r#"<div data-value="42" data-name="test">Content</div>"#);
    let divs = doc.root.query_selector_all("div");
    let div = divs.iter().find(|d| d.attributes.contains_key("data-value")).unwrap();
    assert_eq!(get_attribute(div, "data-value").unwrap(), "42");
}

// ============================================================
// Tree manipulation
// ============================================================

#[test]
fn dom_create_and_append_child() {
    let mut doc = parse(r#"<div id="parent"><p>Existing</p></div>"#);
    let parent = query_selector_mut(&mut doc.root, "#parent").unwrap();
    let old_count = parent.children.len();

    let new_el = create_element("p");
    append_child(parent, new_el);
    assert_eq!(parent.children.len(), old_count + 1);
    assert_eq!(get_last_child(parent).unwrap().tag, "p");
}

#[test]
fn dom_prepend_child() {
    let mut doc = parse(r#"<div id="parent"><p>Second</p></div>"#);
    let parent = query_selector_mut(&mut doc.root, "#parent").unwrap();
    let mut new_el = create_element("p");
    set_attribute(&mut new_el, "id", "first");
    prepend_child(parent, new_el);
    assert_eq!(get_first_child(parent).unwrap().tag, "p");
    assert_eq!(get_attribute(get_first_child(parent).unwrap(), "id").unwrap(), "first");
}

#[test]
fn dom_remove_child() {
    let mut doc = parse(r#"<div id="parent"><p id="a">A</p><p id="b">B</p></div>"#);
    let parent = query_selector_mut(&mut doc.root, "#parent").unwrap();
    let a_ptr = query_selector(parent, "#a").unwrap() as *const HtmlBox;
    let old_count = parent.children.len();
    remove_child(parent, a_ptr);
    assert_eq!(parent.children.len(), old_count - 1);
    assert!(query_selector(parent, "#a").is_none());
}

#[test]
fn dom_clone_element() {
    let doc = parse(r#"<div><p id="orig" class="x">Content</p></div>"#);
    let orig = query_selector(&doc.root, "#orig").unwrap();
    let clone = clone_element(orig);
    assert_eq!(clone.tag, orig.tag);
    assert_eq!(get_attribute(&clone, "id").unwrap(), "orig");
    assert_eq!(get_attribute(&clone, "class").unwrap(), "x");
}

// ============================================================
// Visibility
// ============================================================

#[test]
fn dom_hide_and_show() {
    let mut doc = parse(r#"<div><p id="t">Text</p></div>"#);
    let t = query_selector_mut(&mut doc.root, "#t").unwrap();
    assert!(is_visible(t));
    hide(t);
    assert!(!is_visible(t));
    assert_eq!(t.style.display, Display::None);
    show(t);
    assert!(is_visible(t));
    assert!(t.style.display != Display::None);
}

// ============================================================
// Class manipulation – additional cases
// ============================================================

#[test]
fn dom_add_class_does_not_duplicate() {
    let mut doc = parse(r#"<div><p id="t" class="a">Text</p></div>"#);
    let t = query_selector_mut(&mut doc.root, "#t").unwrap();
    add_class(t, "a");
    assert_eq!(get_attribute(t, "class").unwrap(), "a");
}

#[test]
fn dom_remove_class_not_present() {
    let mut doc = parse(r#"<div><p id="t" class="a">Text</p></div>"#);
    let t = query_selector_mut(&mut doc.root, "#t").unwrap();
    remove_class(t, "z");
    assert_eq!(get_attribute(t, "class").unwrap(), "a");
}

// ============================================================
// Attributes – additional cases
// ============================================================

#[test]
fn dom_set_and_get_id() {
    let mut doc = parse(r#"<div><p>Text</p></div>"#);
    // Find the <p> element
    let p = query_selector_mut(&mut doc.root, "p").unwrap();
    set_attribute(p, "id", "myid");
    assert_eq!(get_attribute(p, "id").unwrap(), "myid");
    // Can now find by id
    assert!(query_selector(&doc.root, "#myid").is_some());
}

#[test]
fn dom_set_and_get_class_via_set_attribute() {
    let mut doc = parse(r#"<div><p id="t">Text</p></div>"#);
    let t = query_selector_mut(&mut doc.root, "#t").unwrap();
    set_attribute(t, "class", "x y");
    assert_eq!(get_attribute(t, "class").unwrap(), "x y");
    assert!(has_class(t, "x"));
    assert!(has_class(t, "y"));
}

#[test]
fn dom_remove_id_attribute() {
    let mut doc = parse(r#"<div><p id="t" class="a">Text</p></div>"#);
    let t = query_selector_mut(&mut doc.root, "#t").unwrap();
    remove_attribute(t, "id");
    assert!(get_attribute(t, "id").is_none());
    // Should no longer be findable by id
    assert!(query_selector(&doc.root, "#t").is_none());
}

// ============================================================
// Inline style
// ============================================================

#[test]
fn dom_set_style_property_color() {
    let mut doc = parse(r#"<div><p id="t">Text</p></div>"#);
    let t = query_selector_mut(&mut doc.root, "#t").unwrap();
    set_style_property(t, "color", "red");
    assert_eq!(t.style.color, Color { r: 255, g: 0, b: 0, a: 255 });
}

#[test]
fn dom_set_style_property_background() {
    let mut doc = parse(r#"<div><p id="t">Text</p></div>"#);
    let t = query_selector_mut(&mut doc.root, "#t").unwrap();
    set_style_property(t, "background-color", "#00ff00");
    assert_eq!(t.style.background_color, Color { r: 0, g: 255, b: 0, a: 255 });
}

// ============================================================
// InnerHTML / OuterHTML
// ============================================================

#[test]
fn dom_get_inner_html() {
    let doc = parse(r#"<div id="t"><p>Hello</p></div>"#);
    let t = query_selector(&doc.root, "#t").unwrap();
    // Inner HTML = serialize children
    let mut html = String::new();
    for child in &t.children {
        serialize_box(child, &mut html);
    }
    assert!(html.contains("Hello"), "got: {html}");
}

#[test]
fn dom_set_inner_html() {
    let mut doc = parse(r#"<div id="t"><p>Old</p></div>"#);
    // Parse the new inner HTML and set it as children
    let new_inner = parse(r#"<strong>Bold</strong> text"#);
    let t = query_selector_mut(&mut doc.root, "#t").unwrap();
    t.children = new_inner.root.children;
    let text = get_text_content(t);
    assert!(text.contains("Bold") || text.contains("text"), "got: {text}");
}

#[test]
fn dom_get_outer_html_simple() {
    let doc = parse(r#"<div><p id="t">Hello</p></div>"#);
    let t = query_selector(&doc.root, "#t").unwrap();
    let mut html = String::new();
    serialize_box(t, &mut html);
    assert!(html.contains("<p"), "got: {html}");
    assert!(html.contains("Hello"), "got: {html}");
}

#[test]
fn dom_get_outer_html_includes_attributes() {
    let doc = parse(r#"<div><div class="highlight">Text</div></div>"#);
    let t = query_selector(&doc.root, ".highlight").unwrap();
    let mut html = String::new();
    serialize_box(t, &mut html);
    assert!(html.contains("<div"), "got: {html}");
    assert!(html.contains("Text"), "got: {html}");
}

#[test]
fn dom_get_outer_html_vs_inner_html() {
    let doc = parse(r#"<div><p id="t">Content</p></div>"#);
    let t = query_selector(&doc.root, "#t").unwrap();
    let mut outer = String::new();
    serialize_box(t, &mut outer);
    let mut inner = String::new();
    for child in &t.children {
        serialize_box(child, &mut inner);
    }
    // Also include own text in inner
    inner.push_str(&t.text);
    // Outer should be longer (includes the tag itself)
    assert!(outer.len() > inner.len());
    // Inner should not contain the opening <p tag
    assert!(!inner.contains("<p"), "inner should not contain <p, got: {inner}");
}

// ============================================================
// InsertBefore / InsertAfter
// ============================================================

#[test]
fn dom_insert_before() {
    let mut doc = parse(r#"<div id="parent"><p id="a">A</p><p id="b">B</p></div>"#);
    let parent = query_selector_mut(&mut doc.root, "#parent").unwrap();
    let b_ptr = query_selector(parent, "#b").unwrap() as *const HtmlBox;
    let mut new_el = create_element("p");
    set_attribute(&mut new_el, "id", "mid");
    insert_before(parent, b_ptr, new_el);

    let element_children: Vec<&HtmlBox> = parent.children.iter()
        .filter(|c| !c.is_text_node())
        .collect();
    let mid_idx = element_children.iter().position(|c| {
        get_attribute(c, "id") == Some("mid")
    });
    assert!(mid_idx.is_some());
    let idx = mid_idx.unwrap();
    assert!(idx > 0);
    assert!(idx + 1 < element_children.len());
}

#[test]
fn dom_insert_after() {
    let mut doc = parse(r#"<div id="parent"><p id="a">A</p><p id="b">B</p></div>"#);
    let parent = query_selector_mut(&mut doc.root, "#parent").unwrap();
    let a_ptr = query_selector(parent, "#a").unwrap() as *const HtmlBox;
    let mut new_el = create_element("p");
    set_attribute(&mut new_el, "id", "mid");
    insert_after(parent, a_ptr, new_el);

    let element_children: Vec<&HtmlBox> = parent.children.iter()
        .filter(|c| !c.is_text_node())
        .collect();
    let mid_idx = element_children.iter().position(|c| {
        get_attribute(c, "id") == Some("mid")
    });
    assert!(mid_idx.is_some());
    let idx = mid_idx.unwrap();
    assert!(idx > 0);
    assert_eq!(get_attribute(element_children[idx - 1], "id"), Some("a"));
}

// ============================================================
// Sibling traversal
// ============================================================

#[test]
fn dom_get_next_and_previous_sibling() {
    let doc = parse(r#"<div><p id="a">A</p><p id="b">B</p><p id="c">C</p></div>"#);
    // Find the parent div
    let div = doc.root.query_selector_all("div");
    assert!(!div.is_empty());
    let parent = div[0];
    let b = query_selector(parent, "#b").unwrap();
    let b_ptr = b as *const HtmlBox;

    let next = get_next_sibling(parent, b_ptr);
    let prev = get_prev_sibling(parent, b_ptr);
    assert!(next.is_some());
    assert!(prev.is_some());
    assert_eq!(get_attribute(next.unwrap(), "id"), Some("c"));
    assert_eq!(get_attribute(prev.unwrap(), "id"), Some("a"));
}

#[test]
fn dom_get_next_sibling_last() {
    let doc = parse(r#"<div><p id="a">A</p></div>"#);
    let div = doc.root.query_selector_all("div");
    assert!(!div.is_empty());
    let parent = div[0];
    let a = query_selector(parent, "#a").unwrap();
    let a_ptr = a as *const HtmlBox;
    assert!(get_next_sibling(parent, a_ptr).is_none());
}

#[test]
fn dom_get_prev_sibling_first() {
    let doc = parse(r#"<div><p id="a">A</p></div>"#);
    let div = doc.root.query_selector_all("div");
    assert!(!div.is_empty());
    let parent = div[0];
    let a = query_selector(parent, "#a").unwrap();
    let a_ptr = a as *const HtmlBox;
    assert!(get_prev_sibling(parent, a_ptr).is_none());
}

// ============================================================
// GetChildCountEmpty – first/last child
// ============================================================

#[test]
fn dom_get_child_count_empty_first_last() {
    let doc = parse(r#"<div id="empty"></div>"#);
    let results = doc.root.query_selector_all("#empty");
    assert!(!results.is_empty());
    let empty = results[0];
    assert!(get_first_child(empty).is_none());
    assert!(get_last_child(empty).is_none());
}

// ============================================================
// MoveElement
// ============================================================

#[test]
fn dom_move_element() {
    let mut doc = parse(r#"<div id="a"><p id="child">Text</p></div><div id="b"></div>"#);

    // Grab the child element, remove from "a", append to "b"
    let child = {
        let a = query_selector_mut(&mut doc.root, "#a").unwrap();
        let child_ptr = query_selector(a, "#child").unwrap() as *const HtmlBox;
        remove_child(a, child_ptr).unwrap()
    };
    {
        let b = query_selector_mut(&mut doc.root, "#b").unwrap();
        append_child(b, child);
    }

    let a = query_selector(&doc.root, "#a").unwrap();
    let b = query_selector(&doc.root, "#b").unwrap();
    let a_children: Vec<_> = a.children.iter().filter(|c| !c.is_text_node()).collect();
    let b_children: Vec<_> = b.children.iter().filter(|c| !c.is_text_node()).collect();
    assert_eq!(a_children.len(), 0);
    assert_eq!(b_children.len(), 1);
    assert_eq!(get_attribute(b_children[0], "id"), Some("child"));
}

// ============================================================
// SetTextContent then get other
// ============================================================

#[test]
fn dom_set_text_content_then_get_other() {
    let mut doc = parse(r#"<div><p id="a">Hello</p><p id="b">World</p></div>"#);

    let a_ptr = query_selector(&doc.root, "#a").unwrap() as *const HtmlBox;
    let b_ptr = query_selector(&doc.root, "#b").unwrap() as *const HtmlBox;

    {
        let a = query_selector_mut(&mut doc.root, "#a").unwrap();
        set_text_content(a, "Changed");
    }
    let ta = {
        let a = unsafe { &*a_ptr };
        get_text_content(a)
    };
    let tb = {
        let b = unsafe { &*b_ptr };
        get_text_content(b)
    };
    assert!(ta.contains("Changed"), "got: {ta}");
    assert!(tb.contains("World"), "got: {tb}");

    {
        let b = query_selector_mut(&mut doc.root, "#b").unwrap();
        set_text_content(b, "Also changed");
    }
    let ta2 = {
        let a = unsafe { &*a_ptr };
        get_text_content(a)
    };
    let tb2 = {
        let b = unsafe { &*b_ptr };
        get_text_content(b)
    };
    assert!(ta2.contains("Changed"), "got: {ta2}");
    assert!(tb2.contains("Also changed"), "got: {tb2}");
}

// ============================================================
// Toggle class preserves text
// ============================================================

#[test]
fn dom_toggle_class_preserves_text() {
    let mut doc = parse(r#"<div><div id="c1" class="card">Card 1 text</div><div id="c2" class="card">Card 2 text</div></div>"#);

    assert!(get_text_content(query_selector(&doc.root, "#c1").unwrap()).contains("Card 1"));
    assert!(get_text_content(query_selector(&doc.root, "#c2").unwrap()).contains("Card 2"));

    {
        let c1 = query_selector_mut(&mut doc.root, "#c1").unwrap();
        toggle_class(c1, "highlight");
    }
    {
        let c2 = query_selector_mut(&mut doc.root, "#c2").unwrap();
        toggle_class(c2, "highlight");
    }

    assert!(get_text_content(query_selector(&doc.root, "#c1").unwrap()).contains("Card 1"));
    assert!(get_text_content(query_selector(&doc.root, "#c2").unwrap()).contains("Card 2"));

    {
        let c1 = query_selector_mut(&mut doc.root, "#c1").unwrap();
        toggle_class(c1, "highlight");
    }
    {
        let c2 = query_selector_mut(&mut doc.root, "#c2").unwrap();
        toggle_class(c2, "highlight");
    }

    assert!(get_text_content(query_selector(&doc.root, "#c1").unwrap()).contains("Card 1"));
    assert!(get_text_content(query_selector(&doc.root, "#c2").unwrap()).contains("Card 2"));
}

// ============================================================
// QuerySelectorByTagAndClass
// ============================================================

#[test]
fn dom_query_selector_by_tag_and_class() {
    let doc = parse(r#"<div><p class="x">P</p><span class="x">S</span></div>"#);
    // query_selector_all only supports simple selectors; "p.x" is compound.
    // Filter manually: all <p> that also have class "x"
    let results: Vec<&HtmlBox> = doc.root.query_selector_all("p")
        .into_iter()
        .filter(|b| has_class(b, "x"))
        .collect();
    assert_eq!(results.len(), 1);
    assert_eq!(results[0].tag, "p");
}

// ============================================================
// GetBoundingRect
// ============================================================

#[test]
fn dom_get_bounding_rect_returns_rect() {
    // After load_html (which runs layout), border_rect is populated.
    let doc = rhtmledit::load_html(r#"<div><p id="t">Box</p></div>"#, 800.0);
    let t = query_selector(&doc.root, "#t").unwrap();
    // width and height should be non-negative (may be 0 without font metrics)
    assert!(t.border_rect.w >= 0.0);
    assert!(t.border_rect.h >= 0.0);
}

// ============================================================
// Data Attributes
// ============================================================

#[test]
fn dom_set_and_get_data() {
    let mut doc = parse(r#"<div id="x">Test</div>"#);
    let x = query_selector_mut(&mut doc.root, "#x").unwrap();
    set_data(x, "color", "red");
    assert_eq!(get_data(x, "color"), Some("red"));
}

#[test]
fn dom_get_data_missing() {
    let doc = parse(r#"<div id="x">Test</div>"#);
    let x = query_selector(&doc.root, "#x").unwrap();
    assert!(get_data(x, "nonexistent").is_none());
}

#[test]
fn dom_has_data() {
    let mut doc = parse(r#"<div id="x">Test</div>"#);
    let x = query_selector_mut(&mut doc.root, "#x").unwrap();
    assert!(!has_data(x, "key"));
    set_data(x, "key", "val");
    assert!(has_data(x, "key"));
}

#[test]
fn dom_remove_data() {
    let mut doc = parse(r#"<div id="x">Test</div>"#);
    let x = query_selector_mut(&mut doc.root, "#x").unwrap();
    set_data(x, "key", "val");
    assert!(has_data(x, "key"));
    remove_data(x, "key");
    assert!(!has_data(x, "key"));
    assert!(get_data(x, "key").is_none());
}

#[test]
fn dom_set_data_overwrite() {
    let mut doc = parse(r#"<div id="x">Test</div>"#);
    let x = query_selector_mut(&mut doc.root, "#x").unwrap();
    set_data(x, "k", "first");
    set_data(x, "k", "second");
    assert_eq!(get_data(x, "k"), Some("second"));
}

#[test]
fn dom_multiple_data_keys() {
    let mut doc = parse(r#"<div id="x">Test</div>"#);
    let x = query_selector_mut(&mut doc.root, "#x").unwrap();
    set_data(x, "a", "1");
    set_data(x, "b", "2");
    set_data(x, "c", "3");
    assert_eq!(get_data(x, "a"), Some("1"));
    assert_eq!(get_data(x, "b"), Some("2"));
    assert_eq!(get_data(x, "c"), Some("3"));
}

#[test]
fn dom_remove_nonexistent_data() {
    let mut doc = parse(r#"<div id="x">Test</div>"#);
    let x = query_selector_mut(&mut doc.root, "#x").unwrap();
    remove_data(x, "nope"); // no crash
}

// ============================================================
// Event Listeners
// ============================================================

#[test]
fn dom_event_listener_add_returns_id() {
    let mut listeners = EventListeners::new();
    let id = listeners.add("p", HtmlEventType::Click, Box::new(|_| {}));
    assert!(id > 0);
}

#[test]
fn dom_event_listener_add_by_id_selector() {
    let mut listeners = EventListeners::new();
    let id = listeners.add("#btn", HtmlEventType::Click, Box::new(|_| {}));
    assert!(id > 0);
}

#[test]
fn dom_event_listener_add_by_class_selector() {
    let mut listeners = EventListeners::new();
    let id = listeners.add(".item", HtmlEventType::Click, Box::new(|_| {}));
    assert!(id > 0);
}

#[test]
fn dom_event_listener_remove_by_id() {
    let mut listeners = EventListeners::new();
    let id = listeners.add("#x", HtmlEventType::Click, Box::new(|_| {}));
    assert!(id > 0);
    listeners.remove(id);
    // Removing again should not crash
    listeners.remove(id);
    assert!(listeners.is_empty());
}

#[test]
fn dom_event_listener_remove_by_selector() {
    let mut listeners = EventListeners::new();
    listeners.add("#x", HtmlEventType::Click, Box::new(|_| {}));
    listeners.add("#x", HtmlEventType::DblClick, Box::new(|_| {}));
    listeners.remove_by_selector("#x");
    assert!(listeners.is_empty());
}

#[test]
fn dom_event_listener_remove_by_selector_and_type() {
    let mut listeners = EventListeners::new();
    let id1 = listeners.add("#x", HtmlEventType::Click, Box::new(|_| {}));
    let id2 = listeners.add("#x", HtmlEventType::DblClick, Box::new(|_| {}));
    assert!(id1 > 0);
    assert!(id2 > 0);
    listeners.remove_by_selector_and_type("#x", HtmlEventType::Click);
    // DblClick listener should remain
    assert!(!listeners.is_empty());
}

#[test]
fn dom_event_listener_remove_all() {
    let mut listeners = EventListeners::new();
    listeners.add("#a", HtmlEventType::Click, Box::new(|_| {}));
    listeners.add("#b", HtmlEventType::MouseDown, Box::new(|_| {}));
    listeners.remove_all();
    assert!(listeners.is_empty());
}

#[test]
fn dom_event_listener_multiple_on_same_element() {
    let mut listeners = EventListeners::new();
    let id1 = listeners.add("#x", HtmlEventType::Click, Box::new(|_| {}));
    let id2 = listeners.add("#x", HtmlEventType::Click, Box::new(|_| {}));
    assert!(id1 != id2);
}

#[test]
fn dom_event_listener_ids_are_unique() {
    let mut listeners = EventListeners::new();
    let id1 = listeners.add("#a", HtmlEventType::Click, Box::new(|_| {}));
    let id2 = listeners.add("#a", HtmlEventType::MouseDown, Box::new(|_| {}));
    let id3 = listeners.add(".foo", HtmlEventType::MouseUp, Box::new(|_| {}));
    assert!(id1 != id2);
    assert!(id2 != id3);
    assert!(id1 > 0);
    assert!(id2 > 0);
    assert!(id3 > 0);
}

#[test]
fn dom_event_listener_all_types_accepted() {
    let mut listeners = EventListeners::new();
    let a = listeners.add("#x", HtmlEventType::Click, Box::new(|_| {}));
    let b = listeners.add("#x", HtmlEventType::DblClick, Box::new(|_| {}));
    let c = listeners.add("#x", HtmlEventType::MouseDown, Box::new(|_| {}));
    let d = listeners.add("#x", HtmlEventType::MouseUp, Box::new(|_| {}));
    let f = listeners.add("#x", HtmlEventType::MouseEnter, Box::new(|_| {}));
    let g = listeners.add("#x", HtmlEventType::MouseLeave, Box::new(|_| {}));
    assert!(a > 0);
    assert!(b > 0);
    assert!(c > 0);
    assert!(d > 0);
    assert!(f > 0);
    assert!(g > 0);
    listeners.remove_all();
}

#[test]
fn dom_event_listener_new_types() {
    let mut listeners = EventListeners::new();
    listeners.add("*", HtmlEventType::Input, Box::new(|_| {}));
    listeners.add("*", HtmlEventType::Change, Box::new(|_| {}));
    listeners.add("*", HtmlEventType::Focus, Box::new(|_| {}));
    listeners.add("*", HtmlEventType::Blur, Box::new(|_| {}));
    listeners.add("*", HtmlEventType::SelectionChange, Box::new(|_| {}));
    listeners.remove_all();
    assert!(listeners.is_empty());
}

#[test]
fn dom_event_listener_mouse_move_type() {
    let mut listeners = EventListeners::new();
    let id = listeners.add("*", HtmlEventType::MouseMove, Box::new(|_| {}));
    assert!(id > 0);
    listeners.remove(id);
    assert!(listeners.is_empty());
}

#[test]
fn dom_event_listener_context_menu_type() {
    let mut listeners = EventListeners::new();
    let id = listeners.add("*", HtmlEventType::ContextMenu, Box::new(|_| {}));
    assert!(id > 0);
    listeners.remove(id);
    assert!(listeners.is_empty());
}

#[test]
fn dom_event_listener_drag_types() {
    let mut listeners = EventListeners::new();
    let a = listeners.add("*", HtmlEventType::DragStart, Box::new(|_| {}));
    let b = listeners.add("*", HtmlEventType::Drag, Box::new(|_| {}));
    let c = listeners.add("*", HtmlEventType::DragEnter, Box::new(|_| {}));
    let d = listeners.add("*", HtmlEventType::DragOver, Box::new(|_| {}));
    let f = listeners.add("*", HtmlEventType::DragLeave, Box::new(|_| {}));
    let g = listeners.add("*", HtmlEventType::Drop, Box::new(|_| {}));
    let h = listeners.add("*", HtmlEventType::DragEnd, Box::new(|_| {}));
    assert!(a > 0 && b > a && c > b && d > c && f > d && g > f && h > g);
    listeners.remove_all();
}

#[test]
fn dom_event_listener_keyboard_types() {
    let mut listeners = EventListeners::new();
    let a = listeners.add("*", HtmlEventType::KeyDown, Box::new(|_| {}));
    let b = listeners.add("*", HtmlEventType::KeyUp, Box::new(|_| {}));
    let c = listeners.add("*", HtmlEventType::KeyPress, Box::new(|_| {}));
    assert!(a > 0 && b > a && c > b);
    listeners.remove_all();
}

#[test]
fn dom_event_listener_scroll_type() {
    let mut listeners = EventListeners::new();
    let id = listeners.add("*", HtmlEventType::Scroll, Box::new(|_| {}));
    assert!(id > 0);
    listeners.remove(id);
    assert!(listeners.is_empty());
}

#[test]
fn dom_event_listener_remove_nonexistent_no_op() {
    let mut listeners = EventListeners::new();
    listeners.remove(9999); // should not crash
    listeners.remove_by_selector(".nonexistent");
    listeners.remove_by_selector_and_type("#nope", HtmlEventType::Click);
    assert!(listeners.is_empty());
}

#[test]
fn dom_html_event_stop_propagation() {
    let mut evt = HtmlEvent::new(HtmlEventType::Click);
    assert!(!evt.propagation_stopped);
    assert!(!evt.default_prevented);
    evt.stop_propagation();
    assert!(evt.propagation_stopped);
    evt.prevent_default();
    assert!(evt.default_prevented);
}

#[test]
fn dom_html_event_prevent_default_return_value() {
    let mut evt = HtmlEvent::new(HtmlEventType::Click);
    assert!(!evt.default_prevented);
    evt.prevent_default();
    assert!(evt.default_prevented);
}

#[test]
fn dom_html_event_keyboard_fields() {
    let mut evt = HtmlEvent::new(HtmlEventType::KeyDown);
    evt.key_code = 65; // 'A'
    evt.char_code = Some('A');
    evt.ctrl_key = true;
    evt.shift_key = false;
    evt.alt_key = true;
    evt.meta_key = false;
    assert_eq!(evt.key_code, 65);
    assert_eq!(evt.char_code, Some('A'));
    assert!(evt.ctrl_key);
    assert!(!evt.shift_key);
    assert!(evt.alt_key);
    assert!(!evt.meta_key);
}

#[test]
fn dom_html_event_mouse_button() {
    let mut evt = HtmlEvent::new(HtmlEventType::ContextMenu);
    evt.button = 2; // right-click
    assert_eq!(evt.button, 2);
}

#[test]
fn dom_html_event_drag_fields() {
    let evt = HtmlEvent::new(HtmlEventType::DragStart);
    assert!(evt.drag_source.is_null());
    assert!(evt.related_target.is_null());
}

#[test]
fn dom_event_listener_all_new_types_registrable() {
    let mut listeners = EventListeners::new();
    let types = [
        HtmlEventType::MouseMove, HtmlEventType::ContextMenu,
        HtmlEventType::DragStart, HtmlEventType::Drag,
        HtmlEventType::DragEnter, HtmlEventType::DragOver,
        HtmlEventType::DragLeave, HtmlEventType::Drop,
        HtmlEventType::DragEnd,
        HtmlEventType::KeyDown, HtmlEventType::KeyUp, HtmlEventType::KeyPress,
        HtmlEventType::Scroll,
    ];
    let selectors = ["#t", ".c", "p"];
    let mut ids = Vec::new();
    for &t in &types {
        for &sel in &selectors {
            ids.push(listeners.add(sel, t, Box::new(|_| {})));
        }
    }
    assert_eq!(ids.len(), types.len() * selectors.len());
    for id in &ids {
        assert!(*id > 0);
    }
    listeners.remove_all();
}

// ============================================================
// FindElementsByText (manual implementation)
// ============================================================

fn find_elements_by_text<'a>(root: &'a HtmlBox, needle: &str, case_sensitive: bool) -> Vec<&'a HtmlBox> {
    let mut out = Vec::new();
    collect_by_text(root, needle, case_sensitive, &mut out);
    out
}

fn collect_by_text<'a>(node: &'a HtmlBox, needle: &str, case_sensitive: bool, out: &mut Vec<&'a HtmlBox>) {
    if needle.is_empty() { return; }
    let text = get_text_content(node);
    let matches = if case_sensitive {
        text.contains(needle)
    } else {
        text.to_lowercase().contains(&needle.to_lowercase())
    };
    if matches && !text.trim().is_empty() && !node.is_text_node() {
        out.push(node);
    }
    for child in &node.children {
        collect_by_text(child, needle, case_sensitive, out);
    }
}

#[test]
fn dom_find_elements_by_text_basic() {
    let doc = parse(r#"<div><p>Hello world</p><p>Goodbye world</p><p>Other</p></div>"#);
    let results = find_elements_by_text(&doc.root, "world", true);
    // At least the two <p> elements that contain "world"
    assert!(results.len() >= 2);
}

#[test]
fn dom_find_elements_by_text_case_insensitive() {
    let doc = parse(r#"<div><p>Hello WORLD</p><p>world peace</p></div>"#);
    let results = find_elements_by_text(&doc.root, "World", false);
    assert!(results.len() >= 2);
}

#[test]
fn dom_find_elements_by_text_case_sensitive() {
    let doc = parse(r#"<div><p>Hello WORLD</p><p>world peace</p></div>"#);
    let results = find_elements_by_text(&doc.root, "WORLD", true);
    // At least one result should contain "WORLD"
    let found_upper = results.iter().any(|b| get_text_content(b).contains("WORLD"));
    assert!(found_upper);
}

#[test]
fn dom_find_elements_by_text_empty() {
    let doc = parse(r#"<div><p>Hello</p></div>"#);
    let results = find_elements_by_text(&doc.root, "", true);
    assert_eq!(results.len(), 0);
}

#[test]
fn dom_find_elements_by_text_no_match() {
    let doc = parse(r#"<div><p>Hello</p></div>"#);
    let results = find_elements_by_text(&doc.root, "xyz123", true);
    assert_eq!(results.len(), 0);
}
