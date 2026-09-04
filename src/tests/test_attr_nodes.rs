//! `NamedNodeMap` and `Attr` — DOM §4.9.1 / §4.9.2.
//!
//! Every expectation was read off Chrome first
//! (`/tmp/webcore-html/nnm.html`), including the ones that surprised me:
//! `getNamedItem("TITLE")` finds `title`, `createAttribute("Foo")` is `foo`,
//! and the attribute handed back by `removeAttributeNode` has a null
//! `ownerElement`.

use crate::dom::attrs::Attr;
use crate::html::parse_html;

fn doc(html: &str) -> crate::types::Document {
    parse_html(html)
}

#[test]
fn attributes_is_an_indexed_list_in_source_order() {
    let d = doc(r#"<div id="d" TITLE="t" data-x="1">y</div>"#);
    let el = d.get_element_by_id("d").unwrap();

    assert_eq!(d.attributes_length(el), 3, "Chrome: length=3");
    let names: Vec<String> = d.attributes(el).iter().map(|a| a.name.clone()).collect();
    assert_eq!(
        names,
        vec!["id", "title", "data-x"],
        "source order, name folded"
    );

    let first = d.attributes_item(el, 0).unwrap();
    assert_eq!(first.name, "id");
    assert_eq!(first.value, "d");
    assert_eq!(
        d.attributes_item(el, 3),
        None,
        "past the end is None, not a panic"
    );
}

#[test]
fn an_attr_answers_the_node_members() {
    let d = doc(r#"<div id="d" title="t">y</div>"#);
    let el = d.get_element_by_id("d").unwrap();
    let a = d.get_attribute_node(el, "title").unwrap();

    assert_eq!(a.node_type(), 2, "Chrome: nodeType=2");
    assert_eq!(a.node_name(), "title");
    assert_eq!(a.node_value(), "t");
    assert_eq!(a.local_name(), "title");
    assert_eq!(a.prefix(), None, "Chrome: prefix=null");
    assert_eq!(
        a.namespace_uri(),
        None,
        "Chrome: ns=null — HTML attributes have none"
    );
    assert!(a.specified(), "Chrome: specified=true, always");
    assert_eq!(a.owner_element(), Some(el), "Chrome: ownerElement.id=d");
}

#[test]
fn a_qualified_name_lookup_folds_but_a_namespaced_one_does_not() {
    let d = doc(r#"<div id="d" title="t">y</div>"#);
    let el = d.get_element_by_id("d").unwrap();

    // Chrome: getNamedItem('TITLE')=t
    assert_eq!(
        d.get_named_item(el, "TITLE").map(|a| a.value),
        Some("t".into())
    );
    // Chrome: getNamedItemNS(null,'title')=t
    assert_eq!(
        d.get_named_item_ns(el, None, "title").map(|a| a.value),
        Some("t".into())
    );
    // …and a null namespace is not a wildcard.
    assert_eq!(
        d.get_named_item_ns(el, Some("http://example.com/ns"), "title"),
        None
    );
}

#[test]
fn set_attribute_node_appends_and_reports_what_it_replaced() {
    let mut d = doc(r#"<div id="d" title="t" data-x="1">y</div>"#);
    let el = d.get_element_by_id("d").unwrap();

    let mut fresh = d.create_attribute("Foo");
    assert_eq!(
        fresh.name, "foo",
        "Chrome folds createAttribute in an HTML document"
    );
    fresh.value = "bar".into();

    let previous = d.set_attribute_node(el, fresh);
    assert_eq!(previous, None, "Chrome: setAttributeNode prev=null");
    assert_eq!(
        d.get_attribute_names(el),
        vec!["id", "title", "data-x", "foo"],
        "Chrome: now names=id,title,data-x,foo"
    );
    assert_eq!(d.get_attribute(el, "foo").as_deref(), Some("bar"));

    let replaced = d.set_attribute_node(el, Attr::new("foo", "baz")).unwrap();
    assert_eq!(
        replaced.value, "bar",
        "the SECOND set reports the value it displaced"
    );
    assert_eq!(d.get_attribute(el, "foo").as_deref(), Some("baz"));
}

#[test]
fn remove_attribute_node_hands_back_a_detached_attribute() {
    let mut d = doc(r#"<div id="d" title="t" data-x="1">y</div>"#);
    let el = d.get_element_by_id("d").unwrap();

    let node = d.get_attribute_node(el, "data-x").unwrap();
    let removed = d.remove_attribute_node(el, &node).unwrap();

    assert_eq!(removed.name, "data-x");
    assert_eq!(removed.value, "1");
    assert_eq!(
        removed.owner_element(),
        None,
        "Chrome: owner=null after removal"
    );
    assert_eq!(d.get_attribute_names(el), vec!["id", "title"]);
}

#[test]
fn removing_an_attribute_that_is_not_there_reports_it() {
    // Chrome throws NotFoundError. There is no exception channel here, so the
    // absent return IS the error — a caller that ignores it is the one being
    // told, rather than being told nothing.
    let mut d = doc(r#"<div id="d" title="t">y</div>"#);
    let el = d.get_element_by_id("d").unwrap();
    assert_eq!(d.remove_named_item(el, "nope"), None);
    assert_eq!(d.attributes_length(el), 2, "and nothing was removed");
}

#[test]
fn a_namespaced_attribute_keeps_its_prefix_and_namespace() {
    const XLINK: &str = "http://www.w3.org/1999/xlink";
    let mut d = doc(r#"<div id="d">y</div>"#);
    let el = d.get_element_by_id("d").unwrap();
    d.set_attribute_ns(el, XLINK, "xlink:href", "#target");

    let a = d.get_attribute_node_ns(el, Some(XLINK), "href").unwrap();
    assert_eq!(a.name, "xlink:href", "name is the QUALIFIED name");
    assert_eq!(a.local_name(), "href");
    assert_eq!(a.prefix(), Some("xlink"));
    assert_eq!(a.namespace_uri(), Some(XLINK));

    // `href` and `xlink:href` are two different attributes, which is the whole
    // reason `getAttributeNS` exists.
    assert_eq!(d.get_attribute_node_ns(el, None, "href"), None);

    let removed = d.remove_named_item_ns(el, Some(XLINK), "href").unwrap();
    assert_eq!(removed.name, "xlink:href");
    assert_eq!(d.get_attribute_node_ns(el, Some(XLINK), "href"), None);
}

#[test]
fn create_attribute_ns_keeps_its_case() {
    let d = doc("<div id=d></div>");
    let a = d.create_attribute_ns(Some("http://example.com/ns"), "ex:MixedCase");
    assert_eq!(
        a.name, "ex:MixedCase",
        "only HTML folds; a namespaced name does not"
    );
    assert_eq!(a.local_name(), "MixedCase");
    assert_eq!(a.owner_element(), None, "a fresh attribute has no owner");
}
