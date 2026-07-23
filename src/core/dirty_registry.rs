use std::cell::Cell;
use std::rc::Rc;

use crate::core::config::StateFlags;
use crate::core::element::DirtyFlags;
use crate::core::element::ElementArena;
use crate::core::id::ElementId;
use crate::event::EventRegistry;
use crate::style::{Point, Rect};

#[derive(Clone)]
pub(crate) struct ElInfo {
    pub(crate) dirty: Rc<std::cell::Cell<DirtyFlags>>,
    pub(crate) state: Rc<std::cell::Cell<StateFlags>>,
    pub(crate) affected_by_child_size: bool,
    pub(crate) size_independent: crate::ecs::components::AxisPair,
    pub(crate) has_solid_background: bool,
    pub(crate) subtree_gen: Rc<std::cell::Cell<u64>>,
    pub(crate) layout_gen: Rc<std::cell::Cell<u64>>,
    pub(crate) surface_gen: Option<Rc<std::cell::Cell<u64>>>,
    pub(crate) decor_gen: Option<Rc<std::cell::Cell<u64>>>,
    pub(crate) z_index: i32,
    pub(crate) accepts_mouse: bool,
    pub(crate) input_pass_through: bool,
    pub(crate) visible: bool,
    pub(crate) slot_inactive: Option<Rc<std::cell::Cell<bool>>>,
    pub(crate) reactive_visible: Option<Rc<std::cell::Cell<bool>>>,
    /// Cached accumulated scroll offset for O(1) hit-test reads.
    /// Invalidated globally by SCROLL_TREE_GEN.
    pub(crate) accum_scroll_x: Cell<f32>,
    pub(crate) accum_scroll_y: Cell<f32>,
    pub(crate) accum_scroll_gen: Cell<u64>,
    /// Stored for incremental focus-order maintenance.
    pub(crate) tree_order: u64,
    pub(crate) tab_index: Cell<Option<usize>>,
}

// The per-window wake callback (`on_dirty`) lives on AppContext
// (audit 2026-07-18 multi-window pass). The old process-global ON_DIRTY
// thread_local made the last-initialized window capture every other
// window's wakes; free functions below route through `current_app()`.

/// Reset the wake gate, allowing the next register_dirty() to fire the
/// window's `on_dirty` callback. Called by about_to_wait at the top of
/// each event-loop cycle so that each wake-up yields at most one
/// RedrawRequested per window.
pub fn reset_dirty_redraw() {
    crate::core::app_context::current_app().reset_dirty_redraw();
}

/// Suspend wake callbacks for the duration of a widget mount.
/// Batch is recursion-safe — only the outermost `end_mount_batch` flushes accumulated state.
pub fn begin_mount_batch() {
    crate::core::app_context::current_app().begin_mount_batch();
}

pub fn end_mount_batch() {
    let app = crate::core::app_context::current_app();
    let depth = app.end_mount_batch();
    if depth != 1 {
        return;
    }
    app.notify_dirty();
}

pub fn register_dirty(id: ElementId, flags: DirtyFlags) {
    #[cfg(debug_assertions)]
    trace_mark_probe(id, flags, "register_dirty");
    crate::core::app_context::current_app().register_dirty(id, flags);
}

/// Debug probe: `AURALIS_TRACE_DIRTY=<raw element id>` prints a backtrace
/// for every mark/register on that element. Zero cost when unset (env read
/// once via OnceLock). Invaluable for "who keeps dirtying #N" hunts.
#[cfg(debug_assertions)]
pub fn trace_mark_probe(id: ElementId, flags: DirtyFlags, site: &str) {
    static TRACE_ID: std::sync::OnceLock<Option<u64>> = std::sync::OnceLock::new();
    let target = TRACE_ID.get_or_init(|| {
        std::env::var("AURALIS_TRACE_DIRTY")
            .ok()
            .and_then(|v| v.parse().ok())
    });
    if *target == Some(id.to_u64()) {
        eprintln!(
            "[trace-dirty] {site}({:?}, {:?})\n{}",
            id,
            flags,
            std::backtrace::Backtrace::force_capture()
        );
    }
}

pub fn register_animation(id: ElementId, flags: DirtyFlags) {
    record_dirty_event(id, flags);
    record_signal_element_link(id);
    #[cfg(feature = "devtools")]
    {
        let app = crate::core::app_context::current_app();
        if app.is_frozen() {
            app.push_frozen_dirty(id, flags);
            return;
        }
    }
    crate::core::app_context::current_app().register_animation(id, flags);
}

pub fn take_dirty() -> Vec<(ElementId, DirtyFlags)> {
    crate::core::app_context::current_app().take_dirty()
}

pub fn register_parent(child: ElementId, parent: ElementId) {
    crate::core::app_context::current_app().register_parent(child, parent);
}

pub fn unregister_parent(child: ElementId) {
    crate::core::app_context::current_app().unregister_parent(child);
}

pub fn parent_of(id: ElementId) -> Option<ElementId> {
    crate::core::app_context::current_app().parent_of(id)
}

pub fn children_of(arena: &ElementArena, id: ElementId) -> Vec<ElementId> {
    arena
        .get(id)
        .map(|el| el.children.clone())
        .unwrap_or_default()
}

pub fn subtree_ids(arena: &ElementArena, id: ElementId) -> Vec<ElementId> {
    let mut result = Vec::new();
    let mut stack = vec![id];
    while let Some(cur) = stack.pop() {
        result.push(cur);
        if let Some(el) = arena.get(cur) {
            for &child in el.children.iter().rev() {
                stack.push(child);
            }
        }
    }
    result
}

pub fn is_descendant_of(child: ElementId, ancestor: ElementId) -> bool {
    crate::core::app_context::current_app().is_descendant_of(child, ancestor)
}

// ── Widget-domain teardown hooks ────────────────────────────────────
//
// Widget/platform modules keep their own thread_local registries (form
// validators, sticky headers, a11y node cache, …) because their value types
// live above the core layer. Historically none of them were wired into
// element teardown, so registrations of torn-down elements leaked forever
// (audit 2026-07-17 round 3, Finding A).
//
// `register_teardown_hook` lets any module install a `fn(ElementId)` cleanup
// callback that `teardown_subtree` invokes for every removed element —
// without a core → widgets reverse dependency. Hooks are deduplicated by fn
// pointer, so lazy registration at first use is idempotent.

thread_local! {
    static TEARDOWN_HOOKS: std::cell::RefCell<Vec<fn(ElementId)>> =
        const { std::cell::RefCell::new(Vec::new()) };
}

/// Install a per-element teardown cleanup hook. Idempotent per fn pointer.
pub fn register_teardown_hook(hook: fn(ElementId)) {
    TEARDOWN_HOOKS.with(|h| {
        let mut h = h.borrow_mut();
        if !h.contains(&hook) {
            h.push(hook);
        }
    });
}

/// Run all installed teardown hooks for `id`. Called by `teardown_subtree`
/// for every element in the removed subtree. Snapshots the hook list so a
/// hook may itself call `register_teardown_hook` without re-entrant borrow.
pub fn run_teardown_hooks(id: ElementId) {
    let hooks: Vec<fn(ElementId)> = TEARDOWN_HOOKS.with(|h| h.borrow().clone());
    for hook in hooks {
        hook(id);
    }
}

#[allow(clippy::too_many_arguments)]
pub fn register_element_full(
    id: ElementId,
    dirty: Rc<std::cell::Cell<DirtyFlags>>,
    state: Rc<std::cell::Cell<StateFlags>>,
    affected_by_child_size: bool,
    has_solid_background: bool,
    subtree_gen: Rc<std::cell::Cell<u64>>,
    layout_gen: Rc<std::cell::Cell<u64>>,
    surface_gen: Option<Rc<std::cell::Cell<u64>>>,
    decor_gen: Option<Rc<std::cell::Cell<u64>>>,
    z_index: i32,
    accepts_mouse: bool,
    input_pass_through: bool,
    visible: bool,
    slot_inactive: Option<Rc<std::cell::Cell<bool>>>,
    reactive_visible: Option<Rc<std::cell::Cell<bool>>>,
    tree_order: u64,
) {
    crate::core::app_context::current_app().register_element_info(
        id,
        dirty.clone(),
        state.clone(),
        affected_by_child_size,
        has_solid_background,
        subtree_gen.clone(),
        layout_gen.clone(),
        surface_gen.clone(),
        decor_gen.clone(),
        z_index,
        accepts_mouse,
        input_pass_through,
        visible,
        slot_inactive.clone(),
        reactive_visible.clone(),
        tree_order,
    );
}

pub fn unregister_element(id: ElementId) {
    let app = crate::core::app_context::current_app();
    if let Some((tab_index, tree_order)) = app.unregister_element_info(id) {
        app.remove_focusable_entry(tab_index, tree_order, id);
    }
    app.remove_bounds(id);
    spatial_unregister(id);
    app.invalidate_focus_order();
}

/// Sync element visibility to ElInfo for spatial grid O(1) checks.
pub fn set_elinfo_visible(id: ElementId, v: bool) {
    crate::core::app_context::current_app().set_elinfo_visible(id, v);
}

/// Read element visibility from ElInfo.
pub fn is_elinfo_visible(id: ElementId) -> bool {
    crate::core::app_context::current_app().is_elinfo_visible(id)
}

// ── Spatial index for O(log n) hit testing ─────────────────────────

pub fn register_bounds(id: ElementId, bounds: Rect) {
    crate::core::app_context::current_app().register_bounds(id, bounds);
}

pub fn update_bounds(id: ElementId, bounds: Rect) {
    crate::core::app_context::current_app().update_bounds(id, bounds);
}

pub fn unregister_bounds(id: ElementId) {
    crate::core::app_context::current_app().remove_bounds(id);
}

pub fn bounds_of(id: ElementId) -> Option<Rect> {
    crate::core::app_context::current_app().bounds_of(id)
}

/// Read an element's `reactive_visible` cell state, if it carries one.
/// Test/diagnostic observability for portal popups.
#[doc(hidden)]
pub fn reactive_visible_of(id: ElementId) -> Option<bool> {
    let app = crate::core::app_context::current_app();
    let reg = app.elements.el_registry.borrow();
    reg.get(&id)
        .and_then(|info| info.reactive_visible.as_ref().map(|c| c.get()))
}

pub fn query_bounds_at(point: Point) -> Vec<(ElementId, Rect)> {
    crate::core::app_context::current_app().query_bounds_at(point)
}

pub fn all_bounds() -> Vec<(ElementId, Rect)> {
    crate::core::app_context::current_app().all_bounds()
}

// ── Deferred tree mutations (for signal-driven mount/unmount) ──────

pub fn defer_action(
    f: impl Fn(&mut crate::core::element::ElementArena, crate::core::id::ElementId, &mut EventRegistry)
        + 'static,
) {
    let boxed: Box<
        dyn Fn(
            &mut crate::core::element::ElementArena,
            crate::core::id::ElementId,
            &mut EventRegistry,
        ),
    > = Box::new(f);
    crate::core::app_context::current_app().defer_action(boxed);
}

pub fn take_actions() -> Vec<
    Box<
        dyn Fn(
            &mut crate::core::element::ElementArena,
            crate::core::id::ElementId,
            &mut EventRegistry,
        ),
    >,
> {
    crate::core::app_context::current_app().take_actions()
}

/// Returns true if deferred tree mutations are queued for the next frame.
/// Deferred actions live only in the registry (not on any element Cell), so
/// callers checking element flags alone (e.g. `has_active_work`) miss them.
pub fn has_pending_actions() -> bool {
    crate::core::app_context::current_app().has_pending_actions()
}

pub fn mark_dirty(id: ElementId, flags: DirtyFlags) {
    #[cfg(debug_assertions)]
    trace_mark_probe(id, flags, "mark_dirty");
    record_dirty_event(id, flags);
    record_signal_element_link(id);
    #[cfg(feature = "devtools")]
    {
        let app = crate::core::app_context::current_app();
        if app.is_frozen() {
            app.push_frozen_dirty(id, flags);
            return;
        }
    }
    crate::core::app_context::current_app().mark_dirty(id, flags);
}

/// Mark every registered element as needing MEASURE, forcing a full
/// taffy tree rebuild on the next frame.  Used after a caught panic
/// to recover the layout state.
pub fn mark_all_dirty() {
    crate::core::app_context::current_app().mark_all_dirty();
}

pub fn affected_by_child_size(id: ElementId) -> bool {
    crate::core::app_context::current_app().affected_by_child_size(id)
}

pub fn set_affected_by_child_size(id: ElementId, v: bool) {
    crate::core::app_context::current_app().set_affected_by_child_size(id, v);
}

pub fn size_independent(id: ElementId) -> crate::ecs::components::AxisPair {
    crate::core::app_context::current_app().size_independent(id)
}
pub fn set_size_independent(id: ElementId, v: crate::ecs::components::AxisPair) {
    crate::core::app_context::current_app().set_size_independent(id, v);
}

pub fn has_solid_background(id: ElementId) -> bool {
    crate::core::app_context::current_app().has_solid_background(id)
}

pub fn subtree_gen_of(id: ElementId) -> u64 {
    crate::core::app_context::current_app().subtree_gen_of(id)
}

pub fn content_gen_of(id: ElementId) -> u64 {
    subtree_gen_of(id)
}

pub fn layout_gen_of(id: ElementId) -> u64 {
    crate::core::app_context::current_app().layout_gen_of(id)
}

pub fn bump_layout_gen_local(id: ElementId) {
    crate::core::app_context::current_app().bump_layout_gen_local(id);
}

pub fn bump_surface_gen_remote(id: ElementId) {
    crate::core::app_context::current_app().bump_surface_gen_remote(id);
}

pub fn bump_decor_gen_remote(id: ElementId) {
    crate::core::app_context::current_app().bump_decor_gen_remote(id);
}

pub fn bump_subtree_gen(id: ElementId) {
    crate::core::app_context::current_app().bump_subtree_gen(id);
}

/// Convenience: mark element for full repaint — sets dirty flag, registers for processing,
/// bumps both decor and subtree generations.
/// Use when an element's visual output has changed and must be re-rendered.
pub fn mark_widget_repaint(id: ElementId) {
    mark_dirty(id, DirtyFlags::REPAINT);
    register_dirty(id, DirtyFlags::REPAINT);
    bump_decor_gen_remote(id);
    bump_subtree_gen(id);
}

pub fn drain_pending_bumps() -> Vec<ElementId> {
    crate::core::app_context::current_app().drain_pending_bumps()
}

/// Bump only this element's subtree_gen without walking the ancestor chain.
/// Used by process_dirty_set which already walks ancestors in its own loop.
pub fn bump_subtree_gen_local(id: ElementId) {
    crate::core::app_context::current_app().bump_subtree_gen_local(id);
}

pub fn element_count() -> usize {
    crate::core::app_context::current_app().element_count()
}

/// Check whether the AppContext has any pending dirty entries.
/// Signal-driven register_dirty() writes to the dirty set but not to the
/// element Cell (unlike mark_repaint which writes both).  Callers
/// that only check element.needs_repaint() will miss signal-driven
/// dirty changes — use this to detect them.
pub fn has_pending_dirty() -> bool {
    !crate::core::app_context::current_app()
        .dirty
        .entries
        .borrow()
        .is_empty()
}

// ── O(1) Element state access ──────────────────────────────────────

/// O(1) read element state flags.
pub fn state_of(id: ElementId) -> StateFlags {
    crate::core::app_context::current_app().state_of(id)
}

/// O(1) add state flags to an element (used by event system for hover/focus/press).
pub fn add_state(id: ElementId, flag: StateFlags) {
    crate::core::app_context::current_app().add_state(id, flag);
}

/// O(1) remove state flags from an element.
pub fn remove_state(id: ElementId, flag: StateFlags) {
    crate::core::app_context::current_app().remove_state(id, flag);
}

/// O(1) set or clear a specific state flag.
pub fn set_state(id: ElementId, flag: StateFlags, on: bool) {
    crate::core::app_context::current_app().set_state(id, flag, on);
}

/// O(1) sync z_index into EL_REGISTRY. Called from Element::set_z_index.
pub fn set_elinfo_z_index(id: ElementId, v: i32) {
    crate::core::app_context::current_app().set_elinfo_z_index(id, v);
}

/// Walk up the ancestor chain; returns `true` if `id` or any parent has `slot_inactive == true`.
pub fn is_slot_inactive_in_ancestry(
    id: ElementId,
    arena: &crate::core::element::ElementArena,
) -> bool {
    if arena.get(id).is_some_and(|el| el.slot_inactive.get()) {
        return true;
    }
    let mut cur = parent_of(id);
    while let Some(pid) = cur {
        if arena.get(pid).is_some_and(|el| el.slot_inactive.get()) {
            return true;
        }
        cur = parent_of(pid);
    }
    false
}

/// Walk the ancestor chain; returns `true` if `id` or any ancestor has
/// `reactive_visible == false` (portal/popup is closed). Used by the
/// frame_tick pass to avoid running ticks on hidden overlay content.
pub fn is_reactive_hidden_in_ancestry(id: ElementId) -> bool {
    let app = crate::core::app_context::current_app();
    let reg = app.elements.el_registry.borrow();
    let mut cur = Some(id);
    while let Some(cid) = cur {
        if reg
            .get(&cid)
            .and_then(|info| info.reactive_visible.as_ref())
            .is_some_and(|c| !c.get())
        {
            return true;
        }
        cur = parent_of(cid);
    }
    false
}

/// Whether the element's visual rect is fully outside the window viewport
/// or a clipping scroll ancestor — the 4th animation visibility gate.
/// `viewport` is `(width, height)`; pass `(0.0, 0.0)` to skip the window
/// test (clip-ancestor test still applies). Conservative: unknown → false.
/// Arena-less (reads the thread-local component tables), so frame_tick
/// closures can call it directly.
pub fn is_offscreen(id: ElementId, viewport: (f32, f32)) -> bool {
    crate::core::app_context::current_app().spatial_is_offscreen(id, viewport.0, viewport.1)
}

/// O(1) check if an element has a specific state flag.
pub fn has_state(id: ElementId, flag: StateFlags) -> bool {
    crate::core::app_context::current_app().has_state(id, flag)
}

/// Walk up the ancestor chain; returns `true` if `id` or any parent has `StateFlags::DISABLED`.
/// Used by pointer handlers to suppress HOVERED/PRESSED on disabled subtrees.
pub fn is_element_or_ancestor_disabled(id: ElementId) -> bool {
    if has_state(id, StateFlags::DISABLED) {
        return true;
    }
    let mut cur = parent_of(id);
    while let Some(pid) = cur {
        if has_state(pid, StateFlags::DISABLED) {
            return true;
        }
        cur = parent_of(pid);
    }
    false
}

/// O(1) sync accepts_mouse into EL_REGISTRY. Called from Element::set_accepts_mouse.
pub fn set_elinfo_accepts_mouse(id: ElementId, v: bool) {
    let app = crate::core::app_context::current_app();
    let mut guard = app.elements.el_registry.borrow_mut();
    if let Some(info) = guard.get_mut(&id) {
        info.accepts_mouse = v;
    }
}

/// O(1) sync input_pass_through into EL_REGISTRY. Called from Element::set_input_pass_through.
pub fn set_elinfo_input_pass_through(id: ElementId, v: bool) {
    let app = crate::core::app_context::current_app();
    let mut guard = app.elements.el_registry.borrow_mut();
    if let Some(info) = guard.get_mut(&id) {
        info.input_pass_through = v;
    }
}

/// Mark a container element as structurally changed (children added/removed/slot_inactive flipped).
/// Drained at the start of the layout phase for targeted subtree rebuild.
pub fn mark_structurally_changed(eid: ElementId) {
    crate::core::app_context::current_app().mark_structurally_changed(eid);
}

/// Drain the structurally changed set. Returns all containers that need taffy subtree rebuild.
pub fn drain_structurally_changed() -> std::collections::HashSet<ElementId> {
    crate::core::app_context::current_app().drain_structurally_changed()
}

/// Check if any structural changes are pending.
pub fn has_structurally_changed() -> bool {
    crate::core::app_context::current_app().has_structurally_changed()
}

// ── S4: O(1) systemic check flags ──

/// Whether any exit animation is pending.
pub fn is_exit_pending_active() -> bool {
    crate::core::app_context::current_app().is_exit_pending_active()
}

/// Register an element with active exit animation.
pub fn register_exit(eid: ElementId) {
    crate::core::app_context::current_app().register_exit(eid);
}

/// Drain the exit list for processing. Returns all exit-pending elements.
pub fn drain_exits() -> Vec<ElementId> {
    crate::core::app_context::current_app().drain_exits()
}

/// Invalidate the cached focus order (call on mount/unmount/tab_index change).
pub fn invalidate_focus_order() {
    crate::core::app_context::current_app().invalidate_focus_order();
}

/// Register an element as focusable. Called from Element::set_focusable(true).
/// O(log n) BTreeSet insertion.
pub fn register_focusable(eid: ElementId, tab_index: Option<usize>, tree_order: u64) {
    crate::core::app_context::current_app().register_focusable(eid, tab_index, tree_order);
}

/// Unregister an element from the focusable set. O(log n) removal.
pub fn unregister_focusable(eid: ElementId, tab_index: Option<usize>, tree_order: u64) {
    crate::core::app_context::current_app().unregister_focusable(eid, tab_index, tree_order);
}

/// Sync tab_index into ElInfo for focus-order cleanup at unregister time.
pub fn set_elinfo_tab_index(eid: ElementId, v: Option<usize>) {
    let app = crate::core::app_context::current_app();
    let guard = app.elements.el_registry.borrow();
    if let Some(info) = guard.get(&eid) {
        info.tab_index.set(v);
    }
}

/// Sync reactive_visible into ElInfo so `is_visible_chain_fast` can
/// read it directly instead of going through ComponentTables.
pub fn set_elinfo_reactive_visible(eid: ElementId, v: std::rc::Rc<std::cell::Cell<bool>>) {
    let app = crate::core::app_context::current_app();
    let mut guard = app.elements.el_registry.borrow_mut();
    if let Some(info) = guard.get_mut(&eid) {
        info.reactive_visible = Some(v);
    }
}

/// Ensure focus order is cached and return it. O(k) from BTreeSet, O(1) cached.
/// The BTreeSet is now maintained incrementally by `register_focusable` /
/// `unregister_focusable` — no full-tree scan is ever needed.
pub fn ensure_focus_order(_arena: &crate::core::element::ElementArena) -> Vec<ElementId> {
    let app = crate::core::app_context::current_app();
    if app.focus_order_valid() {
        return app.focus_order_cached();
    }
    let ids = app.focusable_ids();
    app.set_focus_order(ids.clone());
    app.set_focus_order_valid(true);
    ids
}

/// Focus order from cached list + current focus element.
pub fn focus_order_from_cached(
    arena: &crate::core::element::ElementArena,
    current: Option<ElementId>,
) -> (Vec<ElementId>, usize) {
    let ids = ensure_focus_order(arena);
    let next_idx = match current {
        Some(id) => ids
            .iter()
            .position(|&eid| eid == id)
            .map_or(0, |i| (i + 1) % ids.len().max(1)),
        None => 0,
    };
    (ids, next_idx)
}

/// Iterate every registered focusable element.  The callback receives
/// `(element_id, tree_order)` for each entry in the focusable set.
pub fn visit_focusable(f: impl FnMut(ElementId, u64)) {
    crate::core::app_context::current_app().visit_focusable(f);
}

// ── S4: A11y dirty flag ──

// ── Flat dirty clear: O(k) targeted clear of dirty flags ──

/// Clear the specified dirty level for only the given set of element IDs.
/// Replaces O(N) `clear_dirty_subtree` tree recursion with O(k) set iteration.
pub fn clear_dirty_in_set(ids: &[ElementId], level: DirtyFlags) {
    crate::core::app_context::current_app().clear_dirty_in_set(ids, level);
}

pub fn mark_a11y_dirty() {
    crate::core::app_context::current_app().mark_a11y_dirty();
}
pub fn is_a11y_dirty() -> bool {
    crate::core::app_context::current_app().is_a11y_dirty()
}
pub fn clear_a11y_dirty() {
    crate::core::app_context::current_app().clear_a11y_dirty();
}

// ── S4: Spatial hash grid for O(1) hit testing ─────────────────

pub(crate) const SPATIAL_CELL_SIZE: f32 = 128.0;

#[derive(Clone, Copy)]
pub(crate) struct SpatialEntry {
    pub(crate) eid: ElementId,
    pub(crate) z_index: i32,
    pub(crate) tree_order: u64,
}

pub(crate) fn cells_covered_by(rect: Rect) -> Vec<(i32, i32)> {
    let half = SPATIAL_CELL_SIZE * 0.5;
    let x0 = ((rect.x - half) / SPATIAL_CELL_SIZE).floor() as i32;
    let y0 = ((rect.y - half) / SPATIAL_CELL_SIZE).floor() as i32;
    let x1 = ((rect.x + rect.width + half) / SPATIAL_CELL_SIZE).floor() as i32;
    let y1 = ((rect.y + rect.height + half) / SPATIAL_CELL_SIZE).floor() as i32;
    let mut cells = Vec::with_capacity(((x1 - x0 + 1) * (y1 - y0 + 1)) as usize);
    for cx in x0..=x1 {
        for cy in y0..=y1 {
            cells.push((cx, cy));
        }
    }
    cells
}

pub fn spatial_register(id: ElementId, new_bounds: Rect, tree_order: u64) {
    crate::core::app_context::current_app().spatial_register(id, new_bounds, tree_order);
}

pub fn spatial_unregister(id: ElementId) {
    crate::core::app_context::current_app().spatial_unregister(id);
}

/// Update the cached scroll offset for a spatial-grid entry.
/// Called from every `scroll_offset.set()` site so the hit-test
/// can read scroll offsets without an `arena.get()` call.
/// Also bumps the scroll tree gen to invalidate `accumulated_scroll_cached`.
pub fn spatial_update_scroll(eid: ElementId, ox: f32, oy: f32) {
    crate::core::app_context::current_app().spatial_update_scroll(eid, ox, oy);
}

/// Record that `eid` has a position_offset cell attached, so hit-testing
/// will gather its visual (offset) cell. Called from `set_position_offset`.
pub fn spatial_register_position_offset(eid: ElementId) {
    crate::core::app_context::current_app().spatial_register_position_offset(eid);
}

/// Read the element's live position_offset (the visual translation applied at
/// render time). Returns (0, 0) when no offset cell is attached.
pub(crate) fn read_pos_offset(
    arena: &crate::core::element::ElementArena,
    eid: ElementId,
) -> (f32, f32) {
    arena
        .get(eid)
        .and_then(|el| el.position_offset())
        .map(|c| {
            let v = c.get();
            (v.x, v.y)
        })
        .unwrap_or((0.0, 0.0))
}

/// Read the accumulated scroll offset (sum of scroll_offset up the
/// ancestor chain) with O(1) caching.
///
/// Each element caches `(accum_scroll_x, accum_scroll_y, accum_scroll_gen)`.
/// The global `SCROLL_TREE_GEN` is bumped on every `scroll_offset.set()`,
/// so the cache is lazily invalidated: the first hit-test after any scroll
/// change pays O(depth); subsequent hit-tests are O(1) until the next
/// scroll change.
pub fn accumulated_scroll_cached(
    arena: &crate::core::element::ElementArena,
    eid: ElementId,
) -> (f32, f32) {
    crate::core::app_context::current_app().spatial_accumulated_scroll_cached(arena, eid)
}

/// Map a screen-space point into the layout coordinate space of `eid`.
/// Subtracts `eid`'s own scroll offset (if any) from the accumulated scroll
/// so the result is relative to ancestor scrolls only — matching the element's
/// `screen_bounds` which are in layout space.
#[cfg(test)]
fn point_to_layout(
    arena: &crate::core::element::ElementArena,
    eid: ElementId,
    point: Point,
) -> Point {
    crate::core::app_context::current_app().spatial_point_to_layout(arena, eid, point)
}

pub(crate) fn is_visible_chain_fast(id: ElementId) -> bool {
    crate::core::app_context::current_app().spatial_is_visible_chain_fast(id)
}

#[cfg(test)]
fn is_within_scroll_clip(
    arena: &crate::core::element::ElementArena,
    eid: ElementId,
    point: Point,
) -> bool {
    let mut cur = eid;
    loop {
        match parent_of(cur) {
            Some(pid) => {
                if arena.comp_scroll(pid).is_some() {
                    let adj = point_to_layout(arena, pid, point);
                    if let Some(b) = bounds_of(pid) {
                        if !b.contains(adj) {
                            return false;
                        }
                    }
                }
                cur = pid;
            }
            None => return true,
        }
    }
}

pub fn spatial_hit_test(
    arena: &crate::core::element::ElementArena,
    point: Point,
) -> Option<ElementId> {
    crate::core::app_context::current_app().spatial_hit_test(arena, point)
}

pub fn hit_test_with_fallback(
    arena: &crate::core::element::ElementArena,
    point: Point,
) -> Option<ElementId> {
    spatial_hit_test(arena, point).or_else(|| arena.hit_test_leaf(point))
}

pub fn spatial_hit_scrollable(
    arena: &crate::core::element::ElementArena,
    point: Point,
) -> Option<ElementId> {
    crate::core::app_context::current_app().spatial_hit_scrollable(arena, point)
}

/// Return all scrollable elements at the given point, innermost first.
pub fn spatial_scroll_chain(
    arena: &crate::core::element::ElementArena,
    point: Point,
) -> Vec<ElementId> {
    crate::core::app_context::current_app().spatial_scroll_chain(arena, point)
}

#[cfg(debug_assertions)]
mod debug_stats {
    use std::cell::Cell;

    thread_local! {
        static DIRTY_COUNT: Cell<usize> = const { Cell::new(0) };
        static PROCESS_STEPS: Cell<usize> = const { Cell::new(0) };
        static HITTEST_LEAF_COUNT: Cell<usize> = const { Cell::new(0) };
    }

    pub fn inc_dirty_count() {
        DIRTY_COUNT.with(|c| c.set(c.get() + 1));
    }
    pub fn inc_process_step() {
        PROCESS_STEPS.with(|c| c.set(c.get() + 1));
    }
    pub fn inc_hittest_leaf_fallback() {
        HITTEST_LEAF_COUNT.with(|c| c.set(c.get() + 1));
    }

    pub fn reset() {
        DIRTY_COUNT.with(|c| c.set(0));
        PROCESS_STEPS.with(|c| c.set(0));
        HITTEST_LEAF_COUNT.with(|c| c.set(0));
    }

    pub fn snapshot() -> (usize, usize) {
        (
            DIRTY_COUNT.with(|c| c.get()),
            PROCESS_STEPS.with(|c| c.get()),
        )
    }

    pub fn hittest_leaf_fallback_count() -> usize {
        HITTEST_LEAF_COUNT.with(|c| c.get())
    }
}

#[cfg(debug_assertions)]
pub use debug_stats::{
    hittest_leaf_fallback_count, inc_dirty_count, inc_hittest_leaf_fallback, inc_process_step,
    reset as reset_stats, snapshot as stats,
};

// ── DevTools dirty counter (gated behind devtools feature) ──

#[cfg(feature = "devtools")]
mod devtools_dirty_counter {
    use std::cell::Cell;

    thread_local! {
        static COUNT: Cell<usize> = const { Cell::new(0) };
    }

    pub fn inc() {
        COUNT.with(|c| c.set(c.get() + 1));
    }

    pub fn read() -> usize {
        COUNT.with(|c| c.get())
    }

    pub fn reset() {
        COUNT.with(|c| c.set(0));
    }
}

/// Increment the DevTools dirty counter. Called from dirty propagation.
#[cfg(feature = "devtools")]
pub fn inc_devtools_dirty() {
    devtools_dirty_counter::inc();
}

/// Read current dirty count for this frame.
#[cfg(feature = "devtools")]
pub fn devtools_dirty_count() -> usize {
    devtools_dirty_counter::read()
}

/// Reset the DevTools dirty counter at the start of a frame.
#[cfg(feature = "devtools")]
pub fn devtools_reset_dirty() {
    devtools_dirty_counter::reset();
}

// Stub for non-devtools builds
#[cfg(not(feature = "devtools"))]
pub fn inc_devtools_dirty() {}
#[cfg(not(feature = "devtools"))]
pub fn devtools_dirty_count() -> usize {
    0
}
#[cfg(not(feature = "devtools"))]
pub fn devtools_reset_dirty() {}

// ── Freeze/resume API (DevTools UI inspection) ───────────────────

/// Freeze the current window's UI. All signal-driven dirty registrations are
/// queued. Call `unfreeze_ui()` to flush and resume.
#[cfg(feature = "devtools")]
pub fn freeze_ui() {
    crate::core::app_context::current_app().freeze();
}

/// Resume UI updates and flush all queued dirty registrations.
#[cfg(feature = "devtools")]
pub fn unfreeze_ui() {
    crate::core::app_context::current_app().unfreeze();
}

/// Check if the current window is frozen.
#[cfg(feature = "devtools")]
pub fn is_ui_frozen() -> bool {
    crate::core::app_context::current_app().is_frozen()
}

// ── Dirty event trace (DevTools dirty propagation panel) ──────────

thread_local! {
    static DIRTY_TRACE_ENABLED: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
    static DIRTY_TRACE_EVENTS: std::cell::RefCell<Vec<(ElementId, DirtyFlags, u64, DirtyTriggerTag)>> =
        const { std::cell::RefCell::new(Vec::new()) };
}

thread_local! {
    static CURRENT_TRIGGER: std::cell::Cell<DirtyTriggerTag> = const { std::cell::Cell::new(DirtyTriggerTag::Unknown) };
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DirtyTriggerTag {
    Unknown = 0,
    SignalSet = 1,
    PointerEvent = 2,
    FrameTick = 3,
    DeferredAction = 4,
    ChildDirty = 5,
    Animation = 6,
}

impl DirtyTriggerTag {
    #[allow(dead_code)]
    fn as_str(self) -> &'static str {
        match self {
            DirtyTriggerTag::Unknown => "Unknown",
            DirtyTriggerTag::SignalSet => "SignalSet",
            DirtyTriggerTag::PointerEvent => "PointerEvent",
            DirtyTriggerTag::FrameTick => "FrameTick",
            DirtyTriggerTag::DeferredAction => "DeferredAction",
            DirtyTriggerTag::ChildDirty => "ChildDirty",
            DirtyTriggerTag::Animation => "Animation",
        }
    }
}

pub fn set_current_trigger(tag: DirtyTriggerTag) {
    CURRENT_TRIGGER.with(|c| c.set(tag));
}

pub fn current_trigger() -> DirtyTriggerTag {
    CURRENT_TRIGGER.with(|c| c.get())
}

pub fn set_dirty_trace_enabled(enabled: bool) {
    DIRTY_TRACE_ENABLED.with(|c| c.set(enabled));
}

pub(crate) fn record_dirty_event(id: ElementId, flags: DirtyFlags) {
    if !DIRTY_TRACE_ENABLED.with(|c| c.get()) {
        return;
    }
    let us = web_time::Instant::now().elapsed().as_micros() as u64;
    let trigger = {
        #[cfg(feature = "devtools")]
        if crate::core::signal_bridge::read_current_signal_addr() != 0 {
            DirtyTriggerTag::SignalSet
        } else {
            current_trigger()
        }
        #[cfg(not(feature = "devtools"))]
        current_trigger()
    };
    DIRTY_TRACE_EVENTS.with(|events| {
        events.borrow_mut().push((id, flags, us, trigger));
    });
}

pub fn drain_dirty_events_raw() -> Vec<(ElementId, DirtyFlags, u64, DirtyTriggerTag)> {
    if !DIRTY_TRACE_ENABLED.with(|c| c.get()) {
        return Vec::new();
    }
    DIRTY_TRACE_EVENTS.with(|events| events.borrow_mut().drain(..).collect())
}

// ── Signal → Element causal link (DevTools) ────────────────────────

/// A single causal link: signal at `signal_addr` caused `element_id` to be
/// marked dirty during the current subscriber callback.
#[cfg(feature = "devtools")]
#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub struct SignalElementLink {
    pub signal_addr: usize,
    pub element_id: ElementId,
}

#[cfg(feature = "devtools")]
thread_local! {
    static SIGNAL_ELEMENT_LINKS: std::cell::RefCell<Vec<SignalElementLink>> =
        std::cell::RefCell::new(Vec::new());
}

/// Record a signal→element causal link for the current frame.
/// Called from `register_dirty` / `mark_dirty` / `register_animation`
/// when `CURRENT_NOTIFYING_SIGNAL` is non-zero.
#[cfg(feature = "devtools")]
pub(crate) fn record_signal_element_link(id: ElementId) {
    let signal_addr = crate::core::signal_bridge::read_current_signal_addr();
    if signal_addr != 0 {
        SIGNAL_ELEMENT_LINKS.with(|links| {
            links.borrow_mut().push(SignalElementLink {
                signal_addr,
                element_id: id,
            });
        });
    }
}

/// Drain all signal→element causal links collected this frame.
#[cfg(feature = "devtools")]
pub fn drain_signal_element_links() -> Vec<SignalElementLink> {
    SIGNAL_ELEMENT_LINKS.with(|links| links.borrow_mut().drain(..).collect())
}

// Stubs
#[cfg(not(feature = "devtools"))]
fn record_signal_element_link(_id: ElementId) {}
#[cfg(not(feature = "devtools"))]
pub fn drain_signal_element_links() -> Vec<SignalElementLink> {
    Vec::new()
}

// Also make the dummy struct available for not(feature = "devtools")
#[cfg(not(feature = "devtools"))]
#[derive(Clone, Debug)]
pub struct SignalElementLink {
    pub signal_addr: usize,
    pub element_id: ElementId,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dirty_flags_combine() {
        let f = DirtyFlags::REPAINT | DirtyFlags::REPOSITION;
        assert!(f.has_repaint());
        assert!(f.has_reposition());
        assert!(!f.has_measure());
    }

    #[test]
    fn registry_mark_and_clear() {
        let id = ElementId::allocate();
        register_dirty(id, DirtyFlags::REPAINT);
        assert!(has_pending_dirty());
        take_dirty();
        assert!(!has_pending_dirty());
    }

    // ── position_offset hit-test parity (render applies offset; hit-test must too) ──

    use crate::style::Vec2;
    use crate::testing::TestHarness;
    use crate::widgets::input::Button;
    use std::cell::Cell as StdCell;
    use std::rc::Rc as StdRc;

    /// Mount a single Button, run one frame, and return (harness, target_eid, bounds).
    /// `target_eid` is the deepest mouse-accepting element at the button's centre —
    /// discovered empirically so the test makes no assumption about Button internals.
    fn mount_button_target() -> (TestHarness, ElementId, Rect) {
        let mut h = TestHarness::new(1200.0, 200.0);
        let mounted = h.mount(Button::new("Drag"));
        h.run_frame();
        let mb = h
            .find(mounted)
            .map(|el| el.screen_bounds)
            .expect("mounted bounds");
        let centre = Point::new(mb.x + mb.width * 0.5, mb.y + mb.height * 0.5);
        let target = hit_test_with_fallback(&h.arena, centre).expect("baseline hit");
        let b = h
            .find(target)
            .map(|el| el.screen_bounds)
            .expect("target bounds");
        (h, target, b)
    }

    #[test]
    fn hit_test_leaf_follows_position_offset() {
        // Grid is NOT primed (harness never spatial_register's) → spatial_hit_test
        // returns None → hit_test_with_fallback exercises the LEAF path.
        let (mut h, target, b) = mount_button_target();
        let off = b.width + 250.0;
        h.find_mut(target)
            .unwrap()
            .set_position_offset(StdRc::new(StdCell::new(Vec2::new(off, 0.0))));

        let centre = Point::new(b.x + b.width * 0.5, b.y + b.height * 0.5);
        let visual = Point::new(centre.x + off, centre.y);

        // A click where the element is now DRAWN must hit it.
        assert_eq!(
            hit_test_with_fallback(&h.arena, visual),
            Some(target),
            "leaf: click at visual (offset) position should hit the element",
        );
        // A click where the element USED to be must no longer hit it.
        assert_ne!(
            hit_test_with_fallback(&h.arena, centre),
            Some(target),
            "leaf: click at original layout position should NOT hit the offset element",
        );
    }

    #[test]
    fn spatial_hit_test_follows_position_offset() {
        // Prime the spatial grid for the target only, so hit_test_with_fallback
        // resolves through spatial_hit_test (the production fast path).
        let (mut h, target, b) = mount_button_target();
        let tree_order = h.find(target).unwrap().tree_order;
        // The harness registers bounds at element-allocation time (when
        // screen_bounds is still zero) and never calls update_bounds, so prime
        // both ELEMENT_BOUNDS and the grid with the post-layout bounds —
        // exactly what window.rs does after taffy (update_bounds + spatial_register).
        register_bounds(target, b);
        spatial_register(target, b, tree_order);

        // Offset > SPATIAL_CELL_SIZE so the element's visual cell differs from its
        // registered (layout) cell → exercises the adjacent-cell 补查.
        let off = b.width + 250.0;
        assert!(off > SPATIAL_CELL_SIZE, "offset must cross a spatial cell");
        h.find_mut(target)
            .unwrap()
            .set_position_offset(StdRc::new(StdCell::new(Vec2::new(off, 0.0))));

        let centre = Point::new(b.x + b.width * 0.5, b.y + b.height * 0.5);
        let visual = Point::new(centre.x + off, centre.y);

        assert_eq!(
            spatial_hit_test(&h.arena, visual),
            Some(target),
            "spatial: visual hit requires adjacent-cell gather + offset-aware contains",
        );
        assert_ne!(
            spatial_hit_test(&h.arena, centre),
            Some(target),
            "spatial: original layout position should NOT hit the offset element",
        );
    }

    #[test]
    fn spatial_hit_test_skips_non_mouse_descendant() {
        // A mouse-accepting parent (e.g. a Table resize handle, z=1) with a
        // NON-mouse-accepting child (the visible bar) must resolve to the PARENT,
        // not the child. The descendant-refinement step must mirror the main loop's
        // accepts_mouse filter; otherwise the decorative child shadows the handle
        // and the parent's drag handlers never fire.
        use crate::widgets::display::{ColumnWidth, Table, TableColumn};
        use crate::widgets::layout::SizedBox;
        use auralis_signal::Signal;

        let rows = Signal::new((0..5).map(|i| format!("R{i}")).collect::<Vec<_>>());
        let mut h = TestHarness::new(600.0, 300.0);
        let mounted = h.mount(
            SizedBox::new().width(600.0).height(300.0).child(
                Table::new(rows)
                    .columns(vec![
                        TableColumn::new("A", ColumnWidth::Fixed(120.0))
                            .render(|r: &String, _, _| r.clone())
                            .resizable()
                            .min_width(40.0),
                        TableColumn::new("B", ColumnWidth::Fixed(120.0))
                            .render(|_: &String, ri, _| format!("{ri}")),
                    ])
                    .row_height(28.0),
            ),
        );
        h.run_frame();

        // mounted → container → header_row → [cell0, cell1, handle]; handle → [bar]
        let container = h.find(mounted).unwrap().children[0];
        let header_row = h.find(container).unwrap().children[0];
        let handle = h.find(header_row).unwrap().children[2];
        let bar = h.find(handle).unwrap().children[0];

        // Prime the grid for the handle (z=1) and its bar child (no mouse), as the
        // real window does after taffy. (The harness itself never spatial_registers,
        // so without this spatial_hit_test would find no candidates.)
        for eid in [handle, bar] {
            let bnd = h.find(eid).unwrap().screen_bounds;
            let to = h.find(eid).unwrap().tree_order;
            register_bounds(eid, bnd);
            spatial_register(eid, bnd, to);
        }

        let hb = h.find(bar).unwrap().screen_bounds;
        let centre = Point::new(hb.x + hb.width * 0.5, hb.y + hb.height * 0.5);

        assert_eq!(
            spatial_hit_test(&h.arena, centre),
            Some(handle),
            "spatial hit on the handle must return the handle, not its non-mouse bar child",
        );
    }

    // ═══════════════════════════════════════════════════════════════
    // Scroll-aware hit-test tests
    // ═══════════════════════════════════════════════════════════════

    /// Helper: allocate elements, set up scroll component + parent relationships.
    fn setup_scroll_container_with_items(
        h: &mut TestHarness,
    ) -> (ElementId, ElementId, ElementId, Rc<Cell<Vec2>>) {
        crate::core::element::set_component_tables(h.arena.component_tables.clone());
        let scroll_eid = h.arena.allocate();
        let item_a = h.arena.allocate();
        let item_b = h.arena.allocate();

        register_parent(item_a, scroll_eid);
        register_parent(item_b, scroll_eid);

        // Scroll container at layout (100, 200), 300x500 viewport
        let scroll_bounds = Rect::new(100.0, 200.0, 300.0, 500.0);
        register_bounds(scroll_eid, scroll_bounds);
        spatial_register(scroll_eid, scroll_bounds, 0);
        register_element_minimal(scroll_eid, true, true, 0);

        // Items are registered later by each test with their own bounds
        register_element_minimal(item_a, true, true, 1);
        register_element_minimal(item_b, true, true, 2);

        let scroll_offset: Rc<Cell<Vec2>> = Rc::new(Cell::new(Vec2::ZERO));
        crate::core::element::with_ct_mut(|ct| {
            ct.scroll.entry(scroll_eid).or_default().scroll_offset = scroll_offset.clone();
        });

        (scroll_eid, item_a, item_b, scroll_offset)
    }

    /// Minimal EL_REGISTRY entry for hit-test visibility checks.
    /// Writes to AppContext (when available) or thread_local fallback.
    fn register_element_minimal(
        eid: ElementId,
        visible: bool,
        accepts_mouse: bool,
        tree_order: u64,
    ) {
        fn make_info(visible: bool, accepts_mouse: bool, tree_order: u64) -> ElInfo {
            ElInfo {
                dirty: Rc::new(Cell::new(DirtyFlags::CLEAN)),
                state: Rc::new(Cell::new(StateFlags::NONE)),
                affected_by_child_size: false,
                size_independent: crate::ecs::components::AxisPair::BOTH_DEP,
                has_solid_background: false,
                subtree_gen: Rc::new(Cell::new(0)),
                layout_gen: Rc::new(Cell::new(0)),
                surface_gen: None,
                decor_gen: None,
                z_index: 0,
                accepts_mouse,
                input_pass_through: false,
                visible,
                slot_inactive: None,
                reactive_visible: None,
                accum_scroll_x: Cell::new(0.0),
                accum_scroll_y: Cell::new(0.0),
                accum_scroll_gen: Cell::new(0),
                tree_order,
                tab_index: Cell::new(None),
            }
        }
        crate::core::app_context::current_app()
            .elements
            .el_registry
            .borrow_mut()
            .entry(eid)
            .or_insert_with(|| make_info(visible, accepts_mouse, tree_order));
    }

    #[test]
    fn point_to_layout_includes_ancestor_scroll() {
        let mut h = TestHarness::new(800.0, 800.0);
        let (scroll_eid, item_a, _, scroll_offset) = setup_scroll_container_with_items(&mut h);

        // Item A inside the scroll container at layout y=300
        let bounds_a = Rect::new(100.0, 300.0, 280.0, 40.0);
        register_bounds(item_a, bounds_a);
        spatial_register(item_a, bounds_a, 1);

        // No scroll: screen point at item A's layout position maps back 1:1
        let visual = Point::new(150.0, 320.0);
        let layout = point_to_layout(&h.arena, item_a, visual);
        assert!((layout.x - 150.0).abs() < 0.1);
        assert!((layout.y - 320.0).abs() < 0.1);

        // Scroll down by 200 → item A moves UP visually by 200.
        // A click at visual y = 320 − 200 = 120 maps back to layout y = 120+200 = 320.
        scroll_offset.set(Vec2::new(0.0, 200.0));
        spatial_update_scroll(scroll_eid, 0.0, 200.0);

        let layout_after = point_to_layout(&h.arena, item_a, Point::new(150.0, 120.0));
        assert!(
            (layout_after.y - 320.0).abs() < 1.0,
            "after 200px scroll, visual y=120 should map to layout y=320, got {:.0}",
            layout_after.y
        );
    }

    #[test]
    fn is_within_scroll_clip_handles_outer_scroll() {
        let mut h = TestHarness::new(800.0, 600.0);
        crate::core::element::set_component_tables(h.arena.component_tables.clone());

        // outer_scroll = page-level scroll (root, bounds cover the screen)
        let outer_scroll = h.arena.allocate();
        // inner_scroll = widget-level scroll (inside the page)
        let inner_scroll = h.arena.allocate();
        let item = h.arena.allocate();

        register_parent(inner_scroll, outer_scroll);
        register_parent(item, inner_scroll);

        // Outer scroll at root layout (0,0), 800x600 viewport (full screen)
        let outer_bounds = Rect::new(0.0, 0.0, 800.0, 600.0);
        register_bounds(outer_scroll, outer_bounds);
        spatial_register(outer_scroll, outer_bounds, 0);
        register_element_minimal(outer_scroll, true, true, 0);
        let outer_so: Rc<Cell<Vec2>> = Rc::new(Cell::new(Vec2::new(0.0, 5000.0)));
        crate::core::element::with_ct_mut(|ct| {
            ct.scroll.entry(outer_scroll).or_default().scroll_offset = outer_so.clone();
        });
        spatial_update_scroll(outer_scroll, 0.0, 5000.0);

        // Inner scroll at layout y=5100 inside the page, 400px viewport
        let inner_bounds = Rect::new(0.0, 5100.0, 400.0, 400.0);
        register_bounds(inner_scroll, inner_bounds);
        spatial_register(inner_scroll, inner_bounds, 1);
        register_element_minimal(inner_scroll, true, true, 1);
        let inner_so: Rc<Cell<Vec2>> = Rc::new(Cell::new(Vec2::new(0.0, 100.0)));
        crate::core::element::with_ct_mut(|ct| {
            ct.scroll.entry(inner_scroll).or_default().scroll_offset = inner_so.clone();
        });
        spatial_update_scroll(inner_scroll, 0.0, 100.0);

        // Item at layout y=5200 (100px into the inner scroll)
        let item_bounds = Rect::new(0.0, 5200.0, 400.0, 40.0);
        register_bounds(item, item_bounds);
        spatial_register(item, item_bounds, 2);
        register_element_minimal(item, true, true, 2);

        // After outer(-5000)+inner(-100) scroll:
        //   inner scroll appears at y=5100-5000=100 on screen
        //   item appears at y=5200-5000-100=100 on screen
        // Screen point (50,150) is within inner scroll visible area (y=100..500)

        let layout = point_to_layout(&h.arena, item, Point::new(50.0, 150.0));
        assert!(
            (layout.y - 5250.0).abs() < 10.0,
            "nested scroll: point_to_layout y should be ~5250, got {:.0}",
            layout.y
        );

        assert!(
            is_within_scroll_clip(&h.arena, item, Point::new(50.0, 150.0)),
            "click inside nested scroll visible region should pass clip check"
        );
    }

    #[test]
    fn spatial_grid_finds_scrolled_items_via_adjacent_cell() {
        let mut h = TestHarness::new(800.0, 800.0);
        let (scroll_eid, item_a, item_b, scroll_offset) = setup_scroll_container_with_items(&mut h);

        // Items deeper inside the scroll container so they stay visible after
        // a scroll large enough for adjacent-cell lookup (> SPATIAL_CELL_SIZE=128).
        let bounds_a = Rect::new(100.0, 500.0, 280.0, 40.0);
        let bounds_b = Rect::new(100.0, 550.0, 280.0, 40.0);
        register_bounds(item_a, bounds_a);
        spatial_register(item_a, bounds_a, 1);
        register_bounds(item_b, bounds_b);
        spatial_register(item_b, bounds_b, 2);

        // Baseline: no scroll — items at their layout positions
        assert_eq!(
            spatial_hit_test(&h.arena, Point::new(150.0, 520.0)),
            Some(item_a),
            "baseline: click on item A at layout position"
        );

        // Scroll down by 300 → item A visual y = 500−300 = 200, item B = 550−300 = 250
        scroll_offset.set(Vec2::new(0.0, 300.0));
        spatial_update_scroll(scroll_eid, 0.0, 300.0);

        assert_eq!(
            spatial_hit_test(&h.arena, Point::new(150.0, 220.0)),
            Some(item_a),
            "after 300px scroll: click at item A visual pos should hit item A"
        );
        assert_eq!(
            spatial_hit_test(&h.arena, Point::new(150.0, 270.0)),
            Some(item_b),
            "after 300px scroll: click at item B visual pos should hit item B"
        );

        // Click above viewport → must NOT match
        assert_ne!(
            spatial_hit_test(&h.arena, Point::new(150.0, 150.0)),
            Some(item_a),
            "click above scroll viewport should not hit items"
        );
    }
}
