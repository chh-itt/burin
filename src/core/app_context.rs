//! Explicit process-level application context. Replaces A-class thread_local
//! state. See docs/superpowers/specs/2026-07-07-explicit-context-design.md.
use crate::core::config::StateFlags;
#[cfg(feature = "devtools")]
use crate::core::dirty_registry::record_signal_element_link;
use crate::core::dirty_registry::{record_dirty_event, ElInfo, SpatialEntry};
use crate::core::element::{DirtyFlags, ElementId};
use crate::core::frame_pipeline::FramePhase;
use crate::ecs::active::ActiveTag;
use crate::event::EventRegistry;
use crate::style::{Point, Rect};
use rustc_hash::{FxHashMap, FxHashSet};
use std::cell::{Cell, RefCell};
use std::collections::BTreeSet;
use std::rc::{Rc, Weak};

thread_local! {
    /// TEMPORARY bridge (Layer 1-4). Set by window/harness on frame entry &
    /// mount. Lets legacy free-functions forward to the active AppContext
    /// without changing 320 call sites yet. DELETED in Layer 5.
    static CURRENT_APP: RefCell<Weak<AppContext>> = const { RefCell::new(Weak::new()) };
}

thread_local! {
    /// Lazily-created process-level default AppContext. Used when no window/
    /// harness has installed a current app (bare `dirty_registry::*` calls,
    /// early startup, lightweight unit tests). This is the ONE permitted
    /// thread_local for context state — and it holds a *unified* AppContext,
    /// not scattered per-concern state. Analogous to `with_ct`'s auto-init.
    static DEFAULT_APP: RefCell<Option<Rc<AppContext>>> = const { RefCell::new(None) };
}

/// Install the active AppContext for the current thread.
pub fn set_current_app(app: &Rc<AppContext>) {
    CURRENT_APP.with(|c| *c.borrow_mut() = Rc::downgrade(app));
}

/// Infallible accessor: the currently-installed app, or the lazily-created
/// process default. All `dirty_registry::*` free functions route through this,
/// so they never need a thread_local fallback branch.
pub fn current_app() -> Rc<AppContext> {
    if let Some(app) = CURRENT_APP.with(|c| c.borrow().upgrade()) {
        return app;
    }
    DEFAULT_APP.with(|d| {
        let mut slot = d.borrow_mut();
        if let Some(app) = slot.as_ref() {
            return app.clone();
        }
        let app = Rc::new(AppContext::new());
        *slot = Some(app.clone());
        app
    })
}

/// Run `f` with the active AppContext, if any. Legacy free-functions call this.
pub fn with_current_app<R>(f: impl FnOnce(&AppContext) -> R) -> Option<R> {
    CURRENT_APP.with(|c| c.borrow().upgrade().map(|rc| f(&rc)))
}

/// Upgrade the current weak ref, returning the Rc so the caller can call
/// methods without boxing a move-into-closure problem.
pub fn try_with_current_app() -> Option<Rc<AppContext>> {
    CURRENT_APP.with(|c| c.borrow().upgrade())
}

/// Intent-queue domain: written from signal/event callbacks, drained per-frame.
#[derive(Default)]
pub(crate) struct DirtyDomain {
    pub(crate) entries: RefCell<Vec<(ElementId, DirtyFlags)>>,
    pub(crate) index: RefCell<FxHashMap<ElementId, usize>>,
    pub(crate) animation: RefCell<FxHashMap<ElementId, DirtyFlags>>,
    pub(crate) pending_bumps: RefCell<FxHashSet<ElementId>>,
    pub(crate) structurally_changed: RefCell<std::collections::HashSet<ElementId>>,
    pub(crate) batch_depth: Cell<u32>,
    pub(crate) redraw_sent: Cell<bool>,
    /// Per-window wake callback (audit 2026-07-18 multi-window pass).
    /// The window installs `request_redraw` here ONCE; every
    /// `register_dirty` on this AppContext wakes THIS window. Replaces the
    /// process-global `ON_DIRTY` thread_local whose last writer captured
    /// every other window's wakes.
    pub(crate) on_dirty: RefCell<Option<Rc<dyn Fn()>>>,
    /// Dirty entries whose elements belong to ANOTHER window's arena.
    /// `process_dirty_phase` parks them here; `App::about_to_wait`
    /// redistributes them to the owning window (and wakes it). Empty in
    /// single-window apps — zero steady-state cost.
    pub(crate) foreign: RefCell<Vec<(ElementId, DirtyFlags)>>,
}

/// Element-data domain: read/written immediately from both frame & callback stacks.
#[derive(Default)]
pub(crate) struct ElementDomain {
    pub(crate) el_registry: RefCell<FxHashMap<ElementId, ElInfo>>,
    pub(crate) parent_map: RefCell<FxHashMap<ElementId, ElementId>>,
    pub(crate) bounds: RefCell<FxHashMap<ElementId, Rect>>,
    pub(crate) component_tables:
        RefCell<Option<std::rc::Rc<std::cell::RefCell<crate::ecs::tables::ComponentTables>>>>,
}

/// Focus domain (focusable set; cached focus_order Vec).
#[derive(Default)]
pub(crate) struct FocusDomain {
    pub(crate) focusable_set: RefCell<BTreeSet<(std::cmp::Reverse<Option<usize>>, u64, ElementId)>>,
    pub(crate) focus_order_valid: Cell<bool>,
    /// Cached focus-order list (Layer 2 migrated from thread_local).
    pub(crate) focus_order: RefCell<Vec<ElementId>>,
}

/// Spatial hash-grid domain for O(1) hit testing.
#[derive(Default)]
pub(crate) struct SpatialDomain {
    /// Spatial hash grid: (cell_x, cell_y) → sorted entries (z_index↓, tree_order↓)
    pub(crate) grid: RefCell<FxHashMap<(i32, i32), Vec<SpatialEntry>>>,
    /// Reverse index: ElementId → cell keys it currently occupies
    pub(crate) element_cells: RefCell<FxHashMap<ElementId, Vec<(i32, i32)>>>,
    /// Scroll offsets cached alongside the spatial grid (hit-test use)
    pub(crate) scroll_offsets: RefCell<FxHashMap<ElementId, (f32, f32)>>,
    /// Element ids with position_offset cells attached
    pub(crate) position_offsets: RefCell<FxHashSet<ElementId>>,
    /// Monotonically increasing generation counter for scroll changes
    pub(crate) scroll_tree_gen: Cell<u64>,
}

/// Widget interaction-arbitration state: transient "one operation in progress"
/// coordination that spans widget instances (drag elevation, open-menu chain,
/// hovered submenu). These are per-window (correctly isolated in AppContext):
/// written from a window's event callbacks, read by that window's frame loop.
/// Only holds ElementId/primitive types so `core` needs no `widgets` dependency.
#[derive(Default)]
pub(crate) struct InteractionDomain {
    /// Reorder drag: request to elevate/restore a dragged row's render layer.
    /// (ElementId, elevate). Drained by the window loop each frame.
    pub(crate) drag_z_request: Cell<Option<(ElementId, bool)>>,
    /// ContextMenu: the currently-open menu chain (root → nested submenus).
    pub(crate) menu_chain: RefCell<Vec<ElementId>>,
    /// ContextMenu: the currently-hovered submenu-parent row + hover start time.
    pub(crate) hovered_submenu: RefCell<Option<(ElementId, std::time::Instant)>>,
    /// ContextMenu: timestamp (ms) the most recent submenu opened.
    pub(crate) submenu_open_time: Cell<u64>,
}

/// Global resources (mostly write-once, read-many).
pub(crate) struct ResourceDomain {
    pub(crate) deferred_actions: RefCell<
        Vec<Box<dyn Fn(&mut crate::core::element::ElementArena, ElementId, &mut EventRegistry)>>,
    >,
    // image_registry / ecs tracking sets migrated in Layer 1
    pub(crate) drag_layout_elements: RefCell<std::collections::HashSet<ElementId>>,
    pub(crate) theme_elements: RefCell<std::collections::HashSet<ElementId>>,
    pub(crate) pending_scroll_elements: RefCell<std::collections::HashSet<ElementId>>,
    pub(crate) scrollable_elements: RefCell<std::collections::HashSet<ElementId>>,
    pub(crate) a11y_changed_elements: RefCell<std::collections::HashSet<ElementId>>,
    pub(crate) mount_callbacks: RefCell<Vec<ElementId>>,
    pub(crate) pending_cache_evictions: RefCell<Vec<ElementId>>,
    /// Elements torn down since the last frame — their EventRegistry
    /// handlers are removed by the frame driver (audit 2026-07-16, F1).
    pub(crate) pending_handler_removals: RefCell<Vec<ElementId>>,
    pub(crate) pending_cache_clear_all: Cell<bool>,
    /// Elements with active exit animations (process_exits target).
    pub(crate) exit_list: RefCell<Vec<ElementId>>,
    /// Current frame pipeline phase (None/Prepass/Layout/Paint).
    pub(crate) current_phase: Cell<FramePhase>,
    /// Accessibility tree needs rebuild.
    pub(crate) a11y_dirty: Cell<bool>,
    /// Active-set tracking (replaces O(k) component-table scans in the prepass).
    /// Maps `ActiveTag` → set of `ElementId`s that need per-frame processing.
    pub(crate) active_sets: RefCell<FxHashMap<ActiveTag, std::collections::HashSet<ElementId>>>,
}

impl Default for ResourceDomain {
    fn default() -> Self {
        ResourceDomain {
            deferred_actions: RefCell::new(Vec::new()),
            drag_layout_elements: RefCell::new(std::collections::HashSet::new()),
            theme_elements: RefCell::new(std::collections::HashSet::new()),
            pending_scroll_elements: RefCell::new(std::collections::HashSet::new()),
            scrollable_elements: RefCell::new(std::collections::HashSet::new()),
            a11y_changed_elements: RefCell::new(std::collections::HashSet::new()),
            mount_callbacks: RefCell::new(Vec::new()),
            pending_cache_evictions: RefCell::new(Vec::new()),
            pending_handler_removals: RefCell::new(Vec::new()),
            pending_cache_clear_all: Cell::new(false),
            exit_list: RefCell::new(Vec::new()),
            current_phase: Cell::new(FramePhase::None),
            a11y_dirty: Cell::new(true),
            active_sets: RefCell::new(FxHashMap::default()),
        }
    }
}

pub struct AppContext {
    pub(crate) dirty: DirtyDomain,
    pub(crate) elements: ElementDomain,
    pub(crate) focus: FocusDomain,
    pub(crate) resources: ResourceDomain,
    pub(crate) spatial: SpatialDomain,
    pub(crate) interaction: InteractionDomain,
    pub gesture: RefCell<crate::event::recognizer::GestureDomain>,
    /// Per-window widget-domain extensions (audit 2026-07-18 multi-window
    /// pass). ECS-resource pattern: widget/platform modules that used to
    /// keep process-global `thread_local!` state (overlay stack, toast
    /// queue, portal registries, form validators, scroll simulations,
    /// sticky entries…) store a `Default`-constructed domain object here,
    /// keyed by type. Keeps `core` free of reverse dependencies while
    /// giving every window its own isolated instance.
    extensions: RefCell<FxHashMap<std::any::TypeId, Rc<dyn std::any::Any>>>,
    /// When true, signal-driven dirty registrations are queued instead of
    /// processed, and frame processing skips expensive work (layout, paint,
    /// animations). Used by devtools for per-window UI freeze/resume.
    freeze: Cell<bool>,
    /// Pending dirty registrations accumulated while frozen. Drained and
    /// flushed to `register_dirty` on unfreeze.
    pending_frozen_dirty: RefCell<Vec<(ElementId, DirtyFlags)>>,
}

impl AppContext {
    pub fn new() -> Self {
        AppContext {
            dirty: DirtyDomain::default(),
            elements: ElementDomain::default(),
            focus: FocusDomain::default(),
            resources: ResourceDomain::default(),
            spatial: SpatialDomain::default(),
            interaction: InteractionDomain::default(),
            gesture: RefCell::new(crate::event::recognizer::GestureDomain::default()),
            extensions: RefCell::new(FxHashMap::default()),
            freeze: Cell::new(false),
            pending_frozen_dirty: RefCell::new(Vec::new()),
        }
    }

    /// Fetch (lazily creating) this app's instance of a widget-domain
    /// extension. `T` is the module's domain struct; the first access on a
    /// given AppContext constructs it via `Default`.
    pub fn extension<T: Default + 'static>(&self) -> Rc<T> {
        let key = std::any::TypeId::of::<T>();
        if let Some(existing) = self.extensions.borrow().get(&key) {
            if let Ok(typed) = existing.clone().downcast::<T>() {
                return typed;
            }
        }
        let fresh: Rc<T> = Rc::new(T::default());
        self.extensions
            .borrow_mut()
            .insert(key, fresh.clone() as Rc<dyn std::any::Any>);
        fresh
    }

    // ── Freeze/resume support ──

    /// Whether this window's UI is currently frozen. When frozen, signal-driven
    /// dirty registrations are queued and frame processing skips layout/paint.
    pub fn is_frozen(&self) -> bool {
        self.freeze.get()
    }

    /// Freeze this window's UI. All signal-driven dirty registrations are
    /// queued in `pending_frozen_dirty` instead of being processed immediately.
    /// Frame processing (layout, paint, animations) is skipped.
    pub fn freeze(&self) {
        self.freeze.set(true);
    }

    /// Resume UI updates. Drains all queued dirty registrations and triggers
    /// one repaint so the accumulated changes are reflected.
    pub fn unfreeze(&self) {
        self.freeze.set(false);
        let pending = std::mem::take(&mut *self.pending_frozen_dirty.borrow_mut());
        for (eid, flags) in pending {
            self.register_dirty(eid, flags);
        }
    }

    /// Queue a dirty registration while frozen. These are drained on unfreeze.
    pub fn push_frozen_dirty(&self, eid: ElementId, flags: DirtyFlags) {
        self.pending_frozen_dirty.borrow_mut().push((eid, flags));
    }

    pub fn register_dirty(&self, id: ElementId, flags: DirtyFlags) {
        // DevTools: record dirty event for the propagation panel
        record_dirty_event(id, flags);
        #[cfg(feature = "devtools")]
        {
            record_signal_element_link(id);
            if self.is_frozen() {
                self.push_frozen_dirty(id, flags);
                return;
            }
        }
        {
            let mut map = self.dirty.index.borrow_mut();
            let mut vec = self.dirty.entries.borrow_mut();
            if let Some(&pos) = map.get(&id) {
                vec[pos].1 |= flags;
            } else {
                map.insert(id, vec.len());
                vec.push((id, flags));
            }
        }
        self.notify_dirty();
    }

    /// Install this window's wake callback (typically `request_redraw`).
    /// Invoked at most once per event-loop turn per window — the
    /// `redraw_sent` gate coalesces bursts; `reset_dirty_redraw` reopens it.
    pub fn set_on_dirty(&self, cb: Rc<dyn Fn()>) {
        *self.dirty.on_dirty.borrow_mut() = Some(cb);
    }

    /// Fire the wake callback if the gate is open (borrow-free invocation:
    /// the callback may itself register dirty / replace the callback).
    ///
    /// Suppressed while THIS app is inside its own frame (`current_phase !=
    /// None`): the running frame drains/re-checks entries itself and the
    /// window's `needs_redraw` covers leftovers — waking mid-frame would
    /// schedule a spurious extra frame. Cross-window wakes are unaffected
    /// (the other window is not mid-frame on this stack).
    pub(crate) fn notify_dirty(&self) {
        if self.dirty.batch_depth.get() != 0 {
            return;
        }
        if self.resources.current_phase.get() != FramePhase::None {
            return;
        }
        if self.dirty.redraw_sent.replace(true) {
            return;
        }
        let cb = self.dirty.on_dirty.borrow().clone();
        if let Some(cb) = cb {
            cb();
        }
    }

    /// Park entries that belong to another window's arena (multi-window
    /// redistribution protocol — see `process_dirty_phase`).
    pub(crate) fn park_foreign_dirty(&self, entries: Vec<(ElementId, DirtyFlags)>) {
        if entries.is_empty() {
            return;
        }
        self.dirty.foreign.borrow_mut().extend(entries);
    }

    /// Drain entries parked for other windows. `App::about_to_wait` routes
    /// them to the AppContext whose arena owns each element.
    #[doc(hidden)]
    pub fn take_foreign_dirty(&self) -> Vec<(ElementId, DirtyFlags)> {
        std::mem::take(&mut *self.dirty.foreign.borrow_mut())
    }

    /// `true` when foreign entries are waiting for redistribution.
    pub(crate) fn has_foreign_dirty(&self) -> bool {
        !self.dirty.foreign.borrow().is_empty()
    }

    pub fn register_animation(&self, id: ElementId, flags: DirtyFlags) {
        self.dirty
            .animation
            .borrow_mut()
            .entry(id)
            .and_modify(|existing| *existing |= flags)
            .or_insert(flags);
    }

    pub fn take_dirty(&self) -> Vec<(ElementId, DirtyFlags)> {
        let mut entries = std::mem::take(&mut *self.dirty.entries.borrow_mut());
        self.dirty.index.borrow_mut().clear();
        for (id, flags) in std::mem::take(&mut *self.dirty.animation.borrow_mut()) {
            entries.push((id, flags));
        }
        entries
    }

    pub fn bump_subtree_gen(&self, id: ElementId) {
        self.dirty.pending_bumps.borrow_mut().insert(id);
    }

    pub fn drain_pending_bumps(&self) -> Vec<ElementId> {
        std::mem::take(&mut *self.dirty.pending_bumps.borrow_mut())
            .into_iter()
            .collect()
    }

    pub fn mark_structurally_changed(&self, eid: ElementId) {
        self.dirty.structurally_changed.borrow_mut().insert(eid);
    }

    pub fn drain_structurally_changed(&self) -> std::collections::HashSet<ElementId> {
        std::mem::take(&mut *self.dirty.structurally_changed.borrow_mut())
    }

    pub fn has_structurally_changed(&self) -> bool {
        !self.dirty.structurally_changed.borrow().is_empty()
    }

    pub fn begin_mount_batch(&self) {
        self.dirty.batch_depth.set(self.dirty.batch_depth.get() + 1);
    }

    pub fn end_mount_batch(&self) -> u32 {
        let v = self.dirty.batch_depth.get();
        if v > 0 {
            self.dirty.batch_depth.set(v - 1);
        }
        v
    }

    pub fn reset_dirty_redraw(&self) {
        self.dirty.redraw_sent.set(false);
    }

    // ── Deferred actions ──

    pub fn defer_action(
        &self,
        f: Box<dyn Fn(&mut crate::core::element::ElementArena, ElementId, &mut EventRegistry)>,
    ) {
        self.resources.deferred_actions.borrow_mut().push(f);
    }

    pub fn take_actions(
        &self,
    ) -> Vec<Box<dyn Fn(&mut crate::core::element::ElementArena, ElementId, &mut EventRegistry)>>
    {
        std::mem::take(&mut *self.resources.deferred_actions.borrow_mut())
    }

    pub fn has_pending_actions(&self) -> bool {
        !self.resources.deferred_actions.borrow().is_empty()
    }

    // ── ECS tracking sets ──

    pub fn register_drag_element(&self, eid: ElementId) {
        self.resources.drag_layout_elements.borrow_mut().insert(eid);
    }

    pub fn drain_drag_elements(&self) -> std::collections::HashSet<ElementId> {
        std::mem::take(&mut *self.resources.drag_layout_elements.borrow_mut())
    }

    pub fn register_theme_element(&self, eid: ElementId) {
        self.resources.theme_elements.borrow_mut().insert(eid);
    }

    pub fn drain_theme_elements(&self) -> std::collections::HashSet<ElementId> {
        std::mem::take(&mut *self.resources.theme_elements.borrow_mut())
    }

    pub fn register_pending_scroll(&self, eid: ElementId) {
        self.resources
            .pending_scroll_elements
            .borrow_mut()
            .insert(eid);
    }

    pub fn unregister_pending_scroll(&self, eid: ElementId) {
        self.resources
            .pending_scroll_elements
            .borrow_mut()
            .remove(&eid);
    }

    pub fn pending_scroll_elements(&self) -> Vec<ElementId> {
        self.resources
            .pending_scroll_elements
            .borrow()
            .iter()
            .copied()
            .collect()
    }

    pub fn register_scrollable(&self, eid: ElementId) {
        self.resources.scrollable_elements.borrow_mut().insert(eid);
    }

    pub fn unregister_scrollable(&self, eid: ElementId) {
        self.resources.scrollable_elements.borrow_mut().remove(&eid);
    }

    pub fn scrollable_elements(&self) -> Vec<ElementId> {
        self.resources
            .scrollable_elements
            .borrow()
            .iter()
            .copied()
            .collect()
    }

    pub fn mark_a11y_changed(&self, eid: ElementId) {
        self.resources
            .a11y_changed_elements
            .borrow_mut()
            .insert(eid);
    }

    pub fn drain_a11y_changed(&self) -> std::collections::HashSet<ElementId> {
        std::mem::take(&mut *self.resources.a11y_changed_elements.borrow_mut())
    }

    pub fn has_a11y_changed(&self) -> bool {
        !self.resources.a11y_changed_elements.borrow().is_empty()
    }

    pub fn register_on_mount(&self, eid: ElementId) {
        self.resources.mount_callbacks.borrow_mut().push(eid);
    }

    pub fn drain_mount_callbacks(&self) -> Vec<ElementId> {
        std::mem::take(&mut *self.resources.mount_callbacks.borrow_mut())
    }

    // ── Cache eviction (queued from callback context, drained on frame start) ──

    pub fn queue_cache_eviction(&self, id: ElementId) {
        self.resources.pending_cache_evictions.borrow_mut().push(id);
    }

    pub fn take_cache_evictions(&self) -> Vec<ElementId> {
        std::mem::take(&mut *self.resources.pending_cache_evictions.borrow_mut())
    }

    pub fn queue_clear_all_caches(&self) {
        self.resources.pending_cache_clear_all.set(true);
    }

    pub fn take_clear_all_caches(&self) -> bool {
        self.resources.pending_cache_clear_all.replace(false)
    }

    // ── Event-handler removal (queued at element teardown, drained per frame) ──

    pub fn queue_handler_removal(&self, id: ElementId) {
        self.resources
            .pending_handler_removals
            .borrow_mut()
            .push(id);
    }

    pub fn take_handler_removals(&self) -> Vec<ElementId> {
        std::mem::take(&mut *self.resources.pending_handler_removals.borrow_mut())
    }

    pub fn has_pending_handler_removals(&self) -> bool {
        !self.resources.pending_handler_removals.borrow().is_empty()
    }

    // ── Exit list (active exit animations) ──

    pub fn clear_exit_list(&self) {
        self.resources.exit_list.borrow_mut().clear();
    }

    pub fn mark_a11y_dirty(&self) {
        self.resources.a11y_dirty.set(true);
    }
    pub fn is_a11y_dirty(&self) -> bool {
        self.resources.a11y_dirty.get()
    }
    pub fn clear_a11y_dirty(&self) {
        self.resources.a11y_dirty.set(false);
    }

    // ── Interaction arbitration (drag elevation / context-menu chain) ──
    pub fn request_drag_z(&self, eid: ElementId, elevate: bool) {
        self.interaction.drag_z_request.set(Some((eid, elevate)));
    }
    pub fn take_drag_z_request(&self) -> Option<(ElementId, bool)> {
        self.interaction.drag_z_request.take()
    }
    pub fn menu_chain_push(&self, eid: ElementId) {
        self.interaction.menu_chain.borrow_mut().push(eid);
    }
    pub fn menu_chain_clear(&self) {
        self.interaction.menu_chain.borrow_mut().clear();
    }
    pub fn menu_chain_snapshot(&self) -> Vec<ElementId> {
        self.interaction.menu_chain.borrow().clone()
    }
    pub fn menu_chain_with<R>(&self, f: impl FnOnce(&mut Vec<ElementId>) -> R) -> R {
        f(&mut self.interaction.menu_chain.borrow_mut())
    }
    pub fn set_hovered_submenu(&self, v: Option<(ElementId, std::time::Instant)>) {
        *self.interaction.hovered_submenu.borrow_mut() = v;
    }
    pub fn hovered_submenu_with<R>(
        &self,
        f: impl FnOnce(&mut Option<(ElementId, std::time::Instant)>) -> R,
    ) -> R {
        f(&mut self.interaction.hovered_submenu.borrow_mut())
    }
    pub fn submenu_open_time(&self) -> u64 {
        self.interaction.submenu_open_time.get()
    }
    pub fn set_submenu_open_time(&self, v: u64) {
        self.interaction.submenu_open_time.set(v);
    }

    pub fn is_exit_pending_active(&self) -> bool {
        !self.resources.exit_list.borrow().is_empty()
    }

    pub fn register_exit(&self, eid: ElementId) {
        self.resources.exit_list.borrow_mut().push(eid);
    }

    pub fn drain_exits(&self) -> Vec<ElementId> {
        std::mem::take(&mut *self.resources.exit_list.borrow_mut())
    }

    // ── Frame pipeline phase ──

    pub fn set_phase(&self, phase: FramePhase) {
        self.resources.current_phase.set(phase);
    }

    pub fn current_phase(&self) -> FramePhase {
        self.resources.current_phase.get()
    }

    pub fn ecs_unregister_element(&self, eid: ElementId) {
        self.resources
            .drag_layout_elements
            .borrow_mut()
            .remove(&eid);
        self.resources.theme_elements.borrow_mut().remove(&eid);
        self.resources
            .pending_scroll_elements
            .borrow_mut()
            .remove(&eid);
        self.resources.scrollable_elements.borrow_mut().remove(&eid);
        self.resources
            .a11y_changed_elements
            .borrow_mut()
            .remove(&eid);
        self.resources
            .mount_callbacks
            .borrow_mut()
            .retain(|&id| id != eid);
    }

    // ── Active-set tracking (per-frame component processing) ──

    pub fn register_active(&self, eid: ElementId, tag: ActiveTag) {
        self.resources
            .active_sets
            .borrow_mut()
            .entry(tag)
            .or_default()
            .insert(eid);
    }

    pub fn unregister_active(&self, eid: ElementId, tag: ActiveTag) {
        if let Some(set) = self.resources.active_sets.borrow_mut().get_mut(&tag) {
            set.remove(&eid);
        }
    }

    pub fn is_active(&self, eid: ElementId, tag: ActiveTag) -> bool {
        self.resources
            .active_sets
            .borrow()
            .get(&tag)
            .is_some_and(|s| s.contains(&eid))
    }

    pub(crate) fn drain_active(&self, tag: ActiveTag) -> std::collections::HashSet<ElementId> {
        self.resources
            .active_sets
            .borrow_mut()
            .remove(&tag)
            .unwrap_or_default()
    }

    /// Remove *all* active-set entries for an element (teardown path).
    pub(crate) fn unregister_all_active(&self, eid: ElementId) {
        for set in self.resources.active_sets.borrow_mut().values_mut() {
            set.remove(&eid);
        }
    }

    // ── Component tables handle ──

    pub fn set_component_tables(
        &self,
        ct: std::rc::Rc<std::cell::RefCell<crate::ecs::tables::ComponentTables>>,
    ) {
        *self.elements.component_tables.borrow_mut() = Some(ct);
    }

    /// Returns the shared ComponentTables handle, lazily creating an empty one
    /// if none has been installed yet (mirrors the old thread_local auto-init
    /// for tests / early-startup paths). Clones the Rc so callers borrow the
    /// tables without holding a RefCell borrow on AppContext.
    pub fn ensure_component_tables(
        &self,
    ) -> std::rc::Rc<std::cell::RefCell<crate::ecs::tables::ComponentTables>> {
        {
            let slot = self.elements.component_tables.borrow();
            if let Some(ct) = slot.as_ref() {
                return ct.clone();
            }
        }
        let empty = std::rc::Rc::new(std::cell::RefCell::new(
            crate::ecs::tables::ComponentTables::new(),
        ));
        *self.elements.component_tables.borrow_mut() = Some(empty.clone());
        empty
    }

    // ── Element domain: bounds ──

    pub fn register_bounds(&self, id: ElementId, bounds: Rect) {
        self.elements.bounds.borrow_mut().insert(id, bounds);
    }

    pub fn update_bounds(&self, id: ElementId, bounds: Rect) {
        self.elements.bounds.borrow_mut().insert(id, bounds);
    }

    pub fn remove_bounds(&self, id: ElementId) {
        self.elements.bounds.borrow_mut().remove(&id);
    }

    pub fn bounds_of(&self, id: ElementId) -> Option<Rect> {
        self.elements.bounds.borrow().get(&id).copied()
    }

    pub fn query_bounds_at(&self, point: Point) -> Vec<(ElementId, Rect)> {
        self.elements
            .bounds
            .borrow()
            .iter()
            .filter(|(_, r)| r.contains(point))
            .map(|(&id, &r)| (id, r))
            .collect()
    }

    pub fn all_bounds(&self) -> Vec<(ElementId, Rect)> {
        self.elements
            .bounds
            .borrow()
            .iter()
            .map(|(&id, &r)| (id, r))
            .collect()
    }

    // ── Element domain: parent map ──

    pub fn register_parent(&self, child: ElementId, parent: ElementId) {
        self.elements.parent_map.borrow_mut().insert(child, parent);
    }

    pub fn unregister_parent(&self, child: ElementId) {
        self.elements.parent_map.borrow_mut().remove(&child);
    }

    pub fn parent_of(&self, id: ElementId) -> Option<ElementId> {
        self.elements.parent_map.borrow().get(&id).copied()
    }

    pub fn is_descendant_of(&self, child: ElementId, ancestor: ElementId) -> bool {
        let mut cur = Some(child);
        while let Some(cid) = cur {
            if cid == ancestor {
                return true;
            }
            cur = self.elements.parent_map.borrow().get(&cid).copied();
        }
        false
    }

    // ── Element domain: el_registry state access ──

    pub fn state_of(&self, id: ElementId) -> StateFlags {
        self.elements
            .el_registry
            .borrow()
            .get(&id)
            .map_or(StateFlags::NONE, |info| info.state.get())
    }

    pub fn add_state(&self, id: ElementId, flag: StateFlags) {
        if let Some(info) = self.elements.el_registry.borrow().get(&id) {
            info.state.set(info.state.get() | flag);
        }
    }

    pub fn remove_state(&self, id: ElementId, flag: StateFlags) {
        if let Some(info) = self.elements.el_registry.borrow().get(&id) {
            let current = info.state.get();
            info.state.set(StateFlags(current.0 & !flag.0));
        }
    }

    pub fn set_state(&self, id: ElementId, flag: StateFlags, on: bool) {
        if let Some(info) = self.elements.el_registry.borrow().get(&id) {
            let mut s = info.state.get();
            s.set(flag, on);
            info.state.set(s);
            self.register_dirty(id, DirtyFlags::REPAINT);
        }
    }

    pub fn has_state(&self, id: ElementId, flag: StateFlags) -> bool {
        self.elements
            .el_registry
            .borrow()
            .get(&id)
            .is_some_and(|info| info.state.get().contains(flag))
    }

    pub fn set_elinfo_visible(&self, id: ElementId, v: bool) {
        if let Some(info) = self.elements.el_registry.borrow_mut().get_mut(&id) {
            info.visible = v;
        }
    }

    pub fn is_elinfo_visible(&self, id: ElementId) -> bool {
        self.elements
            .el_registry
            .borrow()
            .get(&id)
            .is_none_or(|info| info.visible)
    }

    pub fn set_elinfo_z_index(&self, id: ElementId, v: i32) {
        if let Some(info) = self.elements.el_registry.borrow_mut().get_mut(&id) {
            info.z_index = v;
        }
    }

    pub fn bump_subtree_gen_local(&self, id: ElementId) {
        if let Some(info) = self.elements.el_registry.borrow().get(&id) {
            info.subtree_gen.set(info.subtree_gen.get().wrapping_add(1));
        }
    }

    pub fn register_element_info(
        &self,
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
        self.elements.el_registry.borrow_mut().insert(
            id,
            ElInfo {
                dirty,
                state,
                affected_by_child_size,
                size_independent: crate::ecs::components::AxisPair::BOTH_DEP,
                has_solid_background,
                subtree_gen,
                layout_gen,
                surface_gen,
                decor_gen,
                z_index,
                accepts_mouse,
                input_pass_through,
                visible,
                slot_inactive,
                reactive_visible,
                accum_scroll_x: Cell::new(0.0),
                accum_scroll_y: Cell::new(0.0),
                accum_scroll_gen: Cell::new(0),
                tree_order,
                tab_index: Cell::new(None),
            },
        );
    }

    pub fn element_count(&self) -> usize {
        self.elements.el_registry.borrow().len()
    }

    /// Whether an element is registered in this context's element registry.
    /// Used by debug assertions to catch mount-ordering bugs (e.g. the root
    /// being registered into the thread_local fallback instead of the active
    /// AppContext because `set_current_app` ran after `arena.allocate()`).
    pub fn has_element(&self, id: ElementId) -> bool {
        self.elements.el_registry.borrow().contains_key(&id)
    }

    pub fn mark_dirty(&self, id: ElementId, flags: DirtyFlags) {
        if let Some(info) = self.elements.el_registry.borrow().get(&id) {
            info.dirty.set(info.dirty.get() | flags);
        }
    }

    pub fn mark_all_dirty(&self) {
        for info in self.elements.el_registry.borrow().values() {
            info.dirty.set(DirtyFlags::MEASURE);
        }
    }

    pub fn affected_by_child_size(&self, id: ElementId) -> bool {
        self.elements
            .el_registry
            .borrow()
            .get(&id)
            .is_none_or(|info| info.affected_by_child_size)
    }

    pub fn set_affected_by_child_size(&self, id: ElementId, v: bool) {
        if let Some(info) = self.elements.el_registry.borrow_mut().get_mut(&id) {
            info.affected_by_child_size = v;
        }
    }

    pub fn size_independent(&self, id: ElementId) -> crate::ecs::components::AxisPair {
        self.elements
            .el_registry
            .borrow()
            .get(&id)
            .map_or(crate::ecs::components::AxisPair::BOTH_DEP, |info| {
                info.size_independent
            })
    }

    pub fn set_size_independent(&self, id: ElementId, v: crate::ecs::components::AxisPair) {
        if let Some(info) = self.elements.el_registry.borrow_mut().get_mut(&id) {
            info.size_independent = v;
        }
    }

    pub fn has_solid_background(&self, id: ElementId) -> bool {
        self.elements
            .el_registry
            .borrow()
            .get(&id)
            .is_some_and(|info| info.has_solid_background)
    }

    pub fn subtree_gen_of(&self, id: ElementId) -> u64 {
        self.elements
            .el_registry
            .borrow()
            .get(&id)
            .map_or(0, |info| info.subtree_gen.get())
    }

    pub fn layout_gen_of(&self, id: ElementId) -> u64 {
        self.elements
            .el_registry
            .borrow()
            .get(&id)
            .map_or(0, |info| info.layout_gen.get())
    }

    pub fn bump_layout_gen_local(&self, id: ElementId) {
        if let Some(info) = self.elements.el_registry.borrow().get(&id) {
            info.layout_gen.set(info.layout_gen.get().wrapping_add(1));
        }
    }

    pub fn bump_surface_gen_remote(&self, id: ElementId) {
        if let Some(info) = self.elements.el_registry.borrow().get(&id) {
            if let Some(ref sg) = info.surface_gen {
                sg.set(sg.get().wrapping_add(1));
            }
        }
    }

    pub fn bump_decor_gen_remote(&self, id: ElementId) {
        if let Some(info) = self.elements.el_registry.borrow().get(&id) {
            if let Some(ref dg) = info.decor_gen {
                dg.set(dg.get().wrapping_add(1));
            }
        }
    }

    pub fn clear_dirty_in_set(&self, ids: &[ElementId], level: DirtyFlags) {
        let mask = DirtyFlags(!level.0 & 0b111);
        let map = self.elements.el_registry.borrow();
        for eid in ids {
            if let Some(info) = map.get(eid) {
                let current = info.dirty.get();
                info.dirty.set(DirtyFlags(current.0 & mask.0));
            }
        }
    }

    /// Remove element from registry; returns (tab_index, tree_order) if it existed.
    pub fn unregister_element_info(&self, id: ElementId) -> Option<(Option<usize>, u64)> {
        self.elements
            .el_registry
            .borrow_mut()
            .remove(&id)
            .map(|info| (info.tab_index.get(), info.tree_order))
    }

    // ── Focus domain ──

    pub fn invalidate_focus_order(&self) {
        self.focus.focus_order_valid.set(false);
    }

    pub fn register_focusable(&self, eid: ElementId, tab_index: Option<usize>, tree_order: u64) {
        self.focus.focusable_set.borrow_mut().insert((
            std::cmp::Reverse(tab_index),
            tree_order,
            eid,
        ));
        self.focus.focus_order_valid.set(false);
    }

    pub fn unregister_focusable(&self, eid: ElementId, tab_index: Option<usize>, tree_order: u64) {
        self.focus.focusable_set.borrow_mut().remove(&(
            std::cmp::Reverse(tab_index),
            tree_order,
            eid,
        ));
        self.focus.focus_order_valid.set(false);
    }

    pub fn focus_order_valid(&self) -> bool {
        self.focus.focus_order_valid.get()
    }

    pub fn set_focus_order_valid(&self, v: bool) {
        self.focus.focus_order_valid.set(v);
    }

    pub fn focusable_ids(&self) -> Vec<ElementId> {
        self.focus
            .focusable_set
            .borrow()
            .iter()
            .map(|&(_, _, id)| id)
            .collect()
    }

    pub fn visit_focusable(&self, mut f: impl FnMut(ElementId, u64)) {
        for &(_, to, eid) in self.focus.focusable_set.borrow().iter() {
            f(eid, to);
        }
    }

    pub fn remove_focusable_entry(
        &self,
        tab_index: Option<usize>,
        tree_order: u64,
        eid: ElementId,
    ) {
        self.focus.focusable_set.borrow_mut().remove(&(
            std::cmp::Reverse(tab_index),
            tree_order,
            eid,
        ));
    }

    // ── Focus-order cached Vec ──

    pub fn focus_order_cached(&self) -> Vec<ElementId> {
        self.focus.focus_order.borrow().clone()
    }

    pub fn set_focus_order(&self, v: Vec<ElementId>) {
        *self.focus.focus_order.borrow_mut() = v;
    }

    pub fn clear_focus_order(&self) {
        self.focus.focus_order.borrow_mut().clear();
    }

    // ── Spatial domain ──

    pub(crate) fn spatial_point_to_layout(
        &self,
        arena: &crate::core::element::ElementArena,
        eid: ElementId,
        point: Point,
    ) -> Point {
        let (sx, sy) = self.spatial_accumulated_scroll_cached(arena, eid);
        let (own_x, own_y) = arena
            .comp_scroll(eid)
            .map(|sc| {
                let o = sc.scroll_offset.get();
                (o.x, o.y)
            })
            .unwrap_or((0.0, 0.0));
        Point::new(point.x + sx - own_x, point.y + sy - own_y)
    }

    fn spatial_point_to_scroll_visual(
        &self,
        arena: &crate::core::element::ElementArena,
        scroll_eid: ElementId,
        point: Point,
    ) -> Point {
        let (sx, sy) = self.spatial_accumulated_scroll_cached(arena, scroll_eid);
        Point::new(point.x + sx, point.y + sy)
    }

    fn spatial_compute_accum_scroll(
        &self,
        arena: &crate::core::element::ElementArena,
        eid: ElementId,
    ) -> (f32, f32) {
        let mut sx = 0.0;
        let mut sy = 0.0;
        let mut cur = eid;
        loop {
            if let Some(sc) = arena.comp_scroll(cur) {
                let o = sc.scroll_offset.get();
                sx += o.x;
                sy += o.y;
            }
            match self.elements.parent_map.borrow().get(&cur).copied() {
                Some(pid) => cur = pid,
                None => break,
            }
        }
        (sx, sy)
    }

    pub(crate) fn spatial_is_visible_chain_fast(&self, mut id: ElementId) -> bool {
        loop {
            let guard = self.elements.el_registry.borrow();
            let info = guard.get(&id);
            let parent = self.elements.parent_map.borrow().get(&id).copied();
            let v = info.is_some_and(|i| {
                i.visible
                    && !i.slot_inactive.as_ref().is_some_and(|c| c.get())
                    && i.reactive_visible.as_ref().is_none_or(|c| c.get())
            });
            drop(guard);
            if !v {
                return false;
            }
            match parent {
                Some(pid) => {
                    id = pid;
                }
                None => {
                    return true;
                }
            }
        }
    }

    fn spatial_is_within_scroll_clip(
        &self,
        arena: &crate::core::element::ElementArena,
        eid: ElementId,
        point: Point,
    ) -> bool {
        let mut cur = eid;
        loop {
            match self.elements.parent_map.borrow().get(&cur).copied() {
                Some(pid) => {
                    // Only elements that actually clip (overflow Scroll/Clip)
                    // bound their descendants. A dormant ScrollComponent from
                    // a preallocate mask (e.g. Table body) must NOT clip —
                    // virtualized rows are absolutely positioned beyond the
                    // collapsed body and would be wrongly rejected.
                    let is_clipping = arena.comp_scroll(pid).is_some()
                        && arena
                            .component_tables
                            .borrow()
                            .layout
                            .get(&pid)
                            .is_some_and(|l| {
                                matches!(
                                    l.overflow,
                                    crate::core::config::Overflow::Scroll
                                        | crate::core::config::Overflow::Clip
                                )
                            });
                    if is_clipping {
                        let adj = self.spatial_point_to_layout(arena, pid, point);
                        if let Some(b) = self.elements.bounds.borrow().get(&pid).copied() {
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

    pub(crate) fn spatial_accumulated_scroll_cached(
        &self,
        arena: &crate::core::element::ElementArena,
        eid: ElementId,
    ) -> (f32, f32) {
        let gen = self.spatial.scroll_tree_gen.get();

        // Fast path: cache hit
        if let Some(val) = self
            .elements
            .el_registry
            .borrow()
            .get(&eid)
            .and_then(|info| {
                if info.accum_scroll_gen.get() == gen {
                    Some((info.accum_scroll_x.get(), info.accum_scroll_y.get()))
                } else {
                    None
                }
            })
        {
            return val;
        }

        // Cache miss: walk ancestors
        let (sx, sy) = self.spatial_compute_accum_scroll(arena, eid);

        // Store back
        if let Some(info) = self.elements.el_registry.borrow().get(&eid) {
            info.accum_scroll_x.set(sx);
            info.accum_scroll_y.set(sy);
            info.accum_scroll_gen.set(gen);
        }

        (sx, sy)
    }

    /// Whether the element's visual rect (layout bounds shifted by the
    /// accumulated ancestor scroll) lies fully outside the window viewport
    /// or fully outside any clipping scroll ancestor's visual rect.
    ///
    /// Used as the 4th visibility gate for animations (audit 2026-07-18
    /// animation pass): an animation scrolled out of view must not keep
    /// producing dirty/frames. Conservative on missing data: unknown
    /// bounds → treated as on-screen (never freeze something visible).
    /// Component reads go through the thread-local tables (`with_ct`) so
    /// arena-less contexts (frame_tick closures) can use this too.
    ///
    /// Wake-up is inherent: scrolling always produces frames, so a tick
    /// re-evaluates this on every scroll frame and revives on entry.
    pub(crate) fn spatial_is_offscreen(
        &self,
        eid: ElementId,
        viewport_w: f32,
        viewport_h: f32,
    ) -> bool {
        use crate::core::element::with_ct;

        let Some(b) = self.elements.bounds.borrow().get(&eid).copied() else {
            return false;
        };
        if b.width <= 0.0 || b.height <= 0.0 {
            return false; // not laid out yet — assume visible
        }
        let scroll_of = |id: ElementId| -> Option<crate::style::Vec2> {
            with_ct(|ct| ct.scroll.get(&id).map(|sc| sc.scroll_offset.get()))
        };
        let accum_scroll = |id: ElementId| -> (f32, f32) {
            // Same walk as spatial_compute_accum_scroll, via with_ct.
            let mut sx = 0.0;
            let mut sy = 0.0;
            let mut cur = id;
            loop {
                if let Some(o) = scroll_of(cur) {
                    sx += o.x;
                    sy += o.y;
                }
                match self.elements.parent_map.borrow().get(&cur).copied() {
                    Some(pid) => cur = pid,
                    None => break,
                }
            }
            (sx, sy)
        };

        let (sx, sy) = accum_scroll(eid);
        // Visual rect: scroll moves content up/left by the offset.
        let vx = b.x - sx;
        let vy = b.y - sy;

        // 1. Window viewport intersection.
        if viewport_w > 0.0
            && viewport_h > 0.0
            && (vx + b.width <= 0.0 || vy + b.height <= 0.0 || vx >= viewport_w || vy >= viewport_h)
        {
            return true;
        }

        // 2. Clipping scroll ancestors: compare visual rects. An ancestor's
        // own rect is NOT shifted by its own scroll offset (only content is),
        // so subtract its accumulated-scroll-above = accum(pid) - own_offset.
        let mut cur = eid;
        loop {
            match self.elements.parent_map.borrow().get(&cur).copied() {
                Some(pid) => {
                    let own = scroll_of(pid);
                    let is_clipping = own.is_some()
                        && with_ct(|ct| {
                            ct.layout.get(&pid).is_some_and(|l| {
                                matches!(
                                    l.overflow,
                                    crate::core::config::Overflow::Scroll
                                        | crate::core::config::Overflow::Clip
                                )
                            })
                        });
                    if is_clipping {
                        if let Some(pb) = self.elements.bounds.borrow().get(&pid).copied() {
                            let (pax, pay) = accum_scroll(pid);
                            let own = own.unwrap_or(crate::style::Vec2::ZERO);
                            let px = pb.x - (pax - own.x);
                            let py = pb.y - (pay - own.y);
                            if vx + b.width <= px
                                || vy + b.height <= py
                                || vx >= px + pb.width
                                || vy >= py + pb.height
                            {
                                return true;
                            }
                        }
                    }
                    cur = pid;
                }
                None => return false,
            }
        }
    }

    // ── Public spatial methods ──

    pub fn spatial_register(&self, id: ElementId, new_bounds: Rect, tree_order: u64) {
        // 1. Remove old cell entries
        if let Some(cells) = self.spatial.element_cells.borrow_mut().get_mut(&id) {
            for &cell in cells.iter() {
                if let Some(gc) = self.spatial.grid.borrow_mut().get_mut(&cell) {
                    gc.retain(|e| e.eid != id);
                }
            }
            cells.clear();
        }

        // 2. Fetch element z_index from el_registry
        let z = self
            .elements
            .el_registry
            .borrow()
            .get(&id)
            .map_or(0, |info| info.z_index);

        let cells = crate::core::dirty_registry::cells_covered_by(new_bounds);

        // 3. Insert into new cells (binary-search insert, descending z_index + tree_order)
        for &cell in &cells {
            let mut map = self.spatial.grid.borrow_mut();
            let gc = map.entry(cell).or_default();
            let pos = gc.partition_point(|e| {
                e.z_index > z || (e.z_index == z && e.tree_order > tree_order)
            });
            gc.insert(
                pos,
                SpatialEntry {
                    eid: id,
                    z_index: z,
                    tree_order,
                },
            );
        }

        // 4. Cache cell list for this element
        self.spatial.element_cells.borrow_mut().insert(id, cells);
    }

    pub fn spatial_unregister(&self, id: ElementId) {
        self.spatial.scroll_offsets.borrow_mut().remove(&id);
        self.spatial.position_offsets.borrow_mut().remove(&id);
        if let Some(cells) = self.spatial.element_cells.borrow_mut().remove(&id) {
            for cell in cells {
                if let Some(gc) = self.spatial.grid.borrow_mut().get_mut(&cell) {
                    gc.retain(|e| e.eid != id);
                }
            }
        }
    }

    pub fn spatial_update_scroll(&self, eid: ElementId, ox: f32, oy: f32) {
        self.spatial
            .scroll_offsets
            .borrow_mut()
            .insert(eid, (ox, oy));
        self.spatial
            .scroll_tree_gen
            .set(self.spatial.scroll_tree_gen.get() + 1);
    }

    pub fn spatial_register_position_offset(&self, eid: ElementId) {
        self.spatial.position_offsets.borrow_mut().insert(eid);
    }

    pub fn spatial_clear_scroll_tree_gen(&self) {
        self.spatial.scroll_tree_gen.set(0);
    }

    pub fn spatial_reset(&self) {
        self.spatial.grid.borrow_mut().clear();
        self.spatial.element_cells.borrow_mut().clear();
        self.spatial.scroll_offsets.borrow_mut().clear();
        self.spatial.position_offsets.borrow_mut().clear();
        self.spatial.scroll_tree_gen.set(0);
    }

    pub fn spatial_hit_test(
        &self,
        arena: &crate::core::element::ElementArena,
        point: Point,
    ) -> Option<ElementId> {
        let cell_key = (
            (point.x / crate::core::dirty_registry::SPATIAL_CELL_SIZE).floor() as i32,
            (point.y / crate::core::dirty_registry::SPATIAL_CELL_SIZE).floor() as i32,
        );
        let mut all_candidates: Vec<SpatialEntry> = self
            .spatial
            .grid
            .borrow()
            .get(&cell_key)
            .cloned()
            .unwrap_or_default();

        // Adjacent-cell lookup for scroll containers
        let scroll_eids: Vec<ElementId> = self
            .spatial
            .scroll_offsets
            .borrow()
            .iter()
            .filter(|(_, &(x, y))| x != 0.0 || y != 0.0)
            .map(|(&eid, _)| eid)
            .collect();
        for &eid in &scroll_eids {
            let adj = self.spatial_point_to_scroll_visual(arena, eid, point);
            let adj_cell = (
                (adj.x / crate::core::dirty_registry::SPATIAL_CELL_SIZE).floor() as i32,
                (adj.y / crate::core::dirty_registry::SPATIAL_CELL_SIZE).floor() as i32,
            );
            if adj_cell != cell_key {
                if let Some(extra) = self.spatial.grid.borrow().get(&adj_cell) {
                    all_candidates.extend_from_slice(extra);
                }
            }
        }

        // Adjacent-cell lookup for position-offset elements
        let offset_eids: Vec<ElementId> = self
            .spatial
            .position_offsets
            .borrow()
            .iter()
            .copied()
            .collect();
        for &eid in &offset_eids {
            let (ox, oy) = crate::core::dirty_registry::read_pos_offset(arena, eid);
            if ox == 0.0 && oy == 0.0 {
                continue;
            }
            let adj = self.spatial_point_to_layout(arena, eid, point);
            let adj_cell = (
                ((adj.x - ox) / crate::core::dirty_registry::SPATIAL_CELL_SIZE).floor() as i32,
                ((adj.y - oy) / crate::core::dirty_registry::SPATIAL_CELL_SIZE).floor() as i32,
            );
            if adj_cell != cell_key {
                if let Some(extra) = self.spatial.grid.borrow().get(&adj_cell) {
                    all_candidates.extend_from_slice(extra);
                }
            }
        }

        // Dedup + sort by hit precedence
        {
            let mut seen: std::collections::HashSet<ElementId> = std::collections::HashSet::new();
            all_candidates.retain(|e| seen.insert(e.eid));
        }
        all_candidates.sort_by(|a, b| {
            b.z_index
                .cmp(&a.z_index)
                .then_with(|| b.tree_order.cmp(&a.tree_order))
        });

        for (i, entry) in all_candidates.iter().enumerate() {
            let skip = self
                .elements
                .el_registry
                .borrow()
                .get(&entry.eid)
                .is_none_or(|info| {
                    !info.visible
                        || !info.accepts_mouse
                        || info.input_pass_through
                        || info.reactive_visible.as_ref().is_some_and(|c| !c.get())
                });
            if skip {
                continue;
            }

            let mut local = self.spatial_point_to_layout(arena, entry.eid, point);
            if offset_eids.contains(&entry.eid) {
                let (ox, oy) = crate::core::dirty_registry::read_pos_offset(arena, entry.eid);
                local.x -= ox;
                local.y -= oy;
            }

            if let Some(bounds) = self.elements.bounds.borrow().get(&entry.eid).copied() {
                if bounds.contains(local)
                    && self.spatial_is_visible_chain_fast(entry.eid)
                    && self.spatial_is_within_scroll_clip(arena, entry.eid, point)
                {
                    let mut best = entry.eid;
                    for other in &all_candidates[i + 1..] {
                        if !self.is_descendant_of(other.eid, best) {
                            continue;
                        }
                        let other_skip = self
                            .elements
                            .el_registry
                            .borrow()
                            .get(&other.eid)
                            .is_none_or(|info| {
                                !info.visible || !info.accepts_mouse || info.input_pass_through
                            });
                        if other_skip {
                            continue;
                        }

                        let mut olocal = self.spatial_point_to_layout(arena, other.eid, point);
                        if offset_eids.contains(&other.eid) {
                            let (ox, oy) =
                                crate::core::dirty_registry::read_pos_offset(arena, other.eid);
                            olocal.x -= ox;
                            olocal.y -= oy;
                        }
                        if let Some(ob) = self.elements.bounds.borrow().get(&other.eid).copied() {
                            if ob.contains(olocal)
                                && self.spatial_is_visible_chain_fast(other.eid)
                                && self.spatial_is_within_scroll_clip(arena, other.eid, point)
                            {
                                best = other.eid;
                            }
                        }
                    }
                    return Some(best);
                }
            }
        }
        None
    }

    pub fn spatial_hit_scrollable(
        &self,
        arena: &crate::core::element::ElementArena,
        point: Point,
    ) -> Option<ElementId> {
        let cell_key = (
            (point.x / crate::core::dirty_registry::SPATIAL_CELL_SIZE).floor() as i32,
            (point.y / crate::core::dirty_registry::SPATIAL_CELL_SIZE).floor() as i32,
        );
        let mut candidates: Vec<SpatialEntry> = self
            .spatial
            .grid
            .borrow()
            .get(&cell_key)
            .cloned()
            .unwrap_or_default();

        // Adjacent-cell lookup for scroll containers
        let scroll_eids: Vec<ElementId> = self
            .spatial
            .scroll_offsets
            .borrow()
            .iter()
            .filter(|(_, &(x, y))| x != 0.0 || y != 0.0)
            .map(|(&eid, _)| eid)
            .collect();
        for &eid in &scroll_eids {
            let adj = self.spatial_point_to_scroll_visual(arena, eid, point);
            let adj_cell = (
                (adj.x / crate::core::dirty_registry::SPATIAL_CELL_SIZE).floor() as i32,
                (adj.y / crate::core::dirty_registry::SPATIAL_CELL_SIZE).floor() as i32,
            );
            if adj_cell != cell_key {
                if let Some(extra) = self.spatial.grid.borrow().get(&adj_cell) {
                    candidates.extend_from_slice(extra);
                }
            }
        }

        // Dedup
        {
            let mut seen: std::collections::HashSet<ElementId> = std::collections::HashSet::new();
            candidates.retain(|e| seen.insert(e.eid));
        }

        // Find the deepest scrollable element at the point
        let mut matches: Vec<ElementId> = Vec::new();
        for entry in &candidates {
            let el = match arena.get(entry.eid) {
                Some(e) => e,
                None => continue,
            };
            if el.scroll_offset().is_none() {
                continue;
            }
            if !el.is_visible() {
                continue;
            }
            let adj = self.spatial_point_to_layout(arena, entry.eid, point);
            if let Some(bounds) = self.elements.bounds.borrow().get(&entry.eid).copied() {
                if bounds.contains(adj) {
                    matches.push(entry.eid);
                }
            }
        }

        if matches.is_empty() {
            return None;
        }
        matches.sort_by_key(|&eid| {
            -(self
                .elements
                .el_registry
                .borrow()
                .get(&eid)
                .map_or(0i64, |info| info.tree_order as i64))
        });
        Some(matches[0])
    }

    /// Return all scrollable elements at the given point, sorted from
    /// innermost (deepest) to outermost. Used by the nested-scroll
    /// dispatch to try inner scrollables first, then pass unconsumed
    /// delta to outer ones.
    pub fn spatial_scroll_chain(
        &self,
        arena: &crate::core::element::ElementArena,
        point: Point,
    ) -> Vec<ElementId> {
        let cell_key = (
            (point.x / crate::core::dirty_registry::SPATIAL_CELL_SIZE).floor() as i32,
            (point.y / crate::core::dirty_registry::SPATIAL_CELL_SIZE).floor() as i32,
        );
        let mut candidates: Vec<SpatialEntry> = self
            .spatial
            .grid
            .borrow()
            .get(&cell_key)
            .cloned()
            .unwrap_or_default();

        let scroll_eids: Vec<ElementId> = self
            .spatial
            .scroll_offsets
            .borrow()
            .iter()
            .filter(|(_, &(x, y))| x != 0.0 || y != 0.0)
            .map(|(&eid, _)| eid)
            .collect();
        for &eid in &scroll_eids {
            let adj = self.spatial_point_to_scroll_visual(arena, eid, point);
            let adj_cell = (
                (adj.x / crate::core::dirty_registry::SPATIAL_CELL_SIZE).floor() as i32,
                (adj.y / crate::core::dirty_registry::SPATIAL_CELL_SIZE).floor() as i32,
            );
            if adj_cell != cell_key {
                if let Some(extra) = self.spatial.grid.borrow().get(&adj_cell) {
                    candidates.extend_from_slice(extra);
                }
            }
        }

        {
            let mut seen: std::collections::HashSet<ElementId> = std::collections::HashSet::new();
            candidates.retain(|e| seen.insert(e.eid));
        }

        let mut matches: Vec<ElementId> = Vec::new();
        for entry in &candidates {
            let el = match arena.get(entry.eid) {
                Some(e) => e,
                None => continue,
            };
            if el.scroll_offset().is_none() {
                continue;
            }
            if !el.is_visible() {
                continue;
            }
            let adj = self.spatial_point_to_layout(arena, entry.eid, point);
            if let Some(bounds) = self.elements.bounds.borrow().get(&entry.eid).copied() {
                if bounds.contains(adj) {
                    matches.push(entry.eid);
                }
            }
        }

        // Deepest first (highest tree_order = most recently created)
        matches.sort_by_key(|&eid| {
            -(self
                .elements
                .el_registry
                .borrow()
                .get(&eid)
                .map_or(0i64, |info| info.tree_order as i64))
        });
        matches
    }
}

impl Default for AppContext {
    fn default() -> Self {
        Self::new()
    }
}
