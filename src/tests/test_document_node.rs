//! The document node.
//!
//! The tree's root `WebCore` is the `<html>` ELEMENT, so there was no node for
//! the document — the only `NodeType::Document` in the arena was the dead
//! sentinel in slot 0. A dozen IDL members need one to answer at all:
//! `getRootNode`, `ownerDocument`, `documentElement`, `parentNode` of `<html>`,
//! and `nodeType == 9`. Expectations checked against Chrome.

use crate::parse_html;

fn doc() -> crate::Document {
    parse_html("<html><body><div id=a><span id=b>x</span></div></body></html>")
}

#[test]
fn the_document_is_node_type_9() {
    let d = doc();
    assert_eq!(d.node_type(d.document_node()), 9, "DOCUMENT_NODE");
    assert_eq!(d.node_type(d.document_element().unwrap()), 1, "the document ELEMENT is an element");
}

#[test]
fn html_parent_is_the_document() {
    let d = doc();
    // `<html>` read as an orphan before, which is why `getRootNode()` stopped
    // at the document element instead of reaching the document.
    assert_eq!(d.parent_node(d.document_element().unwrap()), d.document_node());
    // And the document has no parent.
    assert_eq!(d.parent_node(d.document_node()), 0);
}

#[test]
fn get_root_node_reaches_the_document() {
    let d = doc();
    let b = d.query_selector("#b").unwrap();
    assert_eq!(d.get_root_node(b, false), d.document_node());
    assert_eq!(d.get_root_node(d.document_node(), false), d.document_node());
}

#[test]
fn owner_document_is_the_document_and_null_for_it() {
    let d = doc();
    let b = d.query_selector("#b").unwrap();
    assert_eq!(d.owner_document(b), Some(d.document_node()));
    // DOM §4.4: null for the document itself. This arm was unreachable before,
    // because `node_type` never answered 9 for anything live.
    assert_eq!(d.owner_document(d.document_node()), None);
}

#[test]
fn document_element_is_html() {
    let d = doc();
    assert_eq!(d.tag_name(d.document_element().unwrap()), Some("html"));
}

#[test]
fn a_shadow_node_roots_at_its_shadow_tree_not_the_document() {
    let d = parse_html(
        "<div id=host><template shadowrootmode=open><span id=inner>s</span></template></div>");
    let host = d.query_selector("#host").unwrap();
    let inner = d.shadow_query_selector(host, "#inner").unwrap();
    // Without `composed`, the root is the shadow tree — that is what makes a
    // shadow root a root. With it, the walk crosses to the document.
    assert_ne!(d.get_root_node(inner, false), d.document_node());
    assert_eq!(d.get_root_node(host, false), d.document_node());
}
