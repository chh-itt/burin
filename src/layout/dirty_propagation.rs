use crate::core::dirty_registry;
use crate::core::element::{DirtyFlags, ElementArena};
use crate::core::id::ElementId;
use std::collections::{HashMap, HashSet};

/// Targeted dirty flag processing: iterates the flat dirty-element set and
/// walks upward only as far as necessary, short-circuited by
/// `affected_by_child_size == false` containers.
///
/// Uses HashSet for O(1) dedup instead of Vec::contains + sort+dedup.
pub fn process_dirty_set(
    arena: &ElementArena,
    elements: &[(ElementId, DirtyFlags)],
) -> (Vec<ElementId>, bool, Vec<ElementId>, Vec<ElementId>) {
    // Drain deferred subtree-gen bumps BEFORE any early-return so a frame
    // whose only dirty work is pending bumps still processes them (otherwise
    // elements marked solely via bump_subtree_gen — e.g. virtual-list item
    // re-mapping after a scroll via defer_action — are never repainted,
    // locked out by the empty-DIRTY_ENTRIES guard below).
    let pending_bumps: HashSet<ElementId> =
        dirty_registry::drain_pending_bumps().into_iter().collect();

    if elements.is_empty() && pending_bumps.is_empty() {
        let has_measure = arena
            .root_id
            .and_then(|rid| arena.get(rid))
            .is_some_and(|r| r.dirty.get().has_measure());
        return (Vec::new(), has_measure, Vec::new(), Vec::new());
    }

    let root_id = arena.root_id.expect("No root element set in arena");

    let mut sorted: Vec<(ElementId, DirtyFlags, u32)> = elements
        .iter()
        .filter_map(|&(id, flags)| {
            if dirty_registry::parent_of(id).is_none() && id != root_id {
                return None;
            }
            let depth = arena.get(id).map_or(0, |el| el.depth);
            Some((id, flags, depth))
        })
        .collect();
    sorted.sort_by_key(|&(_, _, d)| std::cmp::Reverse(d));

    // Merge any pending subtree_gen bumps that aren't already in the dirty set.
    for eid in pending_bumps {
        if !sorted.iter().any(|&(id, _, _)| id == eid) {
            if let Some(el) = arena.get(eid) {
                sorted.push((eid, el.dirty.get(), el.depth));
            }
        }
    }

    let mut propagation: HashMap<ElementId, DirtyFlags> = HashMap::new();
    let mut paint_roots: HashSet<ElementId> = HashSet::new();
    let mut processed: HashSet<ElementId> = HashSet::new();
    let mut layout_roots: HashSet<ElementId> = HashSet::new();
    let mut indep_memo: std::collections::HashMap<ElementId, crate::ecs::components::AxisPair> =
        std::collections::HashMap::new();

    for (id, flags, _depth) in &sorted {
        #[cfg(debug_assertions)]
        crate::core::dirty_registry::inc_dirty_count();
        crate::core::dirty_registry::inc_devtools_dirty();

        let mut cur_id = *id;
        let mut cur_flags = *flags;

        if let Some(&existing) = propagation.get(&cur_id) {
            cur_flags |= existing;
        }
        propagation.insert(cur_id, cur_flags);
        processed.insert(cur_id);
        dirty_registry::bump_subtree_gen_local(cur_id);

        loop {
            #[cfg(debug_assertions)]
            crate::core::dirty_registry::inc_process_step();

            let Some(parent_id) = dirty_registry::parent_of(cur_id) else {
                paint_roots.insert(cur_id);
                break;
            };

            // Relayout boundary: parent's own outer size is independent of this
            // child's size (currently: affected_by_child_size==false). A
            // measure/reposition change is fully CONTAINED — the boundary needs
            // an isolated subtree relayout (recorded in layout_roots), but nothing
            // above it changes size or position, so STOP layout propagation here.
            // Paint / subtree-gen still propagate to the root so the changed
            // subtree is cache-invalidated and repainted.
            let size_change = cur_flags.has_measure() || cur_flags.has_reposition();
            let indep = crate::layout::taffy_bridge::size_independent_memo(
                arena,
                parent_id,
                &mut indep_memo,
            );
            let is_boundary =
                !dirty_registry::affected_by_child_size(parent_id) || indep.x || indep.y;
            if size_change && is_boundary {
                layout_roots.insert(parent_id);
                dirty_registry::bump_subtree_gen_local(parent_id);
                processed.insert(parent_id);
                paint_roots.insert(parent_id);
                let mut bump_id = parent_id;
                while let Some(gp_id) = dirty_registry::parent_of(bump_id) {
                    dirty_registry::bump_subtree_gen_local(gp_id);
                    processed.insert(gp_id);
                    bump_id = gp_id;
                }
                break;
            }

            if !cur_flags.has_reposition() && !cur_flags.has_measure() {
                // Child has only REPAINT — don't mark REPAINT on the parent.
                dirty_registry::bump_subtree_gen_local(parent_id);
                processed.insert(parent_id);
                paint_roots.insert(cur_id);
                if dirty_registry::has_solid_background(parent_id) {
                    paint_roots.insert(parent_id);
                }
                let mut bump_id = parent_id;
                while let Some(gp_id) = dirty_registry::parent_of(bump_id) {
                    dirty_registry::bump_subtree_gen_local(gp_id);
                    processed.insert(gp_id);
                    bump_id = gp_id;
                }
                break;
            }

            cur_id = parent_id;
            // Ancestors inherit only the layout-class bits (MEASURE /
            // REPOSITION). The REPAINT bit is deliberately stripped from
            // the climb: after taffy runs, layout_phase's old-bounds diff
            // re-marks REPAINT on exactly the elements whose rect actually
            // changed. Pre-staining the whole chain forced bounds-stable
            // ancestors into the re-record path every frame (surface/decor
            // generations unchanged → pure waste, flagged by the
            // over-render detector) instead of replaying their own scene
            // (Phase 3.9, audit 2026-07-18 animation pass).
            cur_flags = DirtyFlags(cur_flags.0 & !DirtyFlags::REPAINT.0);
            let existing = propagation
                .get(&cur_id)
                .copied()
                .unwrap_or(DirtyFlags::CLEAN);
            let merged = existing | cur_flags;
            propagation.insert(cur_id, merged);
            processed.insert(cur_id);
            dirty_registry::mark_dirty(cur_id, merged);
            dirty_registry::bump_subtree_gen_local(cur_id);
        }
    }

    // Portal elements — use the explicit portal registry instead of scanning
    // root.children for z_index > 0 children.
    for &cid in &crate::platform::portal::portal_ids() {
        if let Some(child) = arena.get(cid) {
            if child.is_visible() && child.needs_repaint() {
                paint_roots.insert(cid);
                processed.insert(cid);
            }
        }
    }

    let has_measure = arena
        .get(root_id)
        .is_some_and(|r| r.dirty.get().has_measure());

    (
        paint_roots.into_iter().collect(),
        has_measure,
        processed.into_iter().collect(),
        layout_roots.into_iter().collect(),
    )
}

#[allow(dead_code)]
pub fn clear_dirty_subtree(arena: &mut ElementArena, eid: ElementId, level: DirtyFlags) {
    if let Some(element) = arena.get_mut(eid) {
        let mask = DirtyFlags(!level.0 & 0b111);
        let current = element.dirty.get();
        element.dirty.set(DirtyFlags(current.0 & mask.0));
    }
    let child_ids: Vec<ElementId> = arena
        .get(eid)
        .map(|e| e.children.clone())
        .unwrap_or_default();
    for cid in child_ids {
        clear_dirty_subtree(arena, cid, level);
    }
}

#[allow(dead_code)]
pub fn pre_compute_paint_flags(arena: &ElementArena) -> (bool, bool) {
    let dirty = dirty_registry::take_dirty();
    let (_roots, has_measure, _processed, _layout_roots) = process_dirty_set(arena, &dirty);
    let root_id = arena.root_id.expect("No root element set in arena");
    (
        arena
            .get(root_id)
            .is_some_and(|r| r.dirty.get().has_repaint()),
        has_measure,
    )
}
