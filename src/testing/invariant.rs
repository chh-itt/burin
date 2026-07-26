//! Frame-level invariants that are checked automatically in debug/test builds.
//!
//! Each invariant is a cheap O(k) check. Violations panic immediately at the
//! frame where the bug was introduced — no bisecting required.

use crate::core::element::ElementArena;
use crate::core::ElementId;
use crate::event::FocusManager;

/// Run all invariants against the current harness state.
/// Called automatically at the end of `run_frame()` in debug builds.
pub(crate) fn check_all(
    arena: &ElementArena,
    root_id: ElementId,
    focus: &FocusManager,
    frame_id: u64,
) {
    check_tree_invariants(arena, root_id, frame_id);
    check_focus_invariants(arena, focus, frame_id);
    check_dirty_invariants(frame_id);
}

/// Tree invariants: depth consistency, valid parent pointers, no duplicate children.
fn check_tree_invariants(arena: &ElementArena, root_id: ElementId, frame_id: u64) {
    let Some(_root) = arena.get(root_id) else {
        panic!(
            "[invariant frame={frame_id}] root element {:?} not found",
            root_id
        );
    };
    check_subtree(arena, root_id, 0, frame_id);
}

fn check_subtree(arena: &ElementArena, eid: ElementId, expected_depth: u32, frame_id: u64) {
    let Some(el) = arena.get(eid) else {
        panic!(
            "[invariant frame={frame_id}] element {:?} in children list but not in arena",
            eid
        );
    };

    assert_eq!(
        el.depth, expected_depth,
        "[invariant frame={frame_id}] element {:?} depth {} != expected {}",
        eid, el.depth, expected_depth,
    );

    // Parent pointer invariant.
    if let Some(pid) = el.parent {
        assert!(
            arena.get(pid).is_some_and(|p| p.children.contains(&eid)),
            "[invariant frame={frame_id}] element {:?} parent {:?} does not contain it as child",
            eid,
            pid,
        );
    }

    // Check for duplicate children.
    let mut seen = std::collections::HashSet::new();
    for &cid in &el.children {
        assert!(
            seen.insert(cid),
            "[invariant frame={frame_id}] element {:?} has duplicate child {:?}",
            eid,
            cid,
        );
        check_subtree(arena, cid, expected_depth + 1, frame_id);
    }
}

/// Focus invariants: focused element should be focusable. Soft warning only
/// since some widgets (table cells, etc.) receive focus via propagation without
/// explicitly setting a focus policy.
fn check_focus_invariants(arena: &ElementArena, focus: &FocusManager, frame_id: u64) {
    let Some(fid) = focus.focused() else { return };
    let Some(el) = arena.get(fid) else { return };
    if !el.is_focusable() {
        eprintln!(
            "[invariant frame={frame_id}] focused element {:?} is not focusable \
             (may be intentional — table cells, composite widgets)",
            fid,
        );
    }
}

/// Dirty set invariants: catch excessive dirty counts.
#[cfg(debug_assertions)]
fn check_dirty_invariants(frame_id: u64) {
    let (dirty, steps) = crate::core::dirty_registry::stats();

    if steps > dirty * 2 && dirty > 0 {
        eprintln!(
            "[invariant frame={frame_id}] potential O(N) dirty processing: \
             {steps} steps for {dirty} dirty elements (ratio {:.1})",
            steps as f64 / dirty as f64
        );
    }
    const MAX_DIRTY: usize = 10_000;
    if dirty > MAX_DIRTY {
        panic!("[invariant frame={frame_id}] excessive dirty count: {dirty} > {MAX_DIRTY}");
    }
}

#[cfg(not(debug_assertions))]
fn check_dirty_invariants(_frame_id: u64) {}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::element::ElementArena;

    #[test]
    fn valid_tree_passes_invariants() {
        let mut arena = ElementArena::new();
        let root = arena.allocate();
        arena.set_root(root);
        let child = arena.allocate();
        arena.add_child(root, child);
        let _focus = FocusManager::new();
        check_tree_invariants(&arena, root, 0);
    }

    #[test]
    #[should_panic(expected = "depth")]
    fn wrong_depth_panics() {
        let mut arena = ElementArena::new();
        let root = arena.allocate();
        arena.set_root(root);
        let child = arena.allocate();
        arena.add_child(root, child);
        if let Some(el) = arena.get_mut(child) {
            el.depth = 99;
        }
        check_tree_invariants(&arena, root, 0);
    }

    #[test]
    #[should_panic(expected = "duplicate")]
    fn duplicate_child_panics() {
        let mut arena = ElementArena::new();
        let root = arena.allocate();
        arena.set_root(root);
        let child = arena.allocate();
        arena.add_child(root, child);
        if let Some(el) = arena.get_mut(root) {
            el.children.push(child);
        }
        check_tree_invariants(&arena, root, 0);
    }
}
