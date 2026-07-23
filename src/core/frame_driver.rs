//! Unified per-frame orchestration. Drives the shared pipeline (dirty →
//! layout → animation → paint → exits → a11y) as a pure function of
//! explicit state. Both `Window::on_frame` and `TestHarness::run_frame`
//! call the same `drive_frame`; platform I/O is injected via the `FrameHook`
//! trait. See `docs/superpowers/specs/2026-07-09-testkit-design.md`.

use crate::animation::AnimationDriver;
use crate::core::element::{DirtyFlags, ElementArena};
use crate::core::perf::{self, PerfPhase};
use crate::core::ElementId;
use crate::ecs::tables::ComponentTables;
use crate::event::focus::FocusHighlightMode;
use crate::event::{EventRegistry, FocusManager};
use crate::layout::taffy_bridge::TaffyBridge;
use crate::render::wgpu::glyphon_bridge::TextAreaDesc;
use crate::render::DrawCommand;
use crate::style::{Color, Size};
use std::cell::Cell;
use std::collections::HashSet;
use web_time::Instant;

// Thread-local viewport size, updated every frame before prepass.
// Used by viewport-relative portal elements (Toast) that run in frame_tick.
thread_local! {
    pub(crate) static CURRENT_VIEWPORT: Cell<(f32, f32)> = const { Cell::new((0.0, 0.0)) };
}

/// Per-frame mutable state bundle — borrowed disjointly from WindowState / TestHarness.
pub struct FrameState<'a> {
    pub arena: &'a mut ElementArena,
    pub taffy: &'a mut TaffyBridge,
    pub events: &'a mut EventRegistry,
    pub animations: &'a mut AnimationDriver,
    pub focus: &'a mut FocusManager,
    pub(crate) scroll_kinetic: &'a mut Option<ScrollKinetic>,
    pub scroll_kinetic_target: &'a mut Option<ElementId>,
}

/// Per-frame read-only input (all diverging points made explicit).
pub struct FrameInput {
    pub size: Size,
    pub frame_id: u64,
    pub is_first_frame: bool,
    pub force_layout: bool,
    pub scale_factor: f32,
    pub bg: Color,
    pub fg: Color,
    pub highlight_mode: FocusHighlightMode,
    pub now: Instant,
    pub scroll_friction: f32,
    pub scroll_stop_speed: f32,
    /// Caller-side coalescing: skip producing paint output this frame
    /// (window: `coalesce_skip_paint`; harness: false).
    pub skip_paint: bool,
}

/// Platform-injected seam hook. Runs at SEAM 1 (after dirty, before layout)
/// and inside SEAM 2 (`drive_frame_platform`) for platform follow-ups.
pub trait FrameHook {
    /// After dirty processing (before layout). Returns whether to force a
    /// full taffy relayout (OR'd with `FrameInput.force_layout`).
    fn on_after_dirty(&mut self, _arena: &mut ElementArena, _events: &mut EventRegistry) -> bool {
        false
    }

    /// After a programmatic focus transfer inside SEAM 2 (autofocus, a11y).
    /// The window uses this to enable IME for text inputs; the harness
    /// leaves it as a no-op.
    fn on_focus_transferred(&mut self, _events: &EventRegistry, _new_id: crate::core::ElementId) {}
}

/// Harness default (no-op hook).
pub struct NoHook;
impl FrameHook for NoHook {}

/// Intermediate result of `drive_frame_layout`, carried across SEAM 2 to
/// `drive_frame_paint`. Owned data only (no borrows) so the caller can use
/// `&mut self` freely for platform work between the two phases.
pub struct LayoutStage {
    pub root_id: ElementId,
    pub processed_all: Vec<ElementId>,
    pub paint_roots: Vec<ElementId>,
    pub root_flags: DirtyFlags,
    pub did_layout: bool,
}

/// Per-frame output — caller decides: window submits to renderer, harness stores in last_scene.
pub struct FrameOutcome {
    pub painted: bool,
    pub commands: Vec<DrawCommand>,
    pub text_areas: Vec<TextAreaDesc>,
    pub backdrop_regions: Vec<crate::render::BackdropRegion>,
    pub processed_all: Vec<ElementId>,
    pub paint_roots: Vec<ElementId>,
    pub repaint_ids: Vec<ElementId>,
    pub root_flags: DirtyFlags,
    pub did_layout: bool,
}

/// Extra state for SEAM 2 that lives in the caller (window/harness).
pub struct PlatformArgs<'a> {
    /// Active drag ghost element + current cursor position, if a
    /// drag-and-drop operation is in flight.
    pub drag_ghost: Option<(ElementId, crate::style::Point)>,
    /// The currently z-elevated dragged row (reorder) and its saved
    /// `z_index_floor`, restored on drop.
    pub drag_elevated: &'a mut Option<(ElementId, Option<i32>)>,
}

/// SEAM 2 — platform-frame work between layout and paint, SHARED by
/// `Window::on_frame` and `TestHarness::run_frame` (audit 2026-07-16
/// round 4). Previously the window ran drag-ghost restore, drag-z
/// elevation, long-press wins and the full a11y dispatch here while the
/// harness only drained autofocus (with weaker semantics) — tests could
/// pass while production diverged. Platform-only follow-ups (IME) are
/// injected via [`FrameHook::on_focus_transferred`].
pub fn drive_frame_platform(
    st: FrameState<'_>,
    stage: &LayoutStage,
    input: &FrameInput,
    args: PlatformArgs<'_>,
    hook: &mut dyn FrameHook,
) {
    use crate::core::dirty_registry;

    // ── Long-press wins from the gesture arena (process_timeouts) ──
    for eid in crate::event::recognizer::drain_long_press_wins() {
        st.events.fire_long_press(eid);
    }

    // ── IME inset change → keep the focused element visible ──
    // (keyboard avoidance, mobile-groundwork W1: the SafeArea re-pad
    // shrinks the viewport; re-run scroll-into-view on the focused
    // element so the keyboard does not cover it.)
    if crate::platform::insets::take_ime_refocus() {
        if let Some(fid) = st.focus.focused() {
            crate::event::focus_manager::scroll_focused_into_view(st.arena, fid);
        }
    }

    // ── Drag ghost restore / apply_drag_layouts ──
    if stage.did_layout {
        if let Some((ghost, cursor)) = args.drag_ghost {
            let gx = cursor.x + 12.0;
            let gy = cursor.y + 12.0;
            if let Some(el) = st.arena.get_mut(ghost) {
                el.screen_bounds = crate::style::Rect::new(gx, gy, 140.0, 28.0);
                el.set_bounds(el.screen_bounds);
                dirty_registry::update_bounds(ghost, el.screen_bounds);
                dirty_registry::register_dirty(ghost, DirtyFlags::REPAINT);
            }
        }
    } else if stage.root_flags.has_repaint() && !input.is_first_frame {
        crate::core::element::apply_drag_layouts(st.arena, stage.root_id);
    }

    // ── Drag-z elevation (raise dragged row above its siblings so it
    //   fully occludes them — same-z rows can't occlude each other's
    //   text). Only z_index_floor is touched, not z_index, so the row
    //   stays in flow (z_index > 0 would force absolute layout). ──
    if let Some((eid, elevate)) = crate::widgets::shared::reorder::take_drag_z_request() {
        if elevate {
            if let Some(el) = st.arena.get_mut(eid) {
                if args.drag_elevated.is_none() {
                    *args.drag_elevated = Some((eid, el.z_index_floor));
                    el.z_index_floor = Some(1);
                    dirty_registry::mark_widget_repaint(eid);
                }
            }
        } else if let Some((deid, prev_floor)) = args.drag_elevated.take() {
            if let Some(el) = st.arena.get_mut(deid) {
                el.z_index_floor = prev_floor;
                dirty_registry::mark_widget_repaint(deid);
            }
        }
    }

    // ── Autofocus (full production semantics: focus_out on the old
    //   element, overlay capture, scroll-into-view, IME via hook) ──
    for focus_id in st.events.drain_autofocus() {
        crate::event::focus_manager::transfer_focus(
            st.arena,
            st.events,
            st.focus,
            focus_id,
            crate::event::FocusReason::Programmatic,
        );
        hook.on_focus_transferred(st.events, focus_id);
    }

    // ── Accessibility actions (screen-reader requests) ──
    crate::platform::a11y_bridge::dispatch_a11y_actions(st.arena, st.events, st.focus, hook);
}

/// Phase 1: kinetic → prepass → dirty → deferred → SEAM 1 hook → layout.
/// Takes `FrameState` **by value** so the borrow of the caller's fields is
/// released when this returns — letting the caller use `&mut self` freely for
/// SEAM 2 platform work before calling `drive_frame_paint`.
pub fn drive_frame_layout(
    st: FrameState<'_>,
    input: &FrameInput,
    hook: &mut dyn FrameHook,
) -> LayoutStage {
    use crate::core::dirty_registry;
    use crate::core::frame_pipeline::{self, FramePhase};

    // Reset per-frame paint-cache counters (O(k) assertion support).
    frame_pipeline::reset_paint_cache_stats();

    let root_id = match st.arena.root_id {
        Some(rid) => rid,
        None => {
            return LayoutStage {
                root_id: ElementId::SENTINEL,
                processed_all: Vec::new(),
                paint_roots: Vec::new(),
                root_flags: DirtyFlags::CLEAN,
                did_layout: false,
            };
        }
    };

    // ── Step 0: kinetic scroll (before dirty drain so marks flow this frame) ──
    perf::perf_begin(PerfPhase::KineticScroll);
    {
        if let Some(ref target_eid) = *st.scroll_kinetic_target {
            let target = *target_eid;
            match st.scroll_kinetic.as_mut() {
                None => {}
                Some(ScrollKinetic::AnimatedTo {
                    target: to,
                    start,
                    anchor,
                    duration_secs,
                }) => {
                    // Pure function of the anchor instant (Phase 2): a
                    // dropped frame lands exactly on the analytic value.
                    let a = *anchor.get_or_insert(input.now);
                    let elapsed = input.now.saturating_duration_since(a).as_secs_f32();
                    let t = (elapsed / *duration_secs).clamp(0.0, 1.0);
                    let eased =
                        crate::animation::apply_easing(t, crate::animation::EasingCurve::EaseInOut);
                    let current = crate::style::Vec2::new(
                        start.x + (to.x - start.x) * eased,
                        start.y + (to.y - start.y) * eased,
                    );
                    if let Some(sc) = st.arena.comp_scroll(target) {
                        sc.scroll_offset.set(current);
                    }
                    crate::core::dirty_registry::spatial_update_scroll(
                        target, current.x, current.y,
                    );
                    crate::widgets::bundle::scroll::bump_scroll_generation(st.arena, target);
                    if let Some(el) = st.arena.get(target) {
                        el.mark_repaint();
                    }
                    if t >= 1.0 {
                        *st.scroll_kinetic = None;
                    }
                }
            }
        }
    }
    perf::perf_end();

    // ── Prepass: O(k) pure arena pre-passes ──
    perf::perf_begin(PerfPhase::Prepass);
    frame_pipeline::set_phase(FramePhase::Prepass);
    CURRENT_VIEWPORT.with(|c| c.set((input.size.width, input.size.height)));
    frame_pipeline::run_pre_passes(st.arena);
    perf::perf_end();

    // ── Pre-drain portal position sync ──
    // MUST run BEFORE process_dirty: when an anchor moved (e.g. its container
    // scrolled) while the portal itself is clean, this registers MEASURE on
    // the portal. Registered any later, that dirty is only seen by
    // recheck_dirty which feeds paint alone — the reposition never reached
    // taffy and the portal stayed at its old screen position (audit
    // 2026-07-17). Newly-mounted portals are attached in the deferred phase
    // below and are covered by the post-layout call instead.
    perf::perf_begin(PerfPhase::PortalPositions);
    crate::platform::portal::update_portal_positions(st.arena, root_id);
    perf::perf_end();

    // ── Dirty processing (O(k)) ──
    perf::perf_begin(PerfPhase::ProcessDirty);
    let mut dirty = frame_pipeline::process_dirty_phase(st.arena, root_id);
    perf::perf_end();

    // ── Deferred tree mutations + portals ──
    perf::perf_begin(PerfPhase::DeferredActions);
    let _prev_trig = dirty_registry::current_trigger();
    dirty_registry::set_current_trigger(dirty_registry::DirtyTriggerTag::DeferredAction);
    let actions = dirty_registry::take_actions();
    if !actions.is_empty() {
        for action in actions {
            // Panic isolation (audit 2026-07-17 round 5, C1): a panicking
            // deferred action must not unwind through the frame driver —
            // that would leave FramePhase dangling and poison any RefCell
            // borrows held by later actions. Same contract as fire_*.
            let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                action(st.arena, root_id, st.events);
            }));
            if let Err(panic) = result {
                crate::core::error::push_error(crate::core::error::UiError::CallbackPanic {
                    context: "defer_action".into(),
                    window_id: None,
                    element_id: None,
                    message: crate::core::error::panic_to_string(&panic),
                });
            }
        }
        // NOTE: tree_order is allocation order and is NEVER renumbered after
        // structural mutations (audit 2026-07-17): children are only ever
        // appended (add_child), so allocation order == DFS preorder for
        // every mount path, and the focusable BTreeSet keys registered
        // tree_order values — renumbering would desync them.
    }
    for portal in crate::platform::portal::drain_portals() {
        st.arena.add_child(root_id, portal);
    }
    // Loop until quiescent: tearing down a portal can queue removals for
    // portals it owns transitively (e.g. a Select inside a Popover), and
    // owner-linked teardown (audit round 3, ①) queues from inside
    // arena.remove itself.
    let mut portal_iterations = 0;
    loop {
        let removals = crate::platform::portal::drain_portal_removals();
        if removals.is_empty() {
            break;
        }
        for removed in removals {
            st.arena.remove(removed);
        }
        portal_iterations += 1;
        if portal_iterations > 100 {
            eprintln!("[portal] removal loop exceeded 100 iterations — possible cyclic removal");
            break;
        }
    }
    // ── Drain queued event-handler removals (audit 2026-07-16, F1) ──
    // Element teardown can't reach the EventRegistry (owned by the window /
    // harness), so removals are queued on the AppContext and applied here.
    {
        let removed = crate::core::app_context::with_current_app(|app| app.take_handler_removals());
        if let Some(removed) = removed {
            for eid in removed {
                st.events.remove_element(eid);
            }
        }
    }
    dirty_registry::set_current_trigger(_prev_trig);
    perf::perf_end();

    // ── Harvest dirty registered BY deferred actions ──
    // Actions may register layout-class dirty (e.g. virtual-scroll remap
    // repositions pool slots via REPOSITION). The main drain already ran, and
    // recheck_dirty (pre-paint) discards layout_roots — without this second
    // harvest those flags would be consumed and cleared without ever reaching
    // taffy (audit 2026-07-16, Layer 3-1).
    if dirty_registry::has_pending_dirty() {
        perf::perf_begin(PerfPhase::ProcessDirty);
        let extra = frame_pipeline::process_dirty_phase(st.arena, root_id);
        dirty.paint_roots.extend(extra.paint_roots);
        dirty.processed_all.extend(extra.processed_all);
        dirty.layout_roots.extend(extra.layout_roots);
        dirty.has_measure |= extra.has_measure;
        dirty.root_flags |= extra.root_flags;
        perf::perf_end();
    }

    // ── SEAM 1: platform after-dirty hook; force_layout composition ──
    let force_layout = input.force_layout | hook.on_after_dirty(st.arena, st.events);

    // ── Layout (Incremental Taffy) ──
    perf::perf_begin(PerfPhase::Layout);
    frame_pipeline::set_phase(FramePhase::Layout);
    let mut structural = dirty_registry::drain_structurally_changed();
    // Multi-window filter: keep only elements in this arena.
    structural.retain(|eid| st.arena.get(*eid).is_some());

    let (new_flags, did_layout) = frame_pipeline::layout_phase(
        st.arena,
        st.taffy,
        st.events,
        root_id,
        input.size,
        structural,
        &dirty.processed_all,
        &dirty.layout_roots,
        dirty.has_measure,
        dirty.root_flags,
        input.is_first_frame,
        force_layout,
    );
    dirty.root_flags = new_flags;

    if did_layout {
        // Post-layout: track anchor bounds for portals on next frame.
        crate::platform::portal::update_portal_positions(st.arena, root_id);
        // Stretch anchored-dropdown children to portal width.
        crate::platform::portal::stretch_visible_anchored_portals(st.arena);
        // In-scroll-tree resolve — pending scrolls that were queued during layout.
        crate::widgets::bundle::scroll::resolve_pending_scrolls(st.arena, root_id);
        dirty_registry::clear_dirty_in_set(&dirty.processed_all, DirtyFlags::MEASURE_BIT);
        dirty_registry::clear_dirty_in_set(&dirty.processed_all, DirtyFlags::REPOSITION_BIT);
    }
    perf::perf_end();

    LayoutStage {
        root_id,
        processed_all: dirty.processed_all,
        paint_roots: dirty.paint_roots,
        root_flags: dirty.root_flags,
        did_layout,
    }
}

/// Phase 2: autofocus → animation → recheck → paint → clear → exits.
/// Called after the caller's SEAM 2 platform work. Takes a fresh `FrameState`.
pub fn drive_frame_paint(
    st: FrameState<'_>,
    fcx: &crate::core::frame_context::FrameContext<'_>,
    input: &FrameInput,
    mut stage: LayoutStage,
) -> FrameOutcome {
    use crate::core::frame_pipeline::{self, FramePhase};

    let root_id = stage.root_id;

    // ── Animation tick ──
    frame_pipeline::set_phase(FramePhase::Paint);
    perf::perf_begin(PerfPhase::Animation);
    frame_pipeline::animation_phase(st.arena, st.animations, input.now);
    perf::perf_end();

    // ── Re-check dirty after animation ──
    perf::perf_begin(PerfPhase::RecheckDirty);
    frame_pipeline::recheck_dirty_phase(
        st.arena,
        root_id,
        &mut stage.paint_roots,
        &mut stage.processed_all,
        &mut stage.root_flags,
    );
    perf::perf_end();

    // ── Paint (unified gate — divergence #1 frozen) ──
    perf::perf_begin(PerfPhase::Paint);
    let discrete_expired = crate::core::scheduler::drain_expired();
    let paint_needed =
        discrete_expired || stage.root_flags.has_repaint() || !stage.paint_roots.is_empty();
    let painted = paint_needed && !input.skip_paint;

    let mut commands = Vec::new();
    let mut text_areas = Vec::new();
    let mut repaint_ids = Vec::new();
    let mut backdrop_regions = Vec::new();

    if painted {
        // Capture repaint ids BEFORE paint clears the flags (CPU damage input).
        repaint_ids = stage
            .paint_roots
            .iter()
            .copied()
            .filter(|&eid| {
                st.arena
                    .get(eid)
                    .is_some_and(|el| el.dirty.get().has_repaint())
            })
            .collect();

        let viewport = crate::style::Rect::new(0.0, 0.0, input.size.width, input.size.height);
        let mut painter = crate::render::Painter::new(viewport);
        let mut ta: Vec<crate::render::wgpu::glyphon_bridge::TextAreaDesc> = Vec::new();
        let must_paint: HashSet<ElementId> = stage.processed_all.iter().copied().collect();

        // Drain cache evictions queued from callback context.
        for id in fcx.app.take_cache_evictions() {
            fcx.scene_cache.borrow_mut().remove(&id);
            fcx.subtree_cache.borrow_mut().remove(&id);
        }
        if fcx.app.take_clear_all_caches() {
            fcx.scene_cache.borrow_mut().clear();
            fcx.subtree_cache.borrow_mut().clear();
        }

        let tables_rc = st.arena.component_tables.clone();
        let ct_guard = tables_rc.borrow();
        let ct: &ComponentTables = &ct_guard;
        crate::render::paint_tree::paint_element_tree(
            fcx,
            ct,
            st.arena,
            root_id,
            &mut painter,
            &mut ta,
            input.scale_factor,
            (0.0, 0.0),
            false,
            input.bg,
            input.fg,
            crate::style::Rect::new(0.0, 0.0, input.size.width, input.size.height),
            crate::style::CornerRadii::ZERO,
            glam::Affine2::IDENTITY,
            0,
            &must_paint,
            1.0,
            input.highlight_mode,
        );
        commands = painter.take_commands();
        text_areas = ta;
        backdrop_regions = std::mem::take(&mut painter.backdrop_regions);
    }
    perf::perf_end();

    // ── Clear dirty flags after paint ──
    if painted {
        frame_pipeline::clear_dirty_phase(&stage.processed_all);
        // Age out shape-run-cache entries not touched in the last 8 painted
        // frames (audit 2026-07-17 round 2) — bounds the cosmic-text
        // word-level shaping cache while keeping scroll/remap runs warm.
        crate::render::wgpu::glyphon_bridge::trim_shape_run_cache(8);
    }

    // ── Process exit animations ──
    crate::core::element::process_exits(st.arena);

    frame_pipeline::set_phase(FramePhase::None);

    FrameOutcome {
        painted,
        commands,
        text_areas,
        backdrop_regions,
        repaint_ids,
        processed_all: stage.processed_all,
        paint_roots: stage.paint_roots,
        root_flags: stage.root_flags,
        did_layout: stage.did_layout,
    }
}

// ── Kinetic scroll state (moved from window.rs) ──────────────────────

/// Kinetic scroll state — fling inertia or animated scroll-to.
#[derive(Debug, Clone)]
pub(crate) enum ScrollKinetic {
    AnimatedTo {
        target: crate::style::Vec2,
        start: crate::style::Vec2,
        /// Anchor instant, set on the first frame that advances this
        /// animation. Progress is `f(now - anchor)` — pure function.
        anchor: Option<Instant>,
        duration_secs: f32,
    },
}
