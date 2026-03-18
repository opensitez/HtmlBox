use rhtmledit::{load_html};
use rhtmledit::dom::query_selector;

#[test]
fn controls_new_game_hit() {
    // Load the Minesweeper example HTML at the same viewport width used by the example.
    let html = include_str!("../examples/html/minesweeper.html");
    let doc = load_html(html, 500.0);

    // Find the #new-game button in the layout tree and use its actual center for the hit test.
    let new_game = query_selector(&doc.root, "#new-game")
        .expect("#new-game button not found in layout");
    let r = new_game.border_rect;
    let cx = r.x + r.w / 2.0;
    let cy = r.y + r.h / 2.0;

    let pt = (cx, cy);
    let hit = rhtmledit::layout::hit_test::point_to_hit(&doc.root, pt, 0);
    assert!(hit.is_some(), "expected a hit result at the button center {:?}", pt);
    let hit_box = unsafe { &*hit.unwrap().box_ptr };
    let id = hit_box.attributes.get("id");
    assert_eq!(id.map(|s| s.as_str()), Some("new-game"),
               "expected hit to be #new-game, got id={:?}", id);
}
