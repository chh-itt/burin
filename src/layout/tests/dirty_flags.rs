use crate::core::dirty_registry;
use crate::core::element::{DirtyFlags, Element, ElementArena};
use crate::layout::dirty_propagation::process_dirty_set;
use crate::style::Color;

fn el() -> Element {
    Element::new(std::rc::Rc::new(std::cell::RefCell::new(
        crate::ecs::tables::ComponentTables::default(),
    )))
}

fn arena_with_tree() -> ElementArena {
    let mut arena = ElementArena::new();

    let text = el();
    let box_with_bg = el();
    let container = el();
    let root = el();

    let root_id = arena.insert(root);
    let container_id = arena.insert(container);
    let box_id = arena.insert(box_with_bg);
    let text_id = arena.insert(text);

    arena.add_child(root_id, container_id);
    arena.add_child(container_id, box_id);
    arena.add_child(box_id, text_id);
    arena.set_root(root_id);

    // Configure elements AFTER insertion so their ids are valid.
    if let Some(b) = arena.get_mut(box_id) {
        b.set_background(Color::rgba8(59, 130, 246, 255));
    }
    if let Some(t) = arena.get_mut(text_id) {
        t.clear_repaint();
        t.mark_repaint();
    }

    arena
}

#[test]
fn new_element_starts_repaint_dirty() {
    let el = el();
    assert!(el.needs_repaint());
    assert!(!el.needs_reposition());
    assert!(!el.needs_measure());
}

#[test]
fn mark_measure_implies_reposition_and_repaint() {
    let el = el();
    el.clear_repaint();
    assert!(!el.needs_repaint());

    el.mark_measure();
    assert!(el.needs_measure());
    assert!(el.needs_reposition());
    assert!(el.needs_repaint());
}

#[test]
fn mark_reposition_implies_repaint() {
    let el = el();
    el.clear_repaint();
    el.mark_reposition();
    assert!(!el.needs_measure());
    assert!(el.needs_reposition());
    assert!(el.needs_repaint());
}

#[test]
fn mark_repaint_preserves_measure_and_reposition() {
    let el = el();
    el.mark_measure();
    assert!(el.needs_measure());
    assert!(el.needs_reposition());

    el.mark_repaint();
    assert!(el.needs_measure());
    assert!(el.needs_reposition());
    assert!(el.needs_repaint());
}

#[test]
fn clear_repaint_leaves_measure_and_reposition() {
    let el = el();
    el.clear_repaint();
    el.mark_measure();
    assert!(el.needs_measure());
    assert!(el.needs_reposition());
    assert!(el.needs_repaint());

    el.clear_repaint();
    assert!(el.needs_measure());
    assert!(el.needs_reposition());
    assert!(!el.needs_repaint());
}

#[test]
fn clear_reposition_leaves_measure() {
    let el = el();
    el.mark_measure();
    el.clear_reposition();
    assert!(el.needs_measure());
    assert!(!el.needs_reposition());
    assert!(el.needs_repaint());
}

#[test]
fn clear_measure_leaves_reposition_and_repaint() {
    let el = el();
    el.mark_measure();
    el.clear_measure();
    assert!(!el.needs_measure());
    assert!(el.needs_reposition());
    assert!(el.needs_repaint());
}

#[test]
fn clear_measure_then_clear_reposition_leaves_repaint() {
    let el = el();
    el.mark_measure();
    el.clear_measure();
    el.clear_reposition();
    assert!(!el.needs_measure());
    assert!(!el.needs_reposition());
    assert!(el.needs_repaint());
}

#[test]
fn clear_repaint_with_all_dirty() {
    let el = el();
    el.mark_measure();
    assert!(el.needs_measure());
    assert!(el.needs_reposition());
    assert!(el.needs_repaint());

    el.clear_repaint();
    assert!(el.needs_measure());
    assert!(el.needs_reposition());
    assert!(!el.needs_repaint());
}

#[test]
fn dirty_clone_shares_state() {
    let el = el();
    let clone = el.dirty.clone();

    el.mark_measure();
    assert!(clone.get().has_measure());

    el.clear_measure();
    el.clear_reposition();
    el.clear_repaint();
    assert!(clone.get().is_clean());
}

#[test]
fn bitwise_or_combines_flags() {
    let combined = DirtyFlags::REPAINT | DirtyFlags::REPOSITION;
    assert!(combined.has_repaint());
    assert!(combined.has_reposition());
    assert!(!combined.has_measure());
    assert!(!combined.is_clean());
}

#[test]
fn measure_or_reposition_yields_measure() {
    let combined = DirtyFlags::MEASURE | DirtyFlags::REPOSITION;
    assert!(combined.has_measure());
    assert!(combined.has_reposition());
    assert!(combined.has_repaint());
}

#[test]
fn reposition_or_repaint_yields_reposition() {
    let combined = DirtyFlags::REPOSITION | DirtyFlags::REPAINT;
    assert!(combined.has_reposition());
    assert!(combined.has_repaint());
    assert!(!combined.has_measure());
}

#[test]
fn downgrade_measure_to_reposition() {
    let flags = DirtyFlags::MEASURE;
    let downgraded = flags.downgrade_measure();
    assert!(!downgraded.has_measure());
    assert!(downgraded.has_reposition());
    assert!(downgraded.has_repaint());
}

#[test]
fn repaint_propagates_through_solid_background_to_root() {
    dirty_registry::take_dirty();
    dirty_registry::drain_structurally_changed();

    let arena = arena_with_tree();
    let root_id = arena.root_id.unwrap();

    // Find the text element ID (the leaf, marked repaint dirty)
    let dirty_entries = dirty_registry::take_dirty();
    assert!(
        !dirty_entries.is_empty(),
        "take_dirty should return the leaf's entry"
    );

    let (paint_roots, has_measure, _processed, _layout_roots) =
        process_dirty_set(&arena, &dirty_entries);

    assert!(
        !has_measure,
        "a repaint-only hover must not trigger a taffy layout"
    );

    // Verify paint roots include the leaf and its solid-bg parent
    // Find leaf and box IDs from the arena
    let root = arena.get(root_id).unwrap();
    let container_id = root.children[0];
    let box_id = arena.get(container_id).unwrap().children[0];
    let text_id = arena.get(box_id).unwrap().children[0];

    assert!(paint_roots.contains(&text_id), "leaf must be a paint root");
    assert!(
        paint_roots.contains(&box_id),
        "solid-background parent must be a paint root"
    );
}
