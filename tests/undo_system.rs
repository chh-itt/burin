//! Integration tests for the undo/redo system (UndoableSignal, MergePolicy,
//! ElementUndoState).

use std::time::Duration;

use auralis_signal::Signal;
use burin::core::undo::{self, MergePolicy, UndoConfig, UndoableSignal};

// ── Basic undo / redo ────────────────────────────────────────────

#[test]
fn set_records_history() {
    let s = Signal::new("hello".to_string());
    let us = UndoableSignal::new(s, UndoConfig::default());

    assert_eq!(us.read(), "hello");
    assert!(!us.undo()); // nothing to undo

    us.set("world".to_string());
    assert_eq!(us.read(), "world");

    assert!(us.undo());
    assert_eq!(us.read(), "hello");

    assert!(!us.undo()); // back at start
}

#[test]
fn redo_restores_value() {
    let s = Signal::new("a".to_string());
    let us = UndoableSignal::new(
        s,
        UndoConfig {
            merge_policy: MergePolicy::None,
            ..UndoConfig::default()
        },
    );

    us.set("b".to_string());
    us.set("c".to_string());

    assert_eq!(us.read(), "c");
    assert!(us.undo());
    assert_eq!(us.read(), "b");
    assert!(us.undo());
    assert_eq!(us.read(), "a");

    assert!(us.redo());
    assert_eq!(us.read(), "b");
    assert!(us.redo());
    assert_eq!(us.read(), "c");
    assert!(!us.redo()); // at end
}

#[test]
fn new_mutation_discards_redo() {
    let s = Signal::new("a".to_string());
    let us = UndoableSignal::new(
        s,
        UndoConfig {
            merge_policy: MergePolicy::None,
            ..UndoConfig::default()
        },
    );

    us.set("b".to_string());
    us.set("c".to_string());

    us.undo(); // back to "b"
    us.set("d".to_string()); // new branch — discards "c"

    assert_eq!(us.read(), "d");

    // Undo should go "d" → "b" → "a" (not "c")
    assert!(us.undo());
    assert_eq!(us.read(), "b");
    assert!(us.undo());
    assert_eq!(us.read(), "a");
    assert!(!us.undo());
}

#[test]
fn no_op_when_value_unchanged() {
    let s = Signal::new("same".to_string());
    let us = UndoableSignal::new(s, UndoConfig::default());

    us.set("same".to_string()); // no actual change, no recording
    assert_eq!(us.read(), "same");
    assert!(!us.undo()); // nothing recorded
}

// ── MergePolicy::None ────────────────────────────────────────────

#[test]
fn no_merge_every_set_is_distinct() {
    let s = Signal::new(String::new());
    let us = UndoableSignal::new(
        s,
        UndoConfig {
            merge_policy: MergePolicy::None,
            ..UndoConfig::default()
        },
    );

    us.set("a".to_string());
    us.set("b".to_string());
    us.set("c".to_string());

    // 3 distinct undo steps
    assert!(us.undo());
    assert_eq!(us.read(), "b");
    assert!(us.undo());
    assert_eq!(us.read(), "a");
    assert!(us.undo());
    assert_eq!(us.read(), "");
    assert!(!us.undo());
}

// ── MergePolicy::TimeWindow ──────────────────────────────────────

#[test]
fn time_window_merges_rapid_sets() {
    let s = Signal::new(String::new());
    // Use a very large window so all sets within these tests merge
    let us = UndoableSignal::new(
        s,
        UndoConfig {
            merge_policy: MergePolicy::TimeWindow(Duration::from_secs(60)),
            ..UndoConfig::default()
        },
    );

    us.set("a".to_string());
    us.set("ab".to_string());
    us.set("abc".to_string());

    // All three sets should be merged into one undo step
    assert!(us.undo());
    assert_eq!(us.read(), "");
    assert!(!us.undo());
}

// ── MergePolicy::TextInput ───────────────────────────────────────

#[test]
fn text_input_merge_behavior() {
    let s = Signal::new(String::new());
    let us = UndoableSignal::new(
        s,
        UndoConfig {
            merge_policy: MergePolicy::TextInput(Default::default()),
            ..UndoConfig::default()
        },
    );

    // Typing burst: all within the merge window
    us.set("h".to_string());
    us.set("he".to_string());
    us.set("hel".to_string());

    // One undo should go straight back to ""
    assert!(us.undo());
    assert_eq!(us.read(), "");
    assert!(!us.undo());

    // Redo forward
    assert!(us.redo());
    assert_eq!(us.read(), "hel");
}

#[test]
fn text_input_push_boundary_creates_separate_step() {
    let s = Signal::new(String::new());
    let us = UndoableSignal::new(
        s,
        UndoConfig {
            merge_policy: MergePolicy::TextInput(Default::default()),
            ..UndoConfig::default()
        },
    );

    us.set("hello".to_string());
    us.push_boundary();
    us.set("hello ".to_string());
    us.set("hello world".to_string());

    // First undo → back to "hello" (the boundary)
    assert!(us.undo());
    assert_eq!(us.read(), "hello");

    // Second undo → back to ""
    assert!(us.undo());
    assert_eq!(us.read(), "");
}

// ── Max depth ────────────────────────────────────────────────────

#[test]
fn max_depth_discards_oldest() {
    let s = Signal::new(0);
    let us = UndoableSignal::new(
        s,
        UndoConfig {
            max_depth: 3,
            merge_policy: MergePolicy::None,
        },
    );

    us.set(1);
    us.set(2);
    us.set(3);
    us.set(4); // cap at 3 past + 1 current = 4 entries

    // 0 (dropped), 1, 2, 3, 4 (current)
    // Undo: 4 → 3 → 2 → 1, then stop (0 was evicted)
    assert!(us.undo());
    assert_eq!(us.read(), 3);
    assert!(us.undo());
    assert_eq!(us.read(), 2);
    assert!(us.undo());
    assert_eq!(us.read(), 1);
    assert!(!us.undo()); // 0 was dropped
}

// ── Clone shares history ─────────────────────────────────────────

#[test]
fn clones_share_undo_history() {
    let s = Signal::new("initial".to_string());
    let us1 = UndoableSignal::new(s, UndoConfig::default());
    let us2 = us1.clone();

    us1.set("modified".to_string());
    assert_eq!(us2.read(), "modified");

    us2.undo();
    assert_eq!(us1.read(), "initial");
}

// ── Different types ──────────────────────────────────────────────

#[test]
fn works_with_integers() {
    let s = Signal::new(42);
    let us = UndoableSignal::new(
        s,
        UndoConfig {
            merge_policy: MergePolicy::None,
            ..UndoConfig::default()
        },
    );

    us.set(100);
    us.set(200);

    assert!(us.undo());
    assert_eq!(us.read(), 100);
    assert!(us.undo());
    assert_eq!(us.read(), 42);
}

#[test]
fn works_with_floats() {
    let s = Signal::new(1.0f64);
    let us = UndoableSignal::new(
        s,
        UndoConfig {
            merge_policy: MergePolicy::None,
            ..UndoConfig::default()
        },
    );

    us.set(2.5);
    us.set(3.7);

    assert!(us.undo());
    assert!((us.read() - 2.5).abs() < 1e-10);
    assert!(us.undo());
    assert!((us.read() - 1.0).abs() < 1e-10);
}

#[test]
fn works_with_bools() {
    let s = Signal::new(false);
    let us = UndoableSignal::new(s, UndoConfig::default());

    us.set(true);
    assert!(us.read());

    assert!(us.undo());
    assert!(!us.read());
}

// ── enable_undo stores on element ────────────────────────────────

#[test]
fn enable_undo_returns_tracked_signal() {
    use burin::core::element::{Element, ElementArena};

    let mut arena = ElementArena::new();
    let ct = arena.component_tables.clone();
    let eid = arena.insert(Element::new(ct));
    let el = arena.get_mut(eid).unwrap();

    let sig = Signal::new("test".to_string());
    let us = undo::enable_undo(el, sig, UndoConfig::default());

    us.set("changed".to_string());
    assert_eq!(us.read(), "changed");
    assert!(us.undo());
    assert_eq!(us.read(), "test");
}
