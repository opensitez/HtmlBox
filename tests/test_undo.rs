// Ported from cpptests/test_undo.cpp
// Tests for UndoStack and document state restoration.
// NOTE: All C++ undo tests use TestWidget (wxHtmlEditWidget) which is not portable.
// Only the pure undo-stack tests are ported here.

use rhtmledit::dom::*;
use rhtmledit::types::*;
use rhtmledit::parse_html;

#[test]
fn undo_push_and_restore() {
    let mut stack = UndoStack::new();
    let doc1 = parse_html("<p>One</p>");
    let doc2 = parse_html("<p>Two</p>");
    
    // Snapshot state before changing to doc2
    stack.push(doc1.clone(), 0, 0, 0);
    
    assert!(stack.can_undo());
    assert!(!stack.can_redo());
    
    // Perform undo
    let entry = stack.undo(doc2.clone(), 0, 0, 0).unwrap();
    assert_eq!(entry.doc.root.text_content(), "One");
    
    assert!(!stack.can_undo());
    assert!(stack.can_redo());
    
    // Perform redo
    let entry = stack.redo(doc1, 0, 0, 0).unwrap();
    assert_eq!(entry.doc.root.text_content(), "Two");
}

#[test]
fn undo_multiple_levels() {
    let mut stack = UndoStack::new();
    let d0 = parse_html("<p>0</p>");
    let d1 = parse_html("<p>1</p>");
    let d2 = parse_html("<p>2</p>");
    
    stack.push(d0.clone(), 0, 0, 0);
    stack.push(d1.clone(), 0, 0, 0);
    
    // current is d2
    let entry1 = stack.undo(d2.clone(), 0, 0, 0).unwrap();
    assert_eq!(entry1.doc.root.text_content(), "1");
    
    let entry0 = stack.undo(entry1.doc, 0, 0, 0).unwrap();
    assert_eq!(entry0.doc.root.text_content(), "0");
}

#[test]
fn undo_redo_stack_cleared_on_push() {
    let mut stack = UndoStack::new();
    let d0 = parse_html("<p>0</p>");
    let d1 = parse_html("<p>1</p>");
    let d2 = parse_html("<p>2</p>");
    
    stack.push(d0.clone(), 0, 0, 0);
    stack.undo(d1.clone(), 0, 0, 0);
    assert!(stack.can_redo());
    
    // Pushing a new state clears redo
    stack.push(d1.clone(), 0, 0, 0);
    assert!(!stack.can_redo());
}

#[test]
fn undo_limit_respected() {
    let mut stack = UndoStack::new();
    let doc = parse_html("<p>X</p>");
    
    // Push more than 500 entries (the limit in dom/mod.rs)
    for i in 0..600 {
        stack.push(doc.clone(), i, 0, 0);
    }
    
    // Should only have 500 entries in undo stack
    // We can't check length directly because fields are private,
    // but we can try to undo 500 times.
    for _ in 0..500 {
        assert!(stack.can_undo());
        stack.undo(doc.clone(), 0, 0, 0);
    }
    assert!(!stack.can_undo());
}

#[test]
fn undo_noop_on_empty() {
    let mut stack = UndoStack::new();
    let doc = parse_html("<p>X</p>");
    assert!(stack.undo(doc.clone(), 0, 0, 0).is_none());
    assert!(stack.redo(doc, 0, 0, 0).is_none());
}

// ============================================================
// Additional undo stack tests (C++ test_undo.cpp — stack behavior)
// ============================================================

#[test]
fn undo_caret_position_restored() {
    let mut stack = UndoStack::new();
    let doc = parse_html("<p>Hello</p>");

    // Push with specific caret position
    stack.push(doc.clone(), 5, 0, 5);

    let entry = stack.undo(doc.clone(), 10, 0, 10).unwrap();
    assert_eq!(entry.caret_pos, 5);
    assert_eq!(entry.sel_end, 5);
}

#[test]
fn undo_selection_positions_restored() {
    let mut stack = UndoStack::new();
    let doc = parse_html("<p>Hello World</p>");

    stack.push(doc.clone(), 0, 3, 8);

    let entry = stack.undo(doc.clone(), 0, 0, 0).unwrap();
    assert_eq!(entry.sel_start, 3);
    assert_eq!(entry.sel_end, 8);
}

#[test]
fn undo_redo_preserves_document_content() {
    let mut stack = UndoStack::new();
    let d_before = parse_html("<p>Before</p>");
    let d_after  = parse_html("<p>After</p>");

    stack.push(d_before.clone(), 0, 0, 0);
    // Perform undo: save current (After) state, restore Before
    let entry = stack.undo(d_after.clone(), 0, 0, 0).unwrap();
    assert_eq!(entry.doc.root.text_content(), "Before");

    // Perform redo: save current (Before) state, restore After
    let entry2 = stack.redo(d_before, 0, 0, 0).unwrap();
    assert_eq!(entry2.doc.root.text_content(), "After");
}

#[test]
fn undo_multiple_redo_steps() {
    let mut stack = UndoStack::new();
    let d0 = parse_html("<p>0</p>");
    let d1 = parse_html("<p>1</p>");
    let d2 = parse_html("<p>2</p>");

    stack.push(d0.clone(), 0, 0, 0);
    stack.push(d1.clone(), 0, 0, 0);

    // Undo twice
    stack.undo(d2.clone(), 0, 0, 0);
    stack.undo(d1, 0, 0, 0);

    // Should be able to redo twice
    assert!(stack.can_redo());
    stack.redo(d0.clone(), 0, 0, 0);
    assert!(stack.can_redo());
    stack.redo(d0, 0, 0, 0);
    assert!(!stack.can_redo());
}

#[test]
fn undo_new_push_clears_redo_and_sets_can_undo() {
    let mut stack = UndoStack::new();
    let doc = parse_html("<p>X</p>");

    // Push then undo to populate redo
    stack.push(doc.clone(), 0, 0, 0);
    stack.undo(doc.clone(), 0, 0, 0);
    assert!(stack.can_redo());

    // A new push should clear redo
    stack.push(doc.clone(), 0, 0, 0);
    assert!(!stack.can_redo());
    assert!(stack.can_undo());
}
