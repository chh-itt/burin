//! Shared frame orchestration phases used by both `Window::on_frame()` and
//! `TestHarness::run_frame()`. Each function is a pure operation on
//! arena / taffy / focus / animation state — no platform (renderer / kinetic /
//! drag) coupling. Keeping the orchestration in one place prevents the two
//! frame drivers from drifting apart.
//!
//! ## Phase Isolation
//!
//! The frame pipeline is split into 3 strictly-ordered phases with unidirectional data flow:
//!
//! ```text
//! ┌───────────────────────────────────────────────────────┐
//! │  Prepass  → 只读 ECS，只能 queue DeferredAction        │
//! │  Layout   → 只读 ECS + DeferredAction，写 Taffy        │
//! │  Paint    → 读 AnimationDriver + ECS + Cache，写 Painter │
//! └───────────────────────────────────────────────────────┘
//! ```
//!
//! Cross-phase writes are routed through `dirty_registry::defer_action()`,
//! which queues mutations for execution at the next phase boundary.

use crate::animation::AnimationDriver;
use crate::core::dirty_registry;
use crate::core::element::{DirtyFlags, ElementArena};
use crate::core::ElementId;
use crate::event::EventRegistry;
use crate::layout::dirty_propagation::process_dirty_set;
use crate::layout::taffy_bridge::TaffyBridge;
use crate::style::{Rect, Size};
use std::collections::HashSet;
use web_time::Instant;

// ── Phase Isolation Infrastructure ──────────────────────────────

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum FramePhase {
    #[default]
    None,
    Prepass,
    Layout,
    Paint,
}

pub fn set_phase(phase: FramePhase) {
    crate::core::app_context::current_app().set_phase(phase);
}

pub fn current_phase() -> FramePhase {
    crate::core::app_context::current_app().current_phase()
}

pub(crate) fn debug_assert_phase(allowed: &[FramePhase]) {
    let current = current_phase();
    debug_assert!(
        allowed.contains(&current),
        "Phase violation: operation requires {:?} but current phase is {:?}",
        allowed,
        current,
    );
}

// ── Incremental layout gate (enabled by default) ──
thread_local! {
    static INCREMENTAL_LAYOUT: std::cell::Cell<bool> = const { std::cell::Cell::new(true) };
}

thread_local! { static INCREMENTAL_TAKEN: std::cell::Cell<u64> = const { std::cell::Cell::new(0) }; }

/// Number of times `layout_phase` took the incremental (relayout-boundary) path.
/// Test/diagnostic hook only.
pub(crate) fn incremental_taken_count() -> u64 {
    INCREMENTAL_TAKEN.with(|c| c.get())
}

thread_local! { static ESCALATION_TAKEN: std::cell::Cell<u64> = const { std::cell::Cell::new(0) }; }

/// Number of times the incremental path was tried but escalated to a full pass
/// (a single-axis boundary's dependent-axis size changed). Test/diagnostic hook.
pub(crate) fn escalation_taken_count() -> u64 {
    ESCALATION_TAKEN.with(|c| c.get())
}

/// Whether relayout-boundary incremental layout is enabled for this thread (default: true).
pub(crate) fn incremental_layout_enabled() -> bool {
    INCREMENTAL_LAYOUT.with(|c| c.get())
}

/// Enable/disable relayout-boundary incremental layout (test hook).
#[allow(dead_code)]
pub(crate) fn set_incremental_layout_enabled(v: bool) {
    INCREMENTAL_LAYOUT.with(|c| c.set(v));
}

// ── Paint subtree-cache hit/miss counters (per-frame; reset in drive_frame_layout) ──
thread_local! { static SUBTREE_CACHE_HITS: std::cell::Cell<u64> = const { std::cell::Cell::new(0) }; }
thread_local! { static SUBTREE_CACHE_MISSES: std::cell::Cell<u64> = const { std::cell::Cell::new(0) }; }

/// Reset the per-frame paint-cache counters. Called at the top of each frame
/// (`drive_frame_layout`), so the values reflect a single frame's paint.
pub(crate) fn reset_paint_cache_stats() {
    SUBTREE_CACHE_HITS.with(|c| c.set(0));
    SUBTREE_CACHE_MISSES.with(|c| c.set(0));
}

/// Record a subtree-cache replay (O(k) paint win). Called from `try_skip_subtree`.
pub(crate) fn bump_subtree_cache_hit() {
    SUBTREE_CACHE_HITS.with(|c| c.set(c.get() + 1));
}

/// Record a subtree-cache miss (re-record). Called from `try_skip_subtree`.
pub(crate) fn bump_subtree_cache_miss() {
    SUBTREE_CACHE_MISSES.with(|c| c.set(c.get() + 1));
}

/// Number of subtree-cache replays in the most recent frame. Test/diagnostic hook.
pub(crate) fn subtree_cache_hits() -> u64 {
    SUBTREE_CACHE_HITS.with(|c| c.get())
}

/// Number of subtree-cache misses (re-records) in the most recent frame.
pub(crate) fn subtree_cache_misses() -> u64 {
    SUBTREE_CACHE_MISSES.with(|c| c.get())
}

/// Run the O(k) pre-passes that are pure arena operations
/// (cursor blink, gesture timeouts, frame ticks, scroll simulations,
/// overlay pop, sticky header).
pub(crate) fn run_pre_passes(arena: &ElementArena) {
    let _prev = crate::core::dirty_registry::current_trigger();
    crate::core::dirty_registry::set_current_trigger(
        crate::core::dirty_registry::DirtyTriggerTag::FrameTick,
    );
    crate::platform::window::process_cursor_blink(arena);
    crate::event::recognizer::process_timeouts();
    crate::platform::window::process_frame_ticks(arena);
    crate::widgets::bundle::scroll::process_active_simulations(arena);
    crate::event::overlay::process_pending_pop();
    crate::widgets::layout::sticky_header::process_all(arena);
    crate::core::dirty_registry::set_current_trigger(_prev);
}

/// Drain exit/animation requests, tick the driver, apply interpolations, and
/// check exit_pending flags. `now` is the clock source — both callers pass
/// `crate::core::clock::now()` so the path is identical (wall-clock in
/// production, virtual clock under test).
pub(crate) fn animation_phase(arena: &mut ElementArena, animations: &mut AnimationDriver, now: Instant) {
    for req in crate::animation::drain_exit_requests() {
        if let Some(el) = arena.get_mut(req.target) {
            el.animate_exit(req.property, req.to, req.animation);
        }
    }
    crate::animation::drain_requests(animations);
    crate::animation::set_in_anim_apply(true);
    let _prev = crate::core::dirty_registry::current_trigger();
    crate::core::dirty_registry::set_current_trigger(
        crate::core::dirty_registry::DirtyTriggerTag::Animation,
    );
    animations.tick(arena, now);
    crate::core::dirty_registry::set_current_trigger(_prev);
    crate::animation::set_in_anim_apply(false);
}

/// After the animation tick, re-drain dirty entries and re-process them —
/// animations call `mark_repaint`, producing dirty flags that must be
/// propagated to ancestors (and the root) so the frame actually paints them.
pub(crate) fn recheck_dirty_phase(
    arena: &ElementArena,
    root_id: ElementId,
    paint_roots: &mut Vec<ElementId>,
    processed_all: &mut Vec<ElementId>,
    root_flags: &mut DirtyFlags,
) {
    let anim_dirty = dirty_registry::take_dirty();
    if anim_dirty.is_empty() {
        return;
    }
    let (anim_roots, _, anim_processed, _) = process_dirty_set(arena, &anim_dirty);
    paint_roots.extend(anim_roots);
    processed_all.extend(anim_processed);
    if let Some(el) = arena.get(root_id) {
        *root_flags |= el.dirty.get();
    }
    if !paint_roots.is_empty() {
        *root_flags |= DirtyFlags::REPAINT;
    }
}

/// Outcome of the dirty processing phase — consumed by layout + paint decisions.
pub(crate) struct DirtyOutcome {
    pub paint_roots: Vec<ElementId>,
    pub has_measure: bool,
    pub processed_all: Vec<ElementId>,
    pub root_flags: DirtyFlags,
    pub layout_roots: Vec<ElementId>,
}

/// Drain dirty entries, process the dirty set, determine root flags.
/// The multi-window filter is included — for harness (single arena) the
/// "other" partition is always empty (a harmless no-op).
pub(crate) fn process_dirty_phase(arena: &ElementArena, root_id: ElementId) -> DirtyOutcome {
    // Multi-window filter: entries whose elements belong to another
    // window's arena are parked in the foreign bucket; App::about_to_wait
    // redistributes them to (and wakes) the owning window. Never push
    // them back into our own queue — they would ping-pong forever and the
    // owning window would never learn about them.
    let all_dirty = dirty_registry::take_dirty();
    let (mine, other): (Vec<_>, Vec<_>) = all_dirty
        .into_iter()
        .partition(|(id, _)| arena.get(*id).is_some());
    if !other.is_empty() {
        crate::core::app_context::current_app().park_foreign_dirty(other);
    }
    let dirty_elements = mine;

    let (mut paint_roots, has_measure, mut processed_all, mut layout_roots) =
        process_dirty_set(arena, &dirty_elements);

    let root_dirty = arena
        .get(root_id)
        .map(|r| r.dirty.get())
        .unwrap_or(DirtyFlags::CLEAN);
    let mut root_flags = if has_measure {
        DirtyFlags::MEASURE
    } else if root_dirty.has_reposition() {
        DirtyFlags::REPOSITION
    } else if root_dirty.has_repaint() || !paint_roots.is_empty() {
        DirtyFlags::REPAINT
    } else {
        DirtyFlags::CLEAN
    };

    // Systemic fallback — catch dirty entries that arrived after take_dirty.
    if root_flags.is_clean() && dirty_registry::has_pending_dirty() {
        let extra = dirty_registry::take_dirty();
        if !extra.is_empty() {
            let (extra_roots, extra_has_measure, extra_processed, extra_layout_roots) =
                process_dirty_set(arena, &extra);
            root_flags = if extra_has_measure {
                DirtyFlags::MEASURE
            } else if arena
                .get(root_id)
                .is_some_and(|r| r.dirty.get().has_reposition())
            {
                DirtyFlags::REPOSITION
            } else {
                DirtyFlags::REPAINT
            };
            paint_roots.extend(extra_roots);
            processed_all.extend(extra_processed);
            layout_roots.extend(extra_layout_roots);
        }
    }

    DirtyOutcome {
        paint_roots,
        has_measure,
        processed_all,
        root_flags,
        layout_roots,
    }
}

/// Clear REPAINT / MEASURE / REPOSITION flags from the processed set.
/// This is the end-of-frame double-safety: `paint_element_tree` clears
/// repaint on elements it visits, but some elements in `processed_all`
/// may be cache-skipped — this catch-all flush prevents stale flags
/// from persisting across frames.
pub(crate) fn clear_dirty_phase(processed_all: &[ElementId]) {
    dirty_registry::clear_dirty_in_set(processed_all, DirtyFlags::REPAINT);
    dirty_registry::clear_dirty_in_set(processed_all, DirtyFlags::MEASURE_BIT);
    dirty_registry::clear_dirty_in_set(processed_all, DirtyFlags::REPOSITION_BIT);
}

/// Taffy incremental decision + layout + post-layout per-element write-back
/// (set_bounds, update_bounds, fire_resize, spatial_register) with the
/// old-bounds diff that marks only *changed* elements REPAINT.
///
/// Returns `(root_flags, did_layout)`. `did_layout` is true when taffy ran,
/// telling the caller to run its post-layout extras (portal positions, drag
/// ghost, resolve_pending_scrolls, clear MEASURE/REPOSITION). Caller-only
/// concerns (window's `needs_taffy`, drag ghost, portal positioning,
/// apply_drag_layouts) stay in the caller.
#[allow(clippy::too_many_arguments)]
pub(crate) fn layout_phase(
    arena: &mut ElementArena,
    taffy: &mut TaffyBridge,
    events: &mut EventRegistry,
    root_id: ElementId,
    size: Size,
    structural: HashSet<ElementId>,
    processed_all: &[ElementId],
    layout_roots: &[ElementId],
    has_measure: bool,
    mut root_flags: DirtyFlags,
    is_first_frame: bool,
    force_taffy: bool,
) -> (DirtyFlags, bool) {
    let need_taffy = force_taffy
        || is_first_frame
        || !structural.is_empty()
        || has_measure
        || root_flags.has_reposition()
        || !layout_roots.is_empty();

    let had_structural = !structural.is_empty();

    if is_first_frame {
        if let Some(el) = arena.get_mut(root_id) {
            el.set_bounds(Rect::new(0.0, 0.0, size.width, size.height));
            el.screen_bounds = el.bounds();
        }
    }
    if !need_taffy {
        return (root_flags, false);
    }

    let available = Size::new(size.width, size.height);

    // ── Incremental taffy: rebuild only what changed ──
    // Structural containment (audit 2026-07-16, Layer 3-2): when every
    // structurally-changed container sits under a relayout boundary, the
    // rebuild is folded into the incremental path (compute_subtree on those
    // boundaries) instead of escalating to a full-tree pass. The existing
    // dependent-axis escalation check still guards correctness.
    let mut structural_boundaries: Vec<ElementId> = Vec::new();
    let mut structural_contained = !structural.is_empty();
    if is_first_frame {
        taffy.clear();
        taffy.build_full_tree(arena, root_id);
        structural_contained = false;
    } else if !structural.is_empty() {
        let mut structural_vec: Vec<ElementId> = structural.into_iter().collect();
        structural_vec.sort_by_key(|&eid| arena.get(eid).map_or(0, |el| el.depth as i32));
        structural_vec.reverse(); // deepest first

        let mut rebuilt: HashSet<ElementId> = HashSet::new();
        for &container_id in &structural_vec {
            if rebuilt.contains(&container_id) {
                continue;
            }
            let (new_node, old_node) = taffy.ensure_subtree(arena, container_id);
            if let Some(old) = old_node {
                if let Some(pid) = dirty_registry::parent_of(container_id) {
                    taffy.relink_to_parent(pid, new_node, Some(old));
                    rebuilt.insert(pid);
                }
            }
            rebuilt.insert(container_id);
            let mut stack: Vec<ElementId> = vec![container_id];
            while let Some(eid) = stack.pop() {
                if rebuilt.insert(eid) {
                    if let Some(el) = arena.get(eid) {
                        stack.extend(el.children.iter().copied());
                    }
                }
            }
        }

        // Containment probe: walk up from each structural container to the
        // nearest boundary whose own size does not depend on its children
        // (same rule as dirty propagation). No boundary before the root →
        // the change may resize ancestors → full pass.
        for &sid in &structural_vec {
            let mut cur = Some(sid);
            let mut found: Option<ElementId> = None;
            while let Some(id) = cur {
                if id != root_id {
                    let indep = if !dirty_registry::affected_by_child_size(id) {
                        crate::ecs::components::AxisPair::both(true)
                    } else {
                        crate::layout::taffy_bridge::size_independent_of_children(arena, id)
                    };
                    if indep.x && indep.y {
                        found = Some(id);
                        break;
                    }
                }
                cur = dirty_registry::parent_of(id);
            }
            match found {
                Some(b) => structural_boundaries.push(b),
                None => {
                    structural_contained = false;
                    break;
                }
            }
        }

        if structural_contained {
            // Refresh styles of the other dirty elements too (mirrors the
            // non-structural incremental branch below).
            for &eid in processed_all {
                taffy.update_style(arena, eid);
            }
        }
    } else if has_measure || root_flags.has_reposition() || !layout_roots.is_empty() {
        // Also run for the incremental (contained-boundary) case: the changed
        // elements' taffy styles (e.g. a text's measured width) must be refreshed
        // BEFORE compute_subtree, otherwise the isolated subtree is computed with
        // stale styles and diverges from the full pass.
        for &eid in processed_all {
            taffy.update_style(arena, eid);
        }
    }

    // Always update portal element styles before layout.
    for &portal_id in &crate::platform::portal::portal_ids() {
        taffy.update_style(arena, portal_id);
    }

    let use_incremental = incremental_layout_enabled()
        && !is_first_frame
        // Structural changes fold into the incremental path when every
        // changed container is contained under a both-axes boundary.
        && (!had_structural || structural_contained)
        && !root_flags.has_reposition()   // genuine resize -> full pass
        && !has_measure                    // a change escaped to root (no boundary) -> full pass
        && (!layout_roots.is_empty() || !structural_boundaries.is_empty());
    // NOTE (audit 2026-07-16, Layer 3-2): the former blanket gate
    // `portal_position_ids().is_empty()` permanently disabled incremental
    // layout for any app that ever mounted a dropdown/tooltip (persistent
    // portals register at mount, even closed). Portal repositioning is
    // driven by its own MEASURE dirty (update_portal_positions) which sets
    // has_measure → full pass on the frames that actually move a portal;
    // dormant portals are untouched by boundary-subtree recomputes.

    let results = if use_incremental {
        INCREMENTAL_TAKEN.with(|c| c.set(c.get() + 1));
        // Merge dirty-propagation boundaries with structural-containment
        // boundaries, then dedup: drop any boundary that is a descendant of
        // another (the ancestor's subtree recompute already covers it).
        let mut merged_roots: Vec<ElementId> = layout_roots.to_vec();
        for b in structural_boundaries {
            if !merged_roots.contains(&b) {
                merged_roots.push(b);
            }
        }
        let boundaries: Vec<ElementId> = merged_roots
            .iter()
            .copied()
            .filter(|&b| taffy.node_for(b).is_some())
            .filter(|&b| {
                let mut p = dirty_registry::parent_of(b);
                while let Some(pid) = p {
                    if merged_roots.contains(&pid) {
                        return false;
                    }
                    p = dirty_registry::parent_of(pid);
                }
                true
            })
            .collect();

        let mut acc: Vec<(ElementId, Rect)> = Vec::new();
        let mut escalate = false;
        for b in boundaries {
            let Some(node) = taffy.node_for(b) else {
                continue;
            };
            let (cached, origin) = arena
                .get(b)
                .map(|el| {
                    let r = el.screen_bounds;
                    (Size::new(r.width, r.height), (r.x, r.y))
                })
                .unwrap_or((available, (0.0, 0.0)));
            // Freeze axes: an explicit `affected_by_child_size==false` widget
            // claims both axes independent; otherwise use the per-axis rule.
            let indep = if !dirty_registry::affected_by_child_size(b) {
                crate::ecs::components::AxisPair::both(true)
            } else {
                crate::layout::taffy_bridge::size_independent_of_children(arena, b)
            };
            let (sub, new_size) = taffy.compute_subtree(node, indep, cached, origin);
            // Verify the DEPENDENT axes (indep == false) did not change; the
            // independent (frozen) axes always match. A changed dependent axis
            // means the change escaped this single-axis boundary → full pass.
            let dx_changed = !indep.x && (new_size.width - cached.width).abs() > 0.5;
            let dy_changed = !indep.y && (new_size.height - cached.height).abs() > 0.5;
            if dx_changed || dy_changed {
                escalate = true;
                break;
            }
            acc.extend(sub);
        }

        if escalate {
            ESCALATION_TAKEN.with(|c| c.set(c.get() + 1));
            let root_node = sized_root_node(taffy, arena, root_id, available);
            taffy.compute_layout(root_node, available)
        } else {
            acc
        }
    } else {
        let root_node = sized_root_node(taffy, arena, root_id, available);
        taffy.compute_layout(root_node, available)
    };
    let mut layout_changed = 0usize;
    for (eid, rect) in &results {
        // Old bounds are read per-element from the registry *before* the
        // write-back below — O(k) total, replacing the former O(N)
        // whole-tree `snapshot_element_bounds()` (audit 2026-07-15: the
        // snapshot dominated incremental layout, 101µs → 12µs @ 4.7k nodes).
        let old_bound = dirty_registry::bounds_of(*eid);
        if let Some(el) = arena.get_mut(*eid) {
            el.set_bounds(*rect);
            el.screen_bounds = *rect;
            dirty_registry::update_bounds(*eid, *rect);
        }
        events.fire_resize(*eid, rect.width, rect.height);
        if let Some(el) = arena.get(*eid) {
            dirty_registry::spatial_register(*eid, *rect, el.tree_order);
        }
        let changed = old_bound.is_none_or(|old| old != *rect);
        if changed {
            layout_changed += 1;
            dirty_registry::mark_dirty(*eid, DirtyFlags::REPAINT);
            dirty_registry::register_dirty(*eid, DirtyFlags::REPAINT);

            // When the content-area width of a text element changes, bump
            // text_generation so the next paint pass sees a mismatch against
            // buffer_gen and rebuilds the text buffer at the new width.
            // Deferred: the mutation happens after layout completes, keeping
            // the layout phase free of direct ECS writes.
            let old_w = old_bound.map(|r| r.width).unwrap_or(0.0);
            if (rect.width - old_w).abs() > 0.5 {
                if let Some(el) = arena.get(*eid) {
                    if el.lazy_label().is_some() {
                        if let Some(ref tg) = el.text_generation() {
                            let old = tg.get();
                            let tg_rc = tg.clone();
                            dirty_registry::defer_action(move |_arena, _, _| {
                                tg_rc.set(old.wrapping_add(1));
                            });
                            vgen!(
                                "[VGEN:LAYOUT_W] eid={:?} text_generation: {} -> {} width: {:.1} -> {:.1}",
                                eid, old, old.wrapping_add(1), old_w, rect.width
                            );
                        }
                    }
                }
            }
        }
    }
    if let Some(el) = arena.get_mut(root_id) {
        el.set_bounds(Rect::new(0.0, 0.0, size.width, size.height));
        el.screen_bounds = el.bounds();
    }
    if layout_changed > 0 {
        root_flags |= DirtyFlags::REPAINT;
    }

    (root_flags, true)
}

/// Resolve (or rebuild) the root taffy node and pin its style size to the
/// available window size. The node is valid by construction (`node_for` hit
/// or freshly built), so style access failures are internal invariants.
fn sized_root_node(
    taffy: &mut TaffyBridge,
    arena: &ElementArena,
    root_id: ElementId,
    available: Size,
) -> taffy::NodeId {
    let root_node = taffy
        .node_for(root_id)
        .unwrap_or_else(|| taffy.build_full_tree(arena, root_id));
    let mut s = taffy
        .tree
        .style(root_node)
        .expect("root taffy node must have a style")
        .clone();
    s.size.width = taffy::Dimension::length(available.width);
    s.size.height = taffy::Dimension::length(available.height);
    taffy
        .tree
        .set_style(root_node, s)
        .expect("root taffy node accepts set_style");
    root_node
}
