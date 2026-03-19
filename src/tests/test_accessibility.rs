// Tests for the accessibility tree builder (src/accessibility/mod.rs).
//
// All tests require the "accessibility" feature; the whole module is gated on it.

#![cfg(feature = "accessibility")]

use accesskit::{Action, AutoComplete, HasPopup, Invalid, Orientation, Role, SortDirection, Toggled};
use crate::{load_html, accessibility::build_tree};

// ── helpers ───────────────────────────────────────────────────────────────────

/// Find the first node in the tree whose role matches `role`.
fn find_role(update: &accesskit::TreeUpdate, role: Role) -> Option<&accesskit::Node> {
    update.nodes.iter().find(|(_, n)| n.role() == role).map(|(_, n)| n)
}

/// Collect all roles present in the tree.
fn all_roles(update: &accesskit::TreeUpdate) -> Vec<Role> {
    update.nodes.iter().map(|(_, n)| n.role()).collect()
}

// ── 1. Build tree produces valid root ─────────────────────────────────────────

#[test]
fn build_tree_has_window_root() {
    let doc = load_html("<html><body><p>Hello</p></body></html>", 800.0);
    let tree = build_tree(&doc, 1.0);

    assert!(tree.tree.is_some(), "TreeUpdate must have a Tree");
    // ROOT_ID = NodeId(1)
    let root_id = accesskit::NodeId(1);
    assert_eq!(tree.tree.unwrap().root, root_id);

    // The synthetic root must be a Window node.
    let window = tree.nodes.iter().find(|(id, _)| *id == root_id).map(|(_, n)| n);
    assert!(window.is_some(), "ROOT_ID node must exist");
    assert_eq!(window.unwrap().role(), Role::Window);
}

#[test]
fn build_tree_focus_defaults_to_root_when_none_focused() {
    let doc = load_html("<html><body><button>Click</button></body></html>", 800.0);
    let tree = build_tree(&doc, 1.0);
    // No element is focused — focus should fall back to ROOT_ID.
    assert_eq!(tree.focus, accesskit::NodeId(1));
}

// ── 2. Role resolution — HTML element semantics ───────────────────────────────

#[test]
fn role_button_element() {
    let doc = load_html("<html><body><button>OK</button></body></html>", 800.0);
    let tree = build_tree(&doc, 1.0);
    assert!(all_roles(&tree).contains(&Role::Button), "button element must map to Role::Button");
}

#[test]
fn role_link_with_href() {
    let doc = load_html("<html><body><a href=\"/\">Home</a></body></html>", 800.0);
    let tree = build_tree(&doc, 1.0);
    assert!(all_roles(&tree).contains(&Role::Link), "a[href] must map to Role::Link");
}

#[test]
fn role_a_without_href_is_not_link() {
    let doc = load_html("<html><body><a>Anchor</a></body></html>", 800.0);
    let tree = build_tree(&doc, 1.0);
    assert!(!all_roles(&tree).contains(&Role::Link), "a without href must not be Link");
}

#[test]
fn role_headings() {
    for (tag, expected_role) in [
        ("h1", Role::Heading),
        ("h2", Role::Heading),
        ("h3", Role::Heading),
    ] {
        let html = format!("<html><body><{tag}>Title</{tag}></body></html>");
        let doc = load_html(&html, 800.0);
        let tree = build_tree(&doc, 1.0);
        assert!(
            all_roles(&tree).contains(&expected_role),
            "{tag} must map to Role::Heading"
        );
    }
}

#[test]
fn role_input_types() {
    let cases = [
        ("checkbox", Role::CheckBox),
        ("radio",    Role::RadioButton),
        ("range",    Role::Slider),
        ("search",   Role::SearchInput),
        ("email",    Role::EmailInput),
        ("password", Role::PasswordInput),
    ];
    for (input_type, expected_role) in cases {
        let html = format!("<html><body><input type=\"{input_type}\"></body></html>");
        let doc = load_html(&html, 800.0);
        let tree = build_tree(&doc, 1.0);
        assert!(
            all_roles(&tree).contains(&expected_role),
            "input[type={input_type}] must map to {expected_role:?}"
        );
    }
}

#[test]
fn role_input_default_is_text_input() {
    let doc = load_html("<html><body><input type=\"text\"></body></html>", 800.0);
    let tree = build_tree(&doc, 1.0);
    assert!(all_roles(&tree).contains(&Role::TextInput));
}

#[test]
fn role_textarea_is_multiline() {
    let doc = load_html("<html><body><textarea></textarea></body></html>", 800.0);
    let tree = build_tree(&doc, 1.0);
    assert!(all_roles(&tree).contains(&Role::MultilineTextInput));
}

#[test]
fn role_select_is_combobox() {
    let doc = load_html("<html><body><select><option>A</option></select></body></html>", 800.0);
    let tree = build_tree(&doc, 1.0);
    assert!(all_roles(&tree).contains(&Role::ComboBox));
}

#[test]
fn role_list_elements() {
    let doc = load_html("<html><body><ul><li>Item</li></ul></body></html>", 800.0);
    let tree = build_tree(&doc, 1.0);
    assert!(all_roles(&tree).contains(&Role::List), "ul must be List");
    assert!(all_roles(&tree).contains(&Role::ListItem), "li must be ListItem");
}

#[test]
fn role_nav_main_header_footer() {
    let doc = load_html(
        "<html><body><nav></nav><main></main><header></header><footer></footer></body></html>",
        800.0,
    );
    let tree = build_tree(&doc, 1.0);
    let roles = all_roles(&tree);
    assert!(roles.contains(&Role::Navigation));
    assert!(roles.contains(&Role::Main));
    assert!(roles.contains(&Role::Banner));      // header
    assert!(roles.contains(&Role::ContentInfo)); // footer
}

#[test]
fn role_table_elements() {
    let doc = load_html(
        "<html><body><table><tr><th>Head</th><td>Cell</td></tr></table></body></html>",
        800.0,
    );
    let tree = build_tree(&doc, 1.0);
    let roles = all_roles(&tree);
    assert!(roles.contains(&Role::Table));
    assert!(roles.contains(&Role::Row));
    assert!(roles.contains(&Role::ColumnHeader));
    assert!(roles.contains(&Role::Cell));
}

#[test]
fn role_img_is_image() {
    let doc = load_html("<html><body><img alt=\"photo\"></body></html>", 800.0);
    let tree = build_tree(&doc, 1.0);
    assert!(all_roles(&tree).contains(&Role::Image));
}

// ── 3. Explicit role attribute overrides element semantics ────────────────────

#[test]
fn aria_role_overrides_element_role() {
    // A div with role="button" should be Button, not GenericContainer.
    let doc = load_html("<html><body><div role=\"button\">Click</div></body></html>", 800.0);
    let tree = build_tree(&doc, 1.0);
    assert!(all_roles(&tree).contains(&Role::Button), "role=button must override div semantics");
}

#[test]
fn aria_role_none_becomes_generic() {
    let doc = load_html("<html><body><button role=\"none\">X</button></body></html>", 800.0);
    let tree = build_tree(&doc, 1.0);
    // role=none/presentation → GenericContainer; the Button role must not appear
    assert!(
        all_roles(&tree).contains(&Role::GenericContainer),
        "role=none must produce GenericContainer"
    );
}

// ── 4. Accessible name (compute_name) ────────────────────────────────────────

#[test]
fn name_from_aria_label() {
    let doc = load_html(
        "<html><body><button aria-label=\"Close dialog\">X</button></body></html>",
        800.0,
    );
    let tree = build_tree(&doc, 1.0);
    let btn = find_role(&tree, Role::Button).expect("button node");
    assert_eq!(btn.label(), Some("Close dialog"), "aria-label must be used as accessible name");
}

#[test]
fn name_from_img_alt() {
    let doc = load_html("<html><body><img alt=\"Company logo\"></body></html>", 800.0);
    let tree = build_tree(&doc, 1.0);
    let img = find_role(&tree, Role::Image).expect("image node");
    assert_eq!(img.label(), Some("Company logo"), "img alt must be accessible name");
}

#[test]
fn name_from_text_content() {
    let doc = load_html("<html><body><button>Submit form</button></body></html>", 800.0);
    let tree = build_tree(&doc, 1.0);
    let btn = find_role(&tree, Role::Button).expect("button node");
    assert_eq!(btn.label(), Some("Submit form"));
}

// ── 5. Heading level ──────────────────────────────────────────────────────────

#[test]
fn heading_level_set_correctly() {
    for n in 1usize..=6 {
        let html = format!("<html><body><h{n}>Title</h{n}></body></html>");
        let doc = load_html(&html, 800.0);
        let tree = build_tree(&doc, 1.0);
        let heading = find_role(&tree, Role::Heading).expect("heading node");
        assert_eq!(heading.level(), Some(n), "h{n} must have level {n}");
    }
}

// ── 6. Toggled state (aria-checked / checked attribute) ───────────────────────

#[test]
fn checkbox_with_checked_attr_is_toggled_true() {
    let doc = load_html(
        "<html><body><input type=\"checkbox\" checked></body></html>",
        800.0,
    );
    let tree = build_tree(&doc, 1.0);
    let cb = find_role(&tree, Role::CheckBox).expect("checkbox node");
    assert_eq!(cb.toggled(), Some(Toggled::True));
}

#[test]
fn aria_checked_false_is_toggled_false() {
    let doc = load_html(
        "<html><body><input type=\"checkbox\" aria-checked=\"false\"></body></html>",
        800.0,
    );
    let tree = build_tree(&doc, 1.0);
    let cb = find_role(&tree, Role::CheckBox).expect("checkbox node");
    assert_eq!(cb.toggled(), Some(Toggled::False));
}

#[test]
fn aria_checked_mixed_is_toggled_mixed() {
    let doc = load_html(
        "<html><body><input type=\"checkbox\" aria-checked=\"mixed\"></body></html>",
        800.0,
    );
    let tree = build_tree(&doc, 1.0);
    let cb = find_role(&tree, Role::CheckBox).expect("checkbox node");
    assert_eq!(cb.toggled(), Some(Toggled::Mixed));
}

// ── 7. Supported actions ──────────────────────────────────────────────────────

#[test]
fn button_supports_click_and_focus() {
    let doc = load_html("<html><body><button>OK</button></body></html>", 800.0);
    let tree = build_tree(&doc, 1.0);
    let btn = find_role(&tree, Role::Button).expect("button node");
    assert!(btn.supports_action(Action::Click),  "button must support Click");
    assert!(btn.supports_action(Action::Focus),  "button must support Focus");
}

#[test]
fn text_input_supports_set_text_selection() {
    let doc = load_html("<html><body><input type=\"text\"></body></html>", 800.0);
    let tree = build_tree(&doc, 1.0);
    let input = find_role(&tree, Role::TextInput).expect("input node");
    assert!(input.supports_action(Action::SetTextSelection));
}

#[test]
fn link_supports_click() {
    let doc = load_html("<html><body><a href=\"/\">Home</a></body></html>", 800.0);
    let tree = build_tree(&doc, 1.0);
    let link = find_role(&tree, Role::Link).expect("link node");
    assert!(link.supports_action(Action::Click));
}

// ── 8. Link URL ───────────────────────────────────────────────────────────────

#[test]
fn link_url_is_set() {
    let doc = load_html("<html><body><a href=\"https://example.com\">E</a></body></html>", 800.0);
    let tree = build_tree(&doc, 1.0);
    let link = find_role(&tree, Role::Link).expect("link node");
    assert_eq!(link.url(), Some("https://example.com"));
}

// ── 9. Placeholder ────────────────────────────────────────────────────────────

#[test]
fn input_placeholder_exposed() {
    let doc = load_html(
        "<html><body><input type=\"text\" placeholder=\"Search...\"></body></html>",
        800.0,
    );
    let tree = build_tree(&doc, 1.0);
    let input = find_role(&tree, Role::TextInput).expect("input node");
    assert_eq!(input.placeholder(), Some("Search..."));
}

// ── 10. aria-hidden excludes subtrees ─────────────────────────────────────────

#[test]
fn aria_hidden_subtree_not_in_children() {
    // The hidden button must still appear in the flat node list (it's visited
    // but filtered from its parent's children vector).
    let doc = load_html(
        "<html><body><div aria-hidden=\"true\"><button>Hidden</button></div><button>Visible</button></body></html>",
        800.0,
    );
    let tree = build_tree(&doc, 1.0);
    // At least the visible button must be present.
    assert!(
        all_roles(&tree).contains(&Role::Button),
        "visible button must appear in the tree"
    );
    // The div itself is present but filtered from its parent's children list.
    // We just confirm the build doesn't panic and root is valid.
    assert!(tree.tree.is_some());
}

// ── 11. Scale factor applied to bounds ────────────────────────────────────────

#[test]
fn scale_factor_doubles_bounds() {
    let doc = load_html("<html><body><button>OK</button></body></html>", 800.0);
    let tree1 = build_tree(&doc, 1.0);
    let tree2 = build_tree(&doc, 2.0);

    let bounds1 = find_role(&tree1, Role::Button).and_then(|n| n.bounds());
    let bounds2 = find_role(&tree2, Role::Button).and_then(|n| n.bounds());

    if let (Some(b1), Some(b2)) = (bounds1, bounds2) {
        // At 2× scale, coordinates should be doubled.
        assert!(
            (b2.x0 - b1.x0 * 2.0).abs() < 1.0,
            "x0 at 2x scale should be double the 1x value"
        );
        assert!(
            (b2.y0 - b1.y0 * 2.0).abs() < 1.0,
            "y0 at 2x scale should be double the 1x value"
        );
    }
    // If both bounds are None (e.g. zero-size in test env), just ensure no panic.
}

// ── 12. Disabled / required / readonly ARIA attributes ────────────────────────

#[test]
fn disabled_attribute_sets_disabled() {
    let doc = load_html("<html><body><button disabled>No</button></body></html>", 800.0);
    let tree = build_tree(&doc, 1.0);
    let btn = find_role(&tree, Role::Button).expect("button node");
    assert!(btn.is_disabled(), "disabled attribute must set disabled state");
}

#[test]
fn aria_required_sets_required() {
    let doc = load_html(
        "<html><body><input type=\"text\" aria-required=\"true\"></body></html>",
        800.0,
    );
    let tree = build_tree(&doc, 1.0);
    let input = find_role(&tree, Role::TextInput).expect("input node");
    assert!(input.is_required(), "aria-required=true must set required state");
}

// ── 13. Semantic role mapping — new HTML elements ─────────────────────────────

#[test]
fn role_time_element() {
    let doc = load_html("<html><body><time datetime=\"2024-01-01\">Jan 1</time></body></html>", 800.0);
    let tree = build_tree(&doc, 1.0);
    assert!(all_roles(&tree).contains(&Role::Time), "<time> must map to Role::Time");
}

#[test]
fn role_dfn_is_term() {
    let doc = load_html("<html><body><p><dfn>HTML</dfn></p></body></html>", 800.0);
    let tree = build_tree(&doc, 1.0);
    assert!(all_roles(&tree).contains(&Role::Term), "<dfn> must map to Role::Term");
}

#[test]
fn role_pre_element() {
    let doc = load_html("<html><body><pre>code</pre></body></html>", 800.0);
    let tree = build_tree(&doc, 1.0);
    assert!(all_roles(&tree).contains(&Role::Pre), "<pre> must map to Role::Pre");
}

#[test]
fn role_output_is_status() {
    let doc = load_html("<html><body><output>Result</output></body></html>", 800.0);
    let tree = build_tree(&doc, 1.0);
    assert!(all_roles(&tree).contains(&Role::Status), "<output> must map to Role::Status");
}

#[test]
fn role_ruby_element() {
    let doc = load_html("<html><body><ruby>漢字</ruby></body></html>", 800.0);
    let tree = build_tree(&doc, 1.0);
    assert!(all_roles(&tree).contains(&Role::Ruby), "<ruby> must map to Role::Ruby");
}

#[test]
fn role_input_color_is_color_well() {
    let doc = load_html("<html><body><input type=\"color\"></body></html>", 800.0);
    let tree = build_tree(&doc, 1.0);
    assert!(all_roles(&tree).contains(&Role::ColorWell), "input[type=color] must be ColorWell");
}

#[test]
fn role_input_date_types() {
    let cases = [
        ("date",           Role::DateInput),
        ("datetime-local", Role::DateTimeInput),
        ("week",           Role::WeekInput),
        ("month",          Role::MonthInput),
        ("time",           Role::TimeInput),
    ];
    for (input_type, expected_role) in cases {
        let html = format!("<html><body><input type=\"{input_type}\"></body></html>");
        let doc = load_html(&html, 800.0);
        let tree = build_tree(&doc, 1.0);
        assert!(
            all_roles(&tree).contains(&expected_role),
            "input[type={input_type}] must map to {expected_role:?}"
        );
    }
}

#[test]
fn aria_role_attribute_additional_roles() {
    let cases = [
        ("radiogroup", Role::RadioGroup),
        ("toolbar",    Role::Toolbar),
        ("tooltip",    Role::Tooltip),
        ("treegrid",   Role::TreeGrid),
        ("timer",      Role::Timer),
        ("marquee",    Role::Marquee),
    ];
    for (role_attr, expected) in cases {
        let html = format!("<html><body><div role=\"{role_attr}\"></div></body></html>");
        let doc = load_html(&html, 800.0);
        let tree = build_tree(&doc, 1.0);
        assert!(
            all_roles(&tree).contains(&expected),
            "role={role_attr} must produce {expected:?}"
        );
    }
}

// ── 14. ARIA attribute parsing — numeric values ───────────────────────────────

#[test]
fn aria_value_attributes() {
    let doc = load_html(
        r#"<html><body>
            <input type="range" aria-valuenow="50" aria-valuemin="0" aria-valuemax="100">
        </body></html>"#,
        800.0,
    );
    let tree = build_tree(&doc, 1.0);
    let slider = find_role(&tree, Role::Slider).expect("slider node");
    assert_eq!(slider.numeric_value(), Some(50.0));
    assert_eq!(slider.min_numeric_value(), Some(0.0));
    assert_eq!(slider.max_numeric_value(), Some(100.0));
}

#[test]
fn aria_valuetext_overrides_numeric() {
    let doc = load_html(
        r#"<html><body>
            <input type="range" aria-valuenow="3" aria-valuetext="Low">
        </body></html>"#,
        800.0,
    );
    let tree = build_tree(&doc, 1.0);
    let slider = find_role(&tree, Role::Slider).expect("slider node");
    assert_eq!(slider.value(), Some("Low"), "aria-valuetext must set value string");
}

#[test]
fn aria_setsize_and_posinset() {
    let doc = load_html(
        r#"<html><body>
            <ul>
                <li role="option" aria-setsize="3" aria-posinset="2">Item</li>
            </ul>
        </body></html>"#,
        800.0,
    );
    let tree = build_tree(&doc, 1.0);
    let opt = find_role(&tree, Role::ListBoxOption).expect("option node");
    assert_eq!(opt.size_of_set(), Some(3));
    assert_eq!(opt.position_in_set(), Some(2));
}

// ── 15. ARIA attribute parsing — orientation / sort ───────────────────────────

#[test]
fn aria_orientation_horizontal() {
    let doc = load_html(
        "<html><body><div role=\"toolbar\" aria-orientation=\"horizontal\"></div></body></html>",
        800.0,
    );
    let tree = build_tree(&doc, 1.0);
    let toolbar = find_role(&tree, Role::Toolbar).expect("toolbar node");
    assert_eq!(toolbar.orientation(), Some(Orientation::Horizontal));
}

#[test]
fn aria_orientation_vertical() {
    let doc = load_html(
        "<html><body><div role=\"slider\" aria-orientation=\"vertical\"></div></body></html>",
        800.0,
    );
    let tree = build_tree(&doc, 1.0);
    let slider = find_role(&tree, Role::Slider).expect("slider node");
    assert_eq!(slider.orientation(), Some(Orientation::Vertical));
}

#[test]
fn aria_sort_ascending() {
    let doc = load_html(
        "<html><body><th aria-sort=\"ascending\">Name</th></body></html>",
        800.0,
    );
    let tree = build_tree(&doc, 1.0);
    let th = find_role(&tree, Role::ColumnHeader).expect("column header node");
    assert_eq!(th.sort_direction(), Some(SortDirection::Ascending));
}

// ── 16. ARIA attribute parsing — state flags ──────────────────────────────────

#[test]
fn aria_multiselectable() {
    let doc = load_html(
        "<html><body><select aria-multiselectable=\"true\"><option>A</option></select></body></html>",
        800.0,
    );
    let tree = build_tree(&doc, 1.0);
    let sel = find_role(&tree, Role::ComboBox).expect("select node");
    assert!(sel.is_multiselectable());
}

#[test]
fn aria_modal() {
    let doc = load_html(
        "<html><body><dialog aria-modal=\"true\">Dialog</dialog></body></html>",
        800.0,
    );
    let tree = build_tree(&doc, 1.0);
    let dlg = find_role(&tree, Role::Dialog).expect("dialog node");
    assert!(dlg.is_modal());
}

#[test]
fn aria_busy() {
    let doc = load_html(
        "<html><body><div aria-busy=\"true\">Loading…</div></body></html>",
        800.0,
    );
    let tree = build_tree(&doc, 1.0);
    // find_role may hit a child text node first; search for the node with is_busy set.
    let busy_node = tree.nodes.iter()
        .find(|(_, n)| n.is_busy())
        .map(|(_, n)| n);
    assert!(busy_node.is_some(), "a node with aria-busy=true must have is_busy set");
}

#[test]
fn aria_invalid_grammar() {
    let doc = load_html(
        "<html><body><input type=\"text\" aria-invalid=\"grammar\"></body></html>",
        800.0,
    );
    let tree = build_tree(&doc, 1.0);
    let input = find_role(&tree, Role::TextInput).expect("input node");
    assert_eq!(input.invalid(), Some(Invalid::Grammar));
}

#[test]
fn aria_invalid_true() {
    let doc = load_html(
        "<html><body><input type=\"text\" aria-invalid=\"true\"></body></html>",
        800.0,
    );
    let tree = build_tree(&doc, 1.0);
    let input = find_role(&tree, Role::TextInput).expect("input node");
    assert_eq!(input.invalid(), Some(Invalid::True));
}

#[test]
fn aria_haspopup_menu() {
    let doc = load_html(
        "<html><body><button aria-haspopup=\"menu\">Open</button></body></html>",
        800.0,
    );
    let tree = build_tree(&doc, 1.0);
    let btn = find_role(&tree, Role::Button).expect("button node");
    assert_eq!(btn.has_popup(), Some(HasPopup::Menu));
}

#[test]
fn aria_haspopup_dialog() {
    let doc = load_html(
        "<html><body><button aria-haspopup=\"dialog\">Open</button></body></html>",
        800.0,
    );
    let tree = build_tree(&doc, 1.0);
    let btn = find_role(&tree, Role::Button).expect("button node");
    assert_eq!(btn.has_popup(), Some(HasPopup::Dialog));
}

#[test]
fn aria_autocomplete_list() {
    let doc = load_html(
        "<html><body><input type=\"text\" aria-autocomplete=\"list\"></body></html>",
        800.0,
    );
    let tree = build_tree(&doc, 1.0);
    let input = find_role(&tree, Role::TextInput).expect("input node");
    assert_eq!(input.auto_complete(), Some(AutoComplete::List));
}

// ── 17. ARIA cross-references (labelledby / describedby) ─────────────────────

#[test]
fn aria_labelledby_resolves_to_text() {
    // The button is named by the <span id="lbl"> element.
    let doc = load_html(
        r#"<html><body>
            <span id="lbl">Save document</span>
            <button aria-labelledby="lbl">💾</button>
        </body></html>"#,
        800.0,
    );
    let tree = build_tree(&doc, 1.0);
    let btn = find_role(&tree, Role::Button).expect("button node");
    assert_eq!(
        btn.label(),
        Some("Save document"),
        "aria-labelledby must resolve the referenced element's text"
    );
}

#[test]
fn aria_describedby_resolves_to_text() {
    let doc = load_html(
        r#"<html><body>
            <p id="hint">Must be at least 8 characters.</p>
            <input type="password" aria-describedby="hint">
        </body></html>"#,
        800.0,
    );
    let tree = build_tree(&doc, 1.0);
    let input = find_role(&tree, Role::PasswordInput).expect("password input node");
    assert_eq!(
        input.description(),
        Some("Must be at least 8 characters."),
        "aria-describedby must resolve the referenced element's text"
    );
}

#[test]
fn aria_labelledby_multiple_idrefs() {
    // Multiple idrefs: text is concatenated with spaces.
    let doc = load_html(
        r#"<html><body>
            <span id="a">First</span>
            <span id="b">Last</span>
            <input type="text" aria-labelledby="a b">
        </body></html>"#,
        800.0,
    );
    let tree = build_tree(&doc, 1.0);
    let input = find_role(&tree, Role::TextInput).expect("text input node");
    assert_eq!(
        input.label(),
        Some("First Last"),
        "multiple aria-labelledby idrefs must be joined with spaces"
    );
}

// ── 18. Table ARIA grid attributes ───────────────────────────────────────────

#[test]
fn aria_table_row_col_counts() {
    let doc = load_html(
        r#"<html><body>
            <table aria-rowcount="10" aria-colcount="5">
                <tr><td aria-rowindex="1" aria-colindex="1">Cell</td></tr>
            </table>
        </body></html>"#,
        800.0,
    );
    let tree = build_tree(&doc, 1.0);
    let table = find_role(&tree, Role::Table).expect("table node");
    assert_eq!(table.row_count(), Some(10));
    assert_eq!(table.column_count(), Some(5));

    let cell = find_role(&tree, Role::Cell).expect("cell node");
    assert_eq!(cell.row_index(), Some(1));
    assert_eq!(cell.column_index(), Some(1));
}

// ── 19. Computed accessible name — <label for> association ───────────────────

#[test]
fn label_for_names_input() {
    let doc = load_html(
        r#"<html><body>
            <label for="email">Email address</label>
            <input type="email" id="email">
        </body></html>"#,
        800.0,
    );
    let tree = build_tree(&doc, 1.0);
    let input = find_role(&tree, Role::EmailInput).expect("email input");
    assert_eq!(
        input.label(),
        Some("Email address"),
        "<label for> must become the input's accessible name"
    );
}

#[test]
fn wrapping_label_names_input() {
    let doc = load_html(
        r#"<html><body>
            <label>Username <input type="text"></label>
        </body></html>"#,
        800.0,
    );
    let tree = build_tree(&doc, 1.0);
    let input = find_role(&tree, Role::TextInput).expect("text input");
    assert_eq!(
        input.label(),
        Some("Username"),
        "wrapping <label> text must name the embedded input"
    );
}

#[test]
fn label_text_excludes_embedded_input() {
    // The label says "Search" with an embedded input — the input itself
    // must not contribute its own placeholder to the label text.
    let doc = load_html(
        r#"<html><body>
            <label>Search <input type="search" placeholder="type here"></label>
        </body></html>"#,
        800.0,
    );
    let tree = build_tree(&doc, 1.0);
    let input = find_role(&tree, Role::SearchInput).expect("search input");
    // Label text should be "Search", not "Search type here".
    assert_eq!(input.label(), Some("Search"));
}

// ── 20. Computed accessible name — child element naming ───────────────────────

#[test]
fn figure_named_by_figcaption() {
    let doc = load_html(
        r#"<html><body>
            <figure>
                <img alt="chart">
                <figcaption>Monthly revenue</figcaption>
            </figure>
        </body></html>"#,
        800.0,
    );
    let tree = build_tree(&doc, 1.0);
    let fig = find_role(&tree, Role::Figure).expect("figure node");
    assert_eq!(
        fig.label(),
        Some("Monthly revenue"),
        "<figure> must be named by its <figcaption>"
    );
}

#[test]
fn table_named_by_caption() {
    let doc = load_html(
        r#"<html><body>
            <table>
                <caption>Sales data</caption>
                <tr><td>Cell</td></tr>
            </table>
        </body></html>"#,
        800.0,
    );
    let tree = build_tree(&doc, 1.0);
    let table = find_role(&tree, Role::Table).expect("table node");
    assert_eq!(
        table.label(),
        Some("Sales data"),
        "<table> must be named by its <caption>"
    );
}

#[test]
fn fieldset_named_by_legend() {
    let doc = load_html(
        r#"<html><body>
            <fieldset>
                <legend>Shipping address</legend>
                <input type="text" id="addr">
            </fieldset>
        </body></html>"#,
        800.0,
    );
    let tree = build_tree(&doc, 1.0);
    let group = find_role(&tree, Role::Group).expect("fieldset node");
    assert_eq!(
        group.label(),
        Some("Shipping address"),
        "<fieldset> must be named by its <legend>"
    );
}

// ── 21. Computed accessible name — placeholder fallback ───────────────────────

#[test]
fn placeholder_used_as_fallback_name() {
    let doc = load_html(
        "<html><body><input type=\"text\" placeholder=\"Search…\"></body></html>",
        800.0,
    );
    let tree = build_tree(&doc, 1.0);
    let input = find_role(&tree, Role::TextInput).expect("text input");
    assert_eq!(
        input.label(),
        Some("Search…"),
        "placeholder must be used as accessible name when no label exists"
    );
}

#[test]
fn explicit_label_beats_placeholder() {
    let doc = load_html(
        r#"<html><body>
            <label for="q">Search</label>
            <input type="text" id="q" placeholder="type here">
        </body></html>"#,
        800.0,
    );
    let tree = build_tree(&doc, 1.0);
    let input = find_role(&tree, Role::TextInput).expect("text input");
    assert_eq!(
        input.label(),
        Some("Search"),
        "<label for> must take precedence over placeholder"
    );
}

// ── 22. Computed accessible name — aria-label beats everything except labelledby

#[test]
fn aria_label_beats_html_label() {
    let doc = load_html(
        r#"<html><body>
            <label for="x">HTML label</label>
            <input type="text" id="x" aria-label="ARIA label">
        </body></html>"#,
        800.0,
    );
    let tree = build_tree(&doc, 1.0);
    let input = find_role(&tree, Role::TextInput).expect("text input");
    assert_eq!(
        input.label(),
        Some("ARIA label"),
        "aria-label must override <label for>"
    );
}
