//! The table interfaces — HTML §4.9.
//!
//! Chrome-verified (`/tmp/webcore-html/tbl.html`). The source below writes
//! `<tfoot>` FIRST on purpose: `table.rows` is defined in section order, not
//! document order, and a child walk would answer `f1,h1,r1,r2,r3`.

use crate::html::parse_html;

const TABLE: &str = r#"<table id=t>
 <tfoot><tr id=f1><td>f</td></tr></tfoot>
 <thead><tr id=h1><th>h</th></tr></thead>
 <tbody id=b1><tr id=r1><td>a</td><td>b</td></tr><tr id=r2><td>c</td></tr></tbody>
 <tbody id=b2><tr id=r3><td>d</td></tr></tbody>
</table>"#;

fn table() -> crate::types::Document {
    parse_html(TABLE)
}
fn el(d: &crate::types::Document, id: &str) -> u32 {
    d.get_element_by_id(id).unwrap()
}
fn ids(d: &crate::types::Document, nodes: Vec<u32>) -> Vec<String> {
    nodes
        .into_iter()
        .map(|n| d.get_attribute(n, "id").unwrap_or_default())
        .collect()
}

#[test]
fn rows_are_head_then_bodies_then_foot_whatever_the_source_order() {
    let d = table();
    let t = el(&d, "t");
    assert_eq!(
        ids(&d, d.table_rows(t)),
        vec!["h1", "r1", "r2", "r3", "f1"],
        "Chrome: rows=h1,r1,r2,r3,f1 — the tfoot is written first and collected last"
    );
    assert_eq!(ids(&d, d.t_bodies(t)), vec!["b1", "b2"]);
    assert!(d.t_head(t).is_some(), "Chrome: tHead=h1's section");
    assert_eq!(ids(&d, d.section_rows(d.t_head(t).unwrap())), vec!["h1"]);
    assert_eq!(ids(&d, d.section_rows(d.t_foot(t).unwrap())), vec!["f1"]);
    assert_eq!(d.caption(t), None, "Chrome: caption=null");
}

#[test]
fn row_index_counts_the_table_and_section_row_index_counts_the_section() {
    let d = table();
    // Chrome: h1 0/0, r1 1/0, r2 2/1, r3 3/0, f1 4/0
    for (id, row, section) in [
        ("h1", 0, 0),
        ("r1", 1, 0),
        ("r2", 2, 1),
        ("r3", 3, 0),
        ("f1", 4, 0),
    ] {
        let r = el(&d, id);
        assert_eq!(d.row_index(r), row, "{id} rowIndex");
        assert_eq!(d.section_row_index(r), section, "{id} sectionRowIndex");
    }
    assert_eq!(ids(&d, d.section_rows(el(&d, "b1"))), vec!["r1", "r2"]);
}

#[test]
fn cells_and_their_index_and_spans() {
    let d = table();
    let r1 = el(&d, "r1");
    assert_eq!(d.row_cells(r1).len(), 2, "Chrome: r1 cells=2");
    let first = d.row_cells(r1)[0];
    assert_eq!(d.cell_index(first), 0);
    assert_eq!(d.cell_index(d.row_cells(r1)[1]), 1);
    assert_eq!(d.col_span(first), 1, "Chrome: colSpan=1 with no attribute");
    assert_eq!(d.row_span(first), 1, "Chrome: rowSpan=1 with no attribute");

    let mut d2 = table();
    let cell = d2.row_cells(el(&d2, "r1"))[0];
    d2.set_col_span(cell, 3);
    assert_eq!(d2.col_span(cell), 3);
    // Clamped, per HTML §4.9.11 — `colspan=0` is not a thing.
    d2.set_attribute(cell, "colspan", "0");
    assert_eq!(d2.col_span(cell), 1);
    d2.set_attribute(cell, "colspan", "99999");
    assert_eq!(d2.col_span(cell), 1000);
}

#[test]
fn insert_row_lands_in_the_section_that_owns_the_reference_row() {
    let mut d = table();
    let t = el(&d, "t");

    // Chrome: insertRow() -> parent=TFOOT, rowIndex=5
    let appended = d.insert_row(t, -1).unwrap();
    assert_eq!(
        d.tag_name(d.parent_element(appended).unwrap()),
        Some("tfoot"),
        "an appended row joins whatever section holds the LAST row"
    );
    assert_eq!(d.row_index(appended), 5);

    // Chrome: insertRow(0) -> parent=THEAD, rowIndex=0
    let prepended = d.insert_row(t, 0).unwrap();
    assert_eq!(
        d.tag_name(d.parent_element(prepended).unwrap()),
        Some("thead")
    );
    assert_eq!(d.row_index(prepended), 0);
}

#[test]
fn insert_row_on_an_empty_table_makes_a_tbody_to_put_it_in() {
    let mut d = parse_html(r#"<table id="e"></table>"#);
    let t = el(&d, "e");
    let row = d.insert_row(t, -1).unwrap();
    assert_eq!(
        d.tag_name(d.parent_element(row).unwrap()),
        Some("tbody"),
        "Chrome: empty table insertRow -> parent=TBODY"
    );
    assert_eq!(
        d.t_bodies(t).len(),
        1,
        "Chrome: tbodies=1 — exactly one, not one per row"
    );

    let second = d.insert_row(t, -1).unwrap();
    assert_eq!(d.t_bodies(t).len(), 1);
    assert_eq!(d.table_rows(t), vec![row, second]);
}

#[test]
fn an_out_of_range_index_does_nothing_and_says_so() {
    // Chrome throws IndexSizeError for both. `None`/`false` is that error.
    let mut d = table();
    let t = el(&d, "t");
    let before = d.table_rows(t).len();
    assert_eq!(d.insert_row(t, 99), None);
    assert!(!d.delete_row(t, 99));
    assert_eq!(
        d.insert_row(t, -2),
        None,
        "−1 is the only negative index there is"
    );
    assert_eq!(
        d.table_rows(t).len(),
        before,
        "and nothing was inserted or removed"
    );
}

#[test]
fn create_caption_thead_and_tfoot_are_idempotent_but_create_tbody_is_not() {
    let mut d = table();
    let t = el(&d, "t");

    let caption = d.create_caption(t);
    assert_eq!(
        d.tag_name(d.first_element_child(t).unwrap()),
        Some("caption"),
        "Chrome: createCaption -> firstChild=CAPTION"
    );
    assert_eq!(d.create_caption(t), caption, "Chrome: same=true");

    assert_eq!(
        d.create_t_head(t),
        d.t_head(t).unwrap(),
        "Chrome: createTHead returns the existing one"
    );

    let bodies_before = d.t_bodies(t).len();
    let fresh = d.create_t_body(t);
    assert_eq!(
        d.t_bodies(t).len(),
        bodies_before + 1,
        "createTBody always makes a new one"
    );
    assert!(d.t_bodies(t).contains(&fresh));

    d.delete_t_foot(t);
    assert_eq!(d.t_foot(t), None, "Chrome: after deleteTFoot tFoot=null");
    d.delete_caption(t);
    assert_eq!(d.caption(t), None);
}

#[test]
fn create_thead_goes_before_the_sections_and_after_the_caption() {
    let mut d = parse_html(
        r#"<table id="t"><caption>c</caption><tbody><tr><td>x</td></tr></tbody></table>"#,
    );
    let t = el(&d, "t");
    let head = d.create_t_head(t);
    let kids: Vec<&str> = d
        .children(t)
        .into_iter()
        .filter_map(|c| d.tag_name(c))
        .collect();
    assert_eq!(
        kids,
        vec!["caption", "thead", "tbody"],
        "a thead belongs after the caption and before every section"
    );
    assert_eq!(d.t_head(t), Some(head));
}

#[test]
fn insert_cell_makes_a_td_and_delete_cell_takes_one_away() {
    let mut d = table();
    let r2 = el(&d, "r2");
    let cell = d.insert_cell(r2, -1).unwrap();
    assert_eq!(
        d.tag_name(cell),
        Some("td"),
        "Chrome: insertCell -> tag=TD, never TH"
    );
    assert_eq!(d.cell_index(cell), 1);

    let front = d.insert_cell(r2, 0).unwrap();
    assert_eq!(d.cell_index(front), 0);
    assert_eq!(d.row_cells(r2).len(), 3);

    assert!(d.delete_cell(r2, 0));
    assert_eq!(d.row_cells(r2).len(), 2);
    assert!(!d.delete_cell(r2, 7), "out of range says so");
}

#[test]
fn a_sections_own_insert_and_delete_work_on_that_section_only() {
    let mut d = table();
    let b1 = el(&d, "b1");
    let row = d.section_insert_row(b1, 0).unwrap();
    assert_eq!(d.parent_element(row), Some(b1));
    assert_eq!(d.section_row_index(row), 0);
    assert_eq!(ids(&d, d.section_rows(b1))[1..], ["r1", "r2"]);

    assert!(d.section_delete_row(b1, 0));
    assert_eq!(ids(&d, d.section_rows(b1)), vec!["r1", "r2"]);
    assert!(!d.section_insert_row(b1, 9).is_some());
}

#[test]
fn a_row_outside_a_table_has_no_index() {
    let mut d = parse_html("<div id=d></div>");
    let orphan = d.create_element("tr");
    assert_eq!(d.row_index(orphan), -1);
    assert_eq!(d.section_row_index(orphan), -1);
    let cell = d.create_element("td");
    assert_eq!(d.cell_index(cell), -1);
}

// ── Cell vertical alignment (CSS 2.1 §17.5.3) ───────────────────────────────

/// **A table cell centres its content by default.** Browsers get this from a UA
/// rule of `vertical-align: middle` on the row plus `vertical-align: inherit`
/// on the cell. `vertical-align` is NOT an inherited property, so setting it
/// only on `tr` never reached `td`/`th` and every cell in a row taller than its
/// own content sat flush to the top.
#[test]
fn a_table_cell_centres_its_content_by_default() {
    let doc = crate::tests::harness::parse_and_layout(
        "<style>* { margin:0; padding:0 } td { padding:0 }</style>\
         <table><tr>\
           <td id=short>x</td>\
           <td id=tall style='height:100px'>y</td>\
         </tr></table>",
        400.0,
    );
    fn by_id<'a>(n: &'a crate::types::WebCore, id: &str) -> Option<&'a crate::types::WebCore> {
        if n.attributes.get("id").map(String::as_str) == Some(id) {
            return Some(n);
        }
        for c in &n.children {
            if let Some(f) = by_id(c, id) {
                return Some(f);
            }
        }
        None
    }
    let short = by_id(&doc.root, "short").unwrap();
    // The cell is stretched to the row height; its single line of text must sit
    // in the middle of it, not at the top.
    let cell_h = short.layout.content_rect.h;
    assert!(
        cell_h > 50.0,
        "the row is tall, so the cell is too: {cell_h}"
    );
    // Measured against the PADDING box: the vertical-align offset moves
    // `content_rect.y` and the line cache together, so a content-relative
    // reading is zero by construction and would prove nothing.
    let line_y =
        short.layout.line_cache.first().map(|l| l.y).unwrap_or(0.0) - short.layout.padding_rect.y;
    assert!(
        line_y > 20.0,
        "the cell's line should be centred, not at the top — offset {line_y} in a {cell_h}px cell"
    );
}
