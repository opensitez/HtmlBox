use rhtmledit::{load_html};

#[test]
fn controls_new_game_hit() {
    // Load the Minesweeper example HTML at the same viewport width used by the example.
    let html = include_str!("../examples/html/minesweeper.html");
    let doc = load_html(html, 500.0);

    // Choose a point inside the New Game button area observed during manual runs.
    // New Game sits near x ~ 9..497, y ~ 513..549 in the 500px-wide layout.
    let pt = (20.0f32, 520.0f32);
    let hit = rhtmledit::layout::hit_test::point_to_hit(&doc.root, pt, 0);
    assert!(hit.is_some(), "expected a hit result at point {:?}", pt);
    let hit_box = unsafe { &*hit.unwrap().box_ptr };
    let id = hit_box.attributes.get("id");
    assert_eq!(id.map(|s| s.as_str()), Some("new-game"), "expected hit to be #new-game, got id={:?}", id);
}
