//! `Range` — DOM §5.
//!
//! Chrome-measured throughout (`/tmp/webcore-html/rg1.html`, `rg2`, `rg3`).
//! Two rows contradicted the obvious reading and are called out where they
//! land: `insertNode` leaves the start in the SHORTENED text node rather than
//! in the parent, and `deleteContents` across two text nodes leaves TWO
//! adjacent text nodes rather than one merged one.

use crate::dom::range::*;
use crate::html::parse_html;
use crate::types::Document;

const PAGE: &str = "<div id=r><p id=p1>Hello<b id=b1>World</b>Tail</p><p id=p2>Second</p></div>";

fn page() -> Document { parse_html(PAGE) }
fn el(d: &Document, id: &str) -> u32 { d.get_element_by_id(id).unwrap() }
/// The four text nodes: "Hello", "World", "Tail", "Second".
fn texts(d: &Document) -> (u32, u32, u32, u32) {
    let p1 = el(d, "p1");
    let b1 = el(d, "b1");
    let p2 = el(d, "p2");
    let k = d.child_nodes(p1);
    (k[0], d.child_nodes(b1)[0], k[2], d.child_nodes(p2)[0])
}
fn html(d: &Document, id: u32) -> String { d.inner_html(id) }

// ─── boundary points ────────────────────────────────────────────────────────

#[test]
fn a_fresh_range_is_collapsed_on_the_document() {
    let mut d = page();
    let r = d.create_range();
    assert_eq!(d.range_start_container(r), Some(d.document_node()));
    assert_eq!(d.range_start_offset(r), Some(0));
    assert!(d.range_collapsed(r));
}

#[test]
fn an_offset_past_the_nodes_length_is_an_index_size_error() {
    let mut d = page();
    let (t1, ..) = texts(&d);
    let p1 = el(&d, "p1");
    let r = d.create_range();
    assert!(!d.range_set_start(r, t1, 99), "past the DATA length");
    assert!(!d.range_set_start(r, p1, 99), "past the CHILD count");
    assert!(d.range_set_start(r, t1, 5), "exactly the length is legal");
    assert!(d.range_set_start(r, p1, 3));
}

#[test]
fn a_boundary_that_crosses_the_other_one_drags_it_along() {
    let mut d = page();
    let (t1, ..) = texts(&d);
    let r = d.create_range();
    d.range_set_start(r, t1, 3);
    d.range_set_end(r, t1, 1);
    assert_eq!((d.range_start_offset(r), d.range_end_offset(r)), (Some(1), Some(1)));

    let r = d.create_range();
    d.range_set_start(r, t1, 1);
    d.range_set_end(r, t1, 3);
    d.range_set_start(r, t1, 4);
    assert_eq!((d.range_start_offset(r), d.range_end_offset(r)), (Some(4), Some(4)));
}

#[test]
fn a_boundary_in_another_tree_moves_the_whole_range_there() {
    // ⛔ Not a range straddling two trees — Chrome collapses it onto the new
    // point in the new tree.
    let mut d = page();
    let (t1, ..) = texts(&d);
    let r = d.create_range();
    d.range_set_start(r, t1, 1);
    d.range_set_end(r, t1, 3);
    let loose = d.create_text_node("x");
    assert!(d.range_set_start(r, loose, 0));
    assert_eq!(d.range_start_container(r), Some(loose));
    assert_eq!(d.range_end_container(r), Some(loose), "the end came too");
    assert!(d.range_collapsed(r));
}

#[test]
fn select_node_spans_the_node_and_select_node_contents_spans_its_insides() {
    let mut d = page();
    let p1 = el(&d, "p1");
    let b1 = el(&d, "b1");
    let (t1, ..) = texts(&d);

    let r = d.create_range();
    assert!(d.range_select_node(r, b1));
    assert_eq!(d.range_start_container(r), Some(p1), "both boundaries sit in the PARENT");
    assert_eq!((d.range_start_offset(r), d.range_end_offset(r)), (Some(1), Some(2)));
    assert!(!d.range_collapsed(r));

    let r = d.create_range();
    d.range_select_node_contents(r, p1);
    assert_eq!((d.range_start_offset(r), d.range_end_offset(r)), (Some(0), Some(3)));

    let r = d.create_range();
    d.range_select_node_contents(r, t1);
    assert_eq!((d.range_start_offset(r), d.range_end_offset(r)), (Some(0), Some(5)), "a text node's LENGTH");
}

#[test]
fn set_start_before_and_after_address_the_node_from_its_parent() {
    let mut d = page();
    let p1 = el(&d, "p1");
    let b1 = el(&d, "b1");
    let r = d.create_range();
    assert!(d.range_set_start_before(r, b1));
    assert!(d.range_set_end_after(r, b1));
    assert_eq!(d.range_start_container(r), Some(p1));
    assert_eq!((d.range_start_offset(r), d.range_end_offset(r)), (Some(1), Some(2)));
}

#[test]
fn collapse_defaults_to_the_end_not_the_start() {
    let mut d = page();
    let p1 = el(&d, "p1");
    let r = d.create_range();
    d.range_select_node_contents(r, p1);
    d.range_collapse(r, true);
    assert_eq!((d.range_start_offset(r), d.range_end_offset(r)), (Some(0), Some(0)));

    let r = d.create_range();
    d.range_select_node_contents(r, p1);
    d.range_collapse(r, false);
    assert_eq!((d.range_start_offset(r), d.range_end_offset(r)), (Some(3), Some(3)));
}

#[test]
fn the_common_ancestor_rises_only_as_far_as_it_must() {
    let mut d = page();
    let (t1, t2, _, t4) = texts(&d);
    let p1 = el(&d, "p1");
    let r_ = el(&d, "r");

    let r = d.create_range();
    d.range_set_start(r, t1, 1);
    d.range_set_end(r, t1, 3);
    assert_eq!(d.common_ancestor_container(r), Some(t1), "the text node itself");

    let r = d.create_range();
    d.range_set_start(r, t1, 1);
    d.range_set_end(r, t2, 3);
    assert_eq!(d.common_ancestor_container(r), Some(p1));

    let r = d.create_range();
    d.range_set_start(r, t1, 1);
    d.range_set_end(r, t4, 3);
    assert_eq!(d.common_ancestor_container(r), Some(r_));
}

// ─── comparisons ────────────────────────────────────────────────────────────

#[test]
fn compare_boundary_points_reads_its_operands_in_the_opposite_order_to_its_name() {
    // ⛔ `START_TO_END` compares THIS range's END to the other's START. a is
    // (1,3), b is (2,4); the four answers are [-1, 1, -1, -1].
    let mut d = page();
    let (t1, ..) = texts(&d);
    let a = d.create_range();
    d.range_set_start(a, t1, 1);
    d.range_set_end(a, t1, 3);
    let b = d.create_range();
    d.range_set_start(b, t1, 2);
    d.range_set_end(b, t1, 4);
    assert_eq!(d.compare_boundary_points(a, START_TO_START, b), Some(-1));
    assert_eq!(d.compare_boundary_points(a, START_TO_END, b), Some(1), "a.END vs b.START");
    assert_eq!(d.compare_boundary_points(a, END_TO_END, b), Some(-1));
    assert_eq!(d.compare_boundary_points(a, END_TO_START, b), Some(-1), "a.START vs b.END");

    let c = d.create_range();
    d.range_set_start(c, t1, 1);
    d.range_set_end(c, t1, 3);
    assert_eq!(d.compare_boundary_points(a, START_TO_START, c), Some(0));
    assert_eq!(d.compare_boundary_points(a, 9, c), None, "NotSupportedError");
}

#[test]
fn compare_points_and_is_point_in_range_agree_on_the_boundaries() {
    let mut d = page();
    let (t1, ..) = texts(&d);
    let r = d.create_range();
    d.range_set_start(r, t1, 1);
    d.range_set_end(r, t1, 3);
    assert_eq!(d.range_compare_point(r, t1, 0), Some(-1));
    assert_eq!(d.range_compare_point(r, t1, 2), Some(0));
    assert_eq!(d.range_compare_point(r, t1, 4), Some(1));
    // Inclusive of both ends.
    assert!(!d.is_point_in_range(r, t1, 0));
    assert!(d.is_point_in_range(r, t1, 1));
    assert!(d.is_point_in_range(r, t1, 2));
    assert!(d.is_point_in_range(r, t1, 3));

    let loose = d.create_element("div");
    assert_eq!(d.range_compare_point(r, loose, 0), None, "WrongDocumentError");
}

#[test]
fn intersects_node_covers_ancestors_and_stops_at_untouched_siblings() {
    let mut d = page();
    let (t1, t2, ..) = texts(&d);
    let p1 = el(&d, "p1");
    let b1 = el(&d, "b1");
    let p2 = el(&d, "p2");
    let r = d.create_range();
    d.range_set_start(r, t1, 1);
    d.range_set_end(r, t2, 2);
    assert!(d.range_intersects_node(r, p1));
    assert!(d.range_intersects_node(r, b1));
    assert!(d.range_intersects_node(r, t1));
    assert!(!d.range_intersects_node(r, p2), "the second paragraph is untouched");
}

#[test]
fn a_cloned_range_is_independent_and_detach_does_nothing() {
    let mut d = page();
    let (t1, ..) = texts(&d);
    let r = d.create_range();
    d.range_set_start(r, t1, 1);
    d.range_set_end(r, t1, 3);
    let c = d.range_clone(r).unwrap();
    d.range_set_end(c, t1, 4);
    assert_eq!(d.range_end_offset(r), Some(3));
    assert_eq!(d.range_end_offset(c), Some(4));
    d.range_detach(r);
    assert!(d.range_set_start(r, t1, 2), "still usable");
}

// ─── toString ───────────────────────────────────────────────────────────────

#[test]
fn to_string_collects_the_text_between_the_boundaries() {
    let mut d = page();
    let (t1, _, t3, _) = texts(&d);
    let p1 = el(&d, "p1");

    let r = d.create_range();
    d.range_set_start(r, t1, 1);
    d.range_set_end(r, t1, 3);
    assert_eq!(d.range_to_string(r), "el");

    let r = d.create_range();
    d.range_set_start(r, t1, 2);
    d.range_set_end(r, t3, 2);
    assert_eq!(d.range_to_string(r), "lloWorldTa", "through the <b> in the middle");

    let r = d.create_range();
    d.range_select_node_contents(r, p1);
    assert_eq!(d.range_to_string(r), "HelloWorldTail");

    let r = d.create_range();
    d.range_set_start(r, p1, 0);
    d.range_set_end(r, p1, 2);
    assert_eq!(d.range_to_string(r), "HelloWorld", "element boundaries take whole children");
}

// ─── the mutating members ───────────────────────────────────────────────────

#[test]
fn delete_contents_inside_one_text_node_is_a_data_edit() {
    let mut d = page();
    let (t1, ..) = texts(&d);
    let r_ = el(&d, "r");
    let r = d.create_range();
    d.range_set_start(r, t1, 1);
    d.range_set_end(r, t1, 3);
    d.delete_contents(r);
    assert!(html(&d, r_).contains("Hlo"), "{}", html(&d, r_));
    assert_eq!((d.range_start_offset(r), d.range_end_offset(r)), (Some(1), Some(1)));
    assert!(d.range_collapsed(r));
}

#[test]
fn delete_contents_across_nodes_leaves_two_adjacent_text_nodes() {
    // ⛔ Nothing normalizes. Chrome: `childNodes.length` is 2 — `"He"` and
    // `"il"` — and merging them would pass every `toString` test.
    let mut d = page();
    let (t1, _, t3, _) = texts(&d);
    let p1 = el(&d, "p1");
    let r = d.create_range();
    d.range_set_start(r, t1, 2);
    d.range_set_end(r, t3, 2);
    d.delete_contents(r);
    let kids = d.child_nodes(p1);
    assert_eq!(kids.len(), 2, "two text nodes, not one: {:?}", kids.iter().map(|k| d.text_data(*k)).collect::<Vec<_>>());
    assert_eq!(d.text_data(kids[0]), "He");
    assert_eq!(d.text_data(kids[1]), "il");
}

#[test]
fn delete_contents_across_paragraphs_keeps_both_partially_covered_parents() {
    let mut d = page();
    let (t1, _, _, t4) = texts(&d);
    let p1 = el(&d, "p1");
    let p2 = el(&d, "p2");
    let r = d.create_range();
    d.range_set_start(r, t1, 2);
    d.range_set_end(r, t4, 3);
    d.delete_contents(r);
    assert_eq!(d.text_content(p1), "He");
    assert_eq!(d.text_content(p2), "ond");
}

#[test]
fn clone_contents_leaves_the_tree_alone_and_extract_does_not() {
    let mut d = page();
    let (t1, _, t3, _) = texts(&d);
    let r_ = el(&d, "r");
    let before = html(&d, r_);

    let r = d.create_range();
    d.range_set_start(r, t1, 2);
    d.range_set_end(r, t3, 2);
    let frag = d.clone_contents(r).unwrap();
    assert_eq!(html(&d, r_), before, "cloneContents does not touch the tree");
    assert_eq!(d.text_content(frag), "lloWorldTa");
    assert_eq!(d.node_type(frag), 11, "a DocumentFragment");

    let r = d.create_range();
    d.range_set_start(r, t1, 2);
    d.range_set_end(r, t3, 2);
    let frag = d.extract_contents(r).unwrap();
    assert_eq!(d.text_content(frag), "lloWorldTa");
    assert_eq!(d.text_content(el(&d, "p1")), "Heil", "and the tree lost it");
}

#[test]
fn extracting_across_paragraphs_clones_the_partially_covered_parents() {
    // ⛔ The fragment gets CLONES of both `<p>`s, ids and all, while the
    // originals stay in the tree holding what is left.
    let mut d = page();
    let (t1, _, _, t4) = texts(&d);
    let r = d.create_range();
    d.range_set_start(r, t1, 2);
    d.range_set_end(r, t4, 3);
    let frag = d.extract_contents(r).unwrap();
    let kids = d.child_nodes(frag);
    assert_eq!(kids.len(), 2, "two cloned paragraphs");
    assert_eq!(d.tag_name(kids[0]), Some("p"));
    assert_eq!(d.get_attribute(kids[0], "id").as_deref(), Some("p1"), "the clone kept the id");
    assert_eq!(d.text_content(kids[0]), "lloWorldTail");
    assert_eq!(d.text_content(kids[1]), "Sec");
    // The originals are still the ones in the document.
    assert_eq!(d.text_content(el(&d, "p1")), "He");
    assert_eq!(d.text_content(el(&d, "p2")), "ond");
}

#[test]
fn insert_node_mid_text_splits_it_and_leaves_the_start_in_the_first_half() {
    // ⛔ Measured with the CONTAINERS printed, not just the offsets: the start
    // stays in the shortened `"H"` at offset 1, and the end moves into the new
    // `"ello"` at offset 2. The offsets alone (`[1, 2]`) read as if both had
    // moved to the parent.
    let mut d = page();
    let (t1, ..) = texts(&d);
    let p1 = el(&d, "p1");
    let r = d.create_range();
    d.range_set_start(r, t1, 1);
    d.range_set_end(r, t1, 3);
    let i = d.create_element("i");
    let it = d.create_text_node("I");
    d.append_child(i, it);
    assert!(d.insert_node(r, i));

    assert_eq!(d.text_content(p1), "HIelloWorldTail");
    assert_eq!(d.range_start_container(r), Some(t1), "still the FIRST half");
    assert_eq!(d.text_data(t1), "H");
    assert_eq!(d.range_start_offset(r), Some(1));
    assert_eq!(d.range_end_offset(r), Some(2));
    assert_eq!(d.range_to_string(r), "Iel");
}

#[test]
fn insert_node_at_an_element_offset_needs_no_split() {
    let mut d = page();
    let p1 = el(&d, "p1");
    let r = d.create_range();
    d.range_set_start(r, p1, 1);
    d.range_collapse(r, true);
    let i = d.create_element("i");
    let it = d.create_text_node("I");
    d.append_child(i, it);
    assert!(d.insert_node(r, i));
    let kids = d.child_nodes(p1);
    assert_eq!(d.tag_name(kids[1]), Some("i"), "between the text and the <b>");
}

#[test]
fn surround_contents_wraps_and_refuses_a_partially_covered_element() {
    let mut d = page();
    let (t1, t2, ..) = texts(&d);
    let p1 = el(&d, "p1");
    let r = d.create_range();
    d.range_set_start(r, t1, 1);
    d.range_set_end(r, t1, 3);
    let u = d.create_element("u");
    assert!(d.surround_contents(r, u));
    assert_eq!(d.text_content(p1), "HelloWorldTail", "the text is unchanged");
    assert_eq!(d.range_to_string(r), "el");
    assert_eq!(d.text_content(u), "el");

    // ⛔ InvalidStateError: the range runs from inside one text node into the
    // <b>'s text, so <b> is partially contained.
    let mut d = page();
    let (t1, t2, ..) = texts(&d);
    let r = d.create_range();
    d.range_set_start(r, t1, 2);
    d.range_set_end(r, t2, 2);
    let u = d.create_element("u");
    assert!(!d.surround_contents(r, u));
    let _ = t2;
}

#[test]
fn surround_contents_discards_the_wrappers_existing_children() {
    let mut d = page();
    let (t1, ..) = texts(&d);
    let r = d.create_range();
    d.range_set_start(r, t1, 1);
    d.range_set_end(r, t1, 3);
    let u = d.create_element("u");
    let old = d.create_text_node("old");
    d.append_child(u, old);
    assert!(d.surround_contents(r, u));
    assert_eq!(d.text_content(u), "el", "\"old\" is gone");
}

// ─── live updating: the four shapes that move differently ───────────────────

#[test]
fn an_insertion_before_the_start_shifts_both_offsets() {
    let mut d = page();
    let p1 = el(&d, "p1");
    let r = d.create_range();
    d.range_set_start(r, p1, 1);
    d.range_set_end(r, p1, 2);
    let i = d.create_element("i");
    let first = d.child_nodes(p1)[0];
    d.insert_before(p1, i, first);
    assert_eq!((d.range_start_offset(r), d.range_end_offset(r)), (Some(2), Some(3)));
}

#[test]
fn a_removal_before_the_start_shifts_both_offsets_back() {
    let mut d = page();
    let (t1, ..) = texts(&d);
    let p1 = el(&d, "p1");
    let r = d.create_range();
    d.range_set_start(r, p1, 1);
    d.range_set_end(r, p1, 3);
    d.remove_child(t1);
    assert_eq!((d.range_start_offset(r), d.range_end_offset(r)), (Some(0), Some(2)));
}

#[test]
fn removing_the_containers_ancestor_moves_the_container_itself() {
    // ⛔ The only one of the four that changes the CONTAINER rather than an
    // offset. Measured: a range inside the <b>'s text lands on `(p1, 1)`.
    let mut d = page();
    let (_, t2, ..) = texts(&d);
    let b1 = el(&d, "b1");
    let p1 = el(&d, "p1");
    let r = d.create_range();
    d.range_set_start(r, t2, 1);
    d.range_set_end(r, t2, 3);
    d.remove_child(b1);
    assert_eq!(d.range_start_container(r), Some(p1), "no longer the text node");
    assert_eq!((d.range_start_offset(r), d.range_end_offset(r)), (Some(1), Some(1)));
    assert!(d.range_collapsed(r));
}

#[test]
fn replacing_character_data_moves_a_boundary_inside_the_replaced_run_to_its_start() {
    let mut d = page();
    let (t1, ..) = texts(&d);
    // The `data` setter is `replaceData(0, length, …)`, so BOTH boundaries sit
    // inside the replaced run and both collapse to 0.
    let r = d.create_range();
    d.range_set_start(r, t1, 1);
    d.range_set_end(r, t1, 4);
    d.set_text_data(t1, "Hi");
    assert_eq!((d.range_start_offset(r), d.range_end_offset(r)), (Some(0), Some(0)));
}

#[test]
fn the_three_character_data_mutators_each_move_the_offsets_differently() {
    // insertData shifts, deleteData shrinks-and-clamps, appendData does not
    // touch a range that ends before it. One rule, three visible outcomes.
    let mut d = page();
    let (t1, ..) = texts(&d);
    let r = d.create_range();
    d.range_set_start(r, t1, 2);
    d.range_set_end(r, t1, 4);
    d.insert_data(t1, 0, "XX");
    assert_eq!((d.range_start_offset(r), d.range_end_offset(r)), (Some(4), Some(6)));
    assert_eq!(d.text_data(t1), "XXHello");

    let mut d = page();
    let (t1, ..) = texts(&d);
    let r = d.create_range();
    d.range_set_start(r, t1, 2);
    d.range_set_end(r, t1, 5);
    d.delete_data(t1, 0, 2);
    assert_eq!((d.range_start_offset(r), d.range_end_offset(r)), (Some(0), Some(3)));
    assert_eq!(d.text_data(t1), "llo");

    let mut d = page();
    let (t1, ..) = texts(&d);
    let r = d.create_range();
    d.range_set_start(r, t1, 2);
    d.range_set_end(r, t1, 4);
    d.append_data(t1, "ZZ");
    assert_eq!((d.range_start_offset(r), d.range_end_offset(r)), (Some(2), Some(4)), "untouched");
}

#[test]
fn splitting_a_text_node_moves_a_boundary_past_the_cut_into_the_new_node() {
    // ⛔ A split is NOT a delete plus an insert as far as a range is
    // concerned. Its internals fire both of those, and letting their generic
    // hooks run would apply a second, wrong adjustment on top of the split's
    // own rule.
    let mut d = page();
    let (t1, ..) = texts(&d);
    let r = d.create_range();
    d.range_set_start(r, t1, 1);
    d.range_set_end(r, t1, 4);
    let new = d.split_text(t1, 2).unwrap();
    assert_eq!(d.range_start_container(r), Some(t1), "before the cut: stays put");
    assert_eq!(d.range_start_offset(r), Some(1));
    assert_eq!(d.range_end_container(r), Some(new), "past the cut: moves");
    assert_eq!(d.range_end_offset(r), Some(2));
    assert_eq!(d.text_data(t1), "He");
    assert_eq!(d.text_data(new), "llo");
}

#[test]
fn ranges_survive_a_document_clone_where_traversals_do_not() {
    // The difference is the callback: a range holds none, so it clones.
    let mut d = page();
    let (t1, ..) = texts(&d);
    let r = d.create_range();
    d.range_set_start(r, t1, 1);
    d.range_set_end(r, t1, 3);
    let clone = d.clone();
    assert_eq!(clone.range_start_offset(r), Some(1));
    assert_eq!(clone.range_end_offset(r), Some(3));
}

#[test]
fn comparing_two_ranges_in_different_trees_is_a_wrong_document_error() {
    // ⛔ A mutation found this: `compare_points` had a tree-root guard that no
    // test reached, because `setStart` has its OWN guard and every other
    // caller went through it. Chrome throws `WrongDocumentError` here and
    // answers `false` for `intersectsNode` across trees.
    let mut d = page();
    let (t1, ..) = texts(&d);
    let a = d.create_range();
    d.range_set_start(a, t1, 1);
    d.range_set_end(a, t1, 3);

    let loose = d.create_element("div");
    let loose_text = d.create_text_node("xyz");
    d.append_child(loose, loose_text);
    let b = d.create_range();
    d.range_set_start(b, loose_text, 0);
    d.range_set_end(b, loose_text, 2);

    assert_eq!(d.compare_boundary_points(a, START_TO_START, b), None);
    assert_eq!(d.compare_boundary_points(a, END_TO_END, b), None);
    assert!(!d.range_intersects_node(a, loose));
}

#[test]
fn an_edit_exactly_on_a_boundary_does_not_move_that_boundary() {
    // ⛔ Two mutations found this: every earlier fixture edited BEFORE the
    // start, where `>` and `>=` agree. The comparison is strict, and Chrome
    // shows what that means — a node inserted at the START offset lands INSIDE
    // the range (the end moves, the start does not), and one inserted at the
    // END offset lands outside it.
    const MARKUP: &str = "<div id=d><a id=c0></a><a id=c1></a><a id=c2></a><a id=c3></a></div>";
    // (what to do, at which index, expected start, expected end)
    let cases: &[(&str, usize, usize, usize)] = &[
        ("insert", 1, 1, 4), // exactly on the start
        ("insert", 3, 1, 3), // exactly on the end
        ("insert", 4, 1, 3), // past the end
        ("remove", 1, 1, 2), // the node at the start
        ("remove", 3, 1, 3), // the node at the end
        ("remove", 2, 1, 2), // inside
    ];
    for (what, index, want_start, want_end) in cases {
        let mut d = parse_html(MARKUP);
        let host = d.get_element_by_id("d").unwrap();
        let r = d.create_range();
        d.range_set_start(r, host, 1);
        d.range_set_end(r, host, 3);
        let kids = d.child_nodes(host);
        match *what {
            "insert" => {
                let i = d.create_element("i");
                match kids.get(*index) {
                    Some(reference) => d.insert_before(host, i, *reference),
                    None => d.append_child(host, i),
                }
            }
            _ => d.remove_child(kids[*index]),
        }
        assert_eq!(
            (d.range_start_offset(r), d.range_end_offset(r)),
            (Some(*want_start), Some(*want_end)),
            "{what} at {index}"
        );
    }
}
